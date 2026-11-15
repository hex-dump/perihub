#!/usr/bin/env python3
"""Reads a new device request or test report and says what the triage workflow should do.

  triage.py EVENT.json LATEST_VERSION   → {"comment": "..." or null, "labels": [...]}

A request for a device PeriHub already recognizes gets an answer with its level, and a test
report from an older version gets asked to try the latest one. Everything else is left to a person.
The issue text comes from anyone: it is only parsed, never run.
"""

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MARK = "<!-- perihub-triage -->"
DEVICES_MD = "https://github.com/hex-dump/perihub/blob/main/docs/devices.md"
TEST_FORM = "https://github.com/hex-dump/perihub/issues/new?template=device_test.yml"
LEVELS = {
    "supported": "✅ **supported**: tested on real hardware",
    "experimental": "🧪 **experimental**: recognized, not tested yet, changes at your own risk",
    "read_only": "👁 **read-only**: recognized and read, not changed",
    "by_memory_format": "supported or experimental depending on the format of its onboard memory, which PeriHub reads when it connects",
    "recognized": "recognized, but PeriHub sends nothing to it yet",
}


def fields(body: str) -> dict[str, str]:
    """The answers of an issue form: "### Label" followed by the value."""
    out, label = {}, None
    for line in (body or "").splitlines():
        if line.startswith("### "):
            label = line[4:].strip()
            out[label] = ""
        elif label:
            out[label] += line + "\n"
    return {k: v.strip() for k, v in out.items() if v.strip() and v.strip() != "_No response_"}


def usb_id(text: str) -> str | None:
    """046d:c094, 046D:C094, VID_046D&PID_C094 or 0x046d 0xc094, as vvvv:pppp."""
    m = re.search(r"\b([0-9a-fA-F]{4}):([0-9a-fA-F]{4})\b", text) or re.search(
        r"VID_([0-9a-fA-F]{4}).{0,3}PID_([0-9a-fA-F]{4})", text, re.I
    ) or re.search(r"0x([0-9a-fA-F]{4})\W+0x([0-9a-fA-F]{4})", text)
    return f"{m.group(1)}:{m.group(2)}".lower() if m else None


def version(text: str) -> tuple | None:
    m = re.search(r"(\d+)\.(\d+)\.(\d+)", text or "")
    return tuple(int(n) for n in m.groups()) if m else None


def brand_labels(usb: str | None, devices: list[dict]) -> list[str]:
    """"brand: X" for a vendor PeriHub knows, and "mouse" or "keyboard" for a device it knows."""
    if not usb:
        return []
    labels = []
    vendor = usb.split(":")[0]
    brands = sorted({d["brand"] for d in devices if any(i.startswith(vendor + ":") for i in d["usb_ids"])})
    if len(brands) == 1:
        labels.append(f"brand: {brands[0]}")
    known = [d for d in devices if usb in d["usb_ids"]]
    if known and known[0]["kind"] in ("mouse", "keyboard"):
        labels.append(known[0]["kind"])
    return labels


def triage(issue: dict, latest: str, devices: list[dict]) -> dict:
    labels = {label["name"] for label in issue.get("labels", [])}
    answers = fields(issue.get("body", ""))
    result = {"comment": None, "labels": brand_labels(usb_id(answers.get("USB ID (vendor:product)", "")), devices)}

    if "device request" in labels:
        given = answers.get("USB ID (vendor:product)", "")
        found = usb_id(given)
        if not found:
            result["comment"] = (
                "Thanks for the request! The USB ID is missing or not in the form `vvvv:pppp`, and it is what "
                "tells devices apart. On Linux, `lsusb` shows it; on Windows, Device Manager → the device → "
                "Properties → Details → Hardware IDs shows `VID_xxxx&PID_xxxx`. Could you add it?"
            )
            return result
        known = [d for d in devices if found in d["usb_ids"]]
        if known:
            d = known[0]
            result["comment"] = (
                f"Thanks! PeriHub {latest} already recognizes this device: **{d['brand']} {d['name']}** "
                f"(`{found}`), {LEVELS[d['level']]}. The [device list]({DEVICES_MD}) has the details.\n\n"
                "If it does not show up in PeriHub, please say so here, with the message PeriHub shows. "
                + (
                    "If something on its page does not work, a bug report helps."
                    if d["level"] == "supported"
                    else f"If it does, a [device test report]({TEST_FORM}) (**Open test form** on its page) is the most "
                    "useful thing you can send: one report moves the device up for everyone who has it."
                )
            )
        return result

    if labels & {"device test", "mouse test"}:
        used, newest = version(answers.get("PeriHub version", "")), version(latest)
        if used and newest and used < newest:
            result["comment"] = (
                f"Thanks for testing! This report is from PeriHub {'.'.join(map(str, used))}, and {latest} is out: "
                "it may already behave differently with this device. If you can, please try it and add a new test report "
                "here. The [changelog](https://github.com/hex-dump/perihub/blob/main/CHANGELOG.md) says what changed."
            )
    return result


def main() -> None:
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    event = json.loads(Path(sys.argv[1]).read_text())
    devices = json.loads((ROOT / "site/devices.json").read_text())["devices"]
    result = triage(event["issue"], sys.argv[2], devices)
    if result["comment"]:
        result["comment"] += f"\n\n<sub>Automatic answer from the repository's triage workflow.</sub>\n{MARK}"
    print(json.dumps(result))


if __name__ == "__main__":
    main()
