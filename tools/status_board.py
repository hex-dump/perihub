#!/usr/bin/env python3
"""The pinned "Device support status" issue: every open device request and where it stands.

  status_board.py RELEASED.json BRANCHES.json [--apply]

RELEASED.json is site/devices.json of the latest release, BRANCHES.json the devices of each test/* branch
(tools/test-branch-devices.sh); main's site/devices.json is read from the checkout. Without --apply the board is
printed; with it, the issue with the marker is edited (made, pinned and locked the first time), only when its text
changes. Issue text is only parsed (tools/triage.py), never run. Uses gh, with GH_TOKEN in the workflow.
"""

import json
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from triage import fields, next_steps, usb_id  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
REPO = "hex-dump/perihub"
MARK = "<!-- perihub-status-board -->"
TITLE = "Device support: where every request stands"
LEVEL_WORDS = {
    "supported": "supported (tested)",
    "experimental": "experimental",
    "read_only": "read-only",
    "by_memory_format": "supported or experimental (by memory format)",
    "recognized": "recognized",
}
# The groups, in the order they are shown: (key, heading, what it means for the reader).
GROUPS = [
    ("released", "✅ In the latest release", "Install the latest version and run **Test my device**: one clean report moves the device to supported for everyone."),
    ("next", "📦 Coming in the next release", "Written and merged; it ships as experimental with the next version."),
    ("branch", "🧪 Test build available", "Written on a test branch: the link to its test build is on the issue."),
    ("writing", "🛠️ Being written", "Another project documents its protocol: a maintainer is adding it."),
    ("recording", "🎙️ Needs a recording", "No public documentation: a recording of its official software (**Send a report** → **Record the official software**) is what lets it be added."),
    ("other", "💬 Being looked at", "A maintainer is looking into it."),
]


def name_of(issue: dict) -> str:
    title = issue["title"]
    for prefix in ("Device request: ", "Device request:"):
        if title.startswith(prefix):
            title = title[len(prefix):]
    return title.strip().replace("|", "/") or f"#{issue['number']}"


def matches(device: dict, issue: dict) -> bool:
    """The catalogue's device is the one asked for: its brand is in the title (a reporter sometimes gives the id of
    another device they have, issue #17)."""
    return device["brand"].lower() in issue["title"].lower() or device["name"].lower() in issue["title"].lower()


def place(
    issue: dict, released: list[dict], main: list[dict], branches: dict[str, list[dict]], upstream: dict[str, list[str]]
) -> tuple[str, str]:
    """The group of a device request, and a short note (level, branch...)."""
    labels = {label["name"] for label in issue["labels"]}
    answers = fields(issue.get("body") or "")
    usb = usb_id(answers.get("USB ID (vendor:product)", ""))
    if usb:
        for group, devices in (("released", released), ("next", main)):
            known = [d for d in devices if usb in d["usb_ids"]]
            if known and not matches(known[0], issue):
                return "other", "the USB ID given is of another device"
            if known:
                return group, LEVEL_WORDS.get(known[0]["level"], known[0]["level"])
        having = sorted(b for b, ds in branches.items() if any(usb in d["usb_ids"] for d in ds))
        if having:
            return "branch", f"`{having[0]}`"
    if "ready to add" in labels:
        return "writing", ""
    if "needs captures" in labels:
        return "recording", ""
    if usb:
        # Older requests, from before the labels: what the triage would say today.
        _, label = next_steps(usb, "", main, upstream, branches, answers.get("What PeriHub saw", ""))
        return ("writing" if label == "ready to add" else "recording"), ""
    return "other", "no USB ID yet"


def board(
    issues: list[dict],
    released: list[dict],
    main: list[dict],
    branches: dict[str, list[dict]],
    now: datetime,
    upstream: dict[str, list[str]] | None = None,
) -> str:
    rows: dict[str, list[str]] = {key: [] for key, _, _ in GROUPS}
    for issue in sorted(issues, key=lambda i: i["number"]):
        labels = {label["name"] for label in issue["labels"]}
        if "pull_request" in issue or "device request" not in labels:
            continue
        group, note = place(issue, released, main, branches, upstream or {})
        usb = usb_id(fields(issue.get("body") or "").get("USB ID (vendor:product)", "")) or "?"
        notes = [n for n in (note, "⏳ waiting for an answer" if "waiting for reporter" in labels else "") if n]
        rows[group].append(f"| #{issue['number']} | {name_of(issue)} | `{usb}` | {' · '.join(notes)} |")
    total = sum(len(r) for r in rows.values())
    out = [
        "Every open device request and where it stands, updated automatically when an issue or a release changes. "
        "To ask for a device, open **Send a report** in PeriHub (or the [device request form]"
        "(https://github.com/hex-dump/perihub/issues/new?template=device_request.yml)).",
        "",
        f"**{total} open requests.** A new device ships as **experimental** as soon as it is written; a clean "
        "report from **Test my device** makes it **supported**.",
    ]
    for key, heading, meaning in GROUPS:
        if not rows[key]:
            continue
        out += ["", f"### {heading} ({len(rows[key])})", "", meaning, "", "| Issue | Device | USB ID | Notes |", "|---|---|---|---|", *rows[key]]
    out += ["", f"<sub>Updated {now:%Y-%m-%d %H:%M} UTC by the Status board workflow.</sub>", MARK]
    return "\n".join(out)


def gh(*args: str) -> str:
    return subprocess.run(["gh", *args], capture_output=True, text=True, check=True).stdout


def main() -> None:
    args = [a for a in sys.argv[1:] if a != "--apply"]
    if len(args) != 2:
        sys.exit(__doc__)
    apply = "--apply" in sys.argv[1:]
    released = json.loads(Path(args[0]).read_text())["devices"]
    branches = json.loads(Path(args[1]).read_text())
    main_devices = json.loads((ROOT / "site/devices.json").read_text())["devices"]
    out = gh("api", "--paginate", f"repos/{REPO}/issues?state=open&labels=device%20request&per_page=100", "--jq", ".[]")
    issues = [json.loads(line) for line in out.splitlines() if line.strip()]
    upstream = json.loads((ROOT / "tools/upstream-seen.json").read_text())
    text = board(issues, released, main_devices, branches, datetime.now(timezone.utc), upstream)
    if not apply:
        print(text)
        return
    found = gh("issue", "list", "--repo", REPO, "--state", "open", "--search", f"in:title {TITLE}", "--json", "number,body,title")
    mine = [i for i in json.loads(found) if MARK in (i["body"] or "") and i["title"] == TITLE]
    # The time line changes every run: compare without it, so the issue is only edited when something moved.
    def same(a: str, b: str) -> bool:
        strip = lambda t: "\n".join(l for l in t.splitlines() if not l.startswith("<sub>Updated "))
        return strip(a) == strip(b)
    if mine:
        if same(mine[0]["body"], text):
            print(f"#{mine[0]['number']}: nothing changed")
            return
        gh("issue", "edit", str(mine[0]["number"]), "--repo", REPO, "--body", text)
        print(f"#{mine[0]['number']}: updated")
    else:
        url = gh("issue", "create", "--repo", REPO, "--title", TITLE, "--body", text).strip()
        number = url.rsplit("/", 1)[-1]
        gh("issue", "pin", number, "--repo", REPO)
        gh("issue", "lock", number, "--repo", REPO)
        print(f"#{number}: made, pinned and locked")


if __name__ == "__main__":
    main()
