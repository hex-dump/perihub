#!/usr/bin/env python3
"""Reads a new device request or test report and says what the triage workflow should do.

  triage.py EVENT.json LATEST_VERSION [RELEASED_DEVICES.json]   → {"comment": "..." or null, "labels": [...]}

A request for a device the latest release recognizes gets an answer with its level, and a test
report from an older version gets asked to try the latest one. Everything else is left to a person.
RELEASED_DEVICES.json is site/devices.json as of the latest release's tag: a device that is only on
main is not in it, and is left to the maintainer, since no published version has it. Without it,
site/devices.json is used for both.
The issue text comes from anyone: it is only parsed, never run.
"""

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MARK = "<!-- perihub-triage -->"
DEVICES_MD = "https://github.com/hex-dump/perihub/blob/main/docs/devices.md"
TEST_FORM = "https://github.com/hex-dump/perihub/issues/new?template=test_report.yml"
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


TYPES = ("device request", "test report")
# The brands that have a label of their own: the options of the "Brand" dropdown of the forms, and "Other".
BRANDS = ("Logitech", "SteelSeries", "Corsair", "Razer", "ASUS")
KINDS = ("mouse", "keyboard")  # "Receiver" has no label


def form_type(body: str) -> str | None:
    """The kind of issue a form made, from its headings, for when GitHub did not put the label on
    (a label that does not exist yet is dropped when the issue is made)."""
    headings = set(re.findall(r"^### (.+?)\s*$", body or "", re.M))
    if "What PeriHub shows correctly" in headings:
        return "test report"
    if "What the device can do in its official software" in headings:
        return "device request"
    return None


def form_labels(answers: dict[str, str], from_catalogue: list[str]) -> list[str]:
    """The kind of device and the brand: what the form's dropdowns say, else what the catalogue
    knows for the USB ID. A brand not in BRANDS ("Other") is the catalogue's brand if it has one,
    and "brand: Other" if not."""
    catalogue_brand = next((l for l in from_catalogue if l.startswith("brand: ")), None)
    catalogue_kind = next((l for l in from_catalogue if l in KINDS), None)
    kind = answers.get("Device type", "").strip().lower()
    kind = kind if kind in KINDS else catalogue_kind
    chosen = answers.get("Brand", "").strip()
    named = next((b for b in BRANDS if b.lower() == chosen.lower()), None)
    if named:
        brand = f"brand: {named}"
    elif chosen.lower() == "other":
        brand = catalogue_brand or "brand: Other"
    else:
        brand = catalogue_brand
    return [l for l in (brand, kind) if l]


def pasted(body: str) -> tuple[str, dict[str, str]] | None:
    """A report from PeriHub pasted as plain text (Copy test report) instead of sent with its form:
    the kind of issue it is, and the form fields it holds."""
    lines = [line.strip() for line in (body or "").splitlines()]
    start = next((i for i, line in enumerate(lines) if re.match(r"PeriHub \d+\.\d+\.\d+ on \S+", line)), None)
    if start is None:
        return None
    rest = [line for line in lines[start + 1 :] if line]
    second = rest[0] if rest else ""
    text = "\n".join(lines[start:])
    found = {"PeriHub version": lines[start], "USB ID (vendor:product)": usb_id(text) or ""}
    if second.startswith(("Receiver: ", "Mouse: ")) or (second.startswith("Device: ") and ", USB ID " in second):
        kind = "test report"
    elif second.startswith("Device: "):
        kind = "device request"
    else:
        return None
    return kind, found


def kind_label(body: str) -> list[str]:
    """"mouse" or "keyboard", when a pasted report says what the device is."""
    m = re.search(r"^Kind: (Mouse|Keyboard)\b|^Device: .*\((mouse|keyboard)\)|^(Mouse): ", body or "", re.M | re.I)
    return [next(g for g in m.groups() if g).lower()] if m else []


def triage(issue: dict, latest: str, devices: list[dict], current: list[dict] | None = None) -> dict:
    """devices: those the latest release knows. current: those on main, for the labels."""
    current = devices if current is None else current
    labels = {label["name"] for label in issue.get("labels", [])}
    present = set(labels)
    answers = fields(issue.get("body", ""))
    pasted_kind = []
    if not labels & set(TYPES):
        kind = form_type(issue.get("body", ""))
        if not kind:
            found = pasted(issue.get("body", ""))
            if not found:
                return {"comment": None, "labels": []}
            kind, answers = found
            pasted_kind = kind_label(issue.get("body", ""))
        labels.add(kind)
    catalogue = brand_labels(usb_id(answers.get("USB ID (vendor:product)", "")), current)
    wanted = sorted(labels - present) + form_labels(answers, pasted_kind + catalogue)
    result = {"comment": None, "labels": [l for i, l in enumerate(wanted) if l not in present and l not in wanted[:i]]}

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

    if "test report" in labels:
        used, newest = version(answers.get("PeriHub version", "")), version(latest)
        if used and newest and used < newest:
            result["comment"] = (
                f"Thanks for testing! This report is from PeriHub {'.'.join(map(str, used))}, and {latest} is out: "
                "it may already behave differently with this device. If you can, please try it and add a new test report "
                "here. The [changelog](https://github.com/hex-dump/perihub/blob/main/CHANGELOG.md) says what changed."
            )
    return result


def main() -> None:
    if len(sys.argv) not in (3, 4):
        sys.exit(__doc__)
    event = json.loads(Path(sys.argv[1]).read_text())
    current = json.loads((ROOT / "site/devices.json").read_text())["devices"]
    released = json.loads(Path(sys.argv[3]).read_text())["devices"] if len(sys.argv) == 4 else current
    result = triage(event["issue"], sys.argv[2], released, current)
    if result["comment"]:
        result["comment"] += f"\n\n<sub>Automatic answer from the repository's triage workflow.</sub>\n{MARK}"
    print(json.dumps(result))


if __name__ == "__main__":
    main()
