#!/usr/bin/env python3
"""Reads a new device request or test report and says what the triage workflow should do.

  triage.py EVENT.json LATEST_VERSION [RELEASED_DEVICES.json [BRANCHES.json]]   → {"comment": "..." or null, "labels": [...]}
                                           plus {"confirm": {...}, "issue": N} when the report confirms something,
                                           or {"auto_add": "1532:00c1", "issue": N} when the id can be added without a person

A request for a device the latest release recognizes gets an answer with its level; one for a device
that any project lists is "ready to add": added directly if PeriHub has a driver for it, else by writing a driver from
the documentation, no captures needed. "Ready to add" means it is added to the next release as experimental (read-only
where the documentation does not allow checking it), with a test build earlier if one is made; nothing waits for a tester,
and a clean report from Test my device makes it supported. A request whose HID descriptors ("What PeriHub saw") show the
configuration collection of a family PeriHub already has a driver for (tools/platforms.py) is "ready to add" too; when the
collection and its reports match a family whose table takes a new id read only, the id is added without a person
({"auto_add": ..., "platform": ...}). One nothing recognizes is told what was checked (the catalogue, the projects PeriHub
learns from, its collections) and that a recording of its official software is the optional help ("needs captures"),
with no promise of a search nobody makes. A test report from an
older version gets asked to try the latest one. A test report from the latest version with nothing
wrong in it, for a device or a Logitech memory format nobody had confirmed, says so in "confirm"
({"usb": "1532:00b7"} or {"memory_format": [4, 255]}): the confirm workflow takes it from there, and
a person reviews what it opens. Anything doubtful confirms nothing. Everything else is left to a person.
RELEASED_DEVICES.json is site/devices.json as of the latest release's tag: a device that is only on
main is not in it, and its request is told it is already added and comes with the next release. Without it,
site/devices.json is used for both. A device is found by any id the catalogue gives it (its USB ids, the id it has behind
a Logitech receiver, its Bluetooth id: catalogue_ids), and the ids of a request are read from PeriHub's own report and
from the form's field, the report's first (request_ids). An id a project lists never gets "needs captures".
"Ready to add" with a project and vendor whose table a generator writes (MECHANICAL: OpenRazer's mice), or with a
full match of a family's collection (platforms.py, "sibling"), says {"auto_add": "1532:00c1"}: the auto-add workflow writes the support on a test branch, starts its test
build by itself and opens a pull request the maintainer merges after CI, and the answer says so. A new device goes into
main and the next release as experimental without waiting for a tester; a test build (a test/<name> branch) is for
trying it earlier, and its reports start "PeriHub 0.9.5 (test build test/x@abc1234) on ...".
BRANCHES.json ({"test/x": [devices of site/devices.json on that branch]}, made by
tools/test-branch-devices.sh) lets a clean report of such a build confirm a device that only its branch has (not
merged yet) on that branch: {"usb": ..., "branch": "test/x"}. A test build is never told to update (it is as new as its branch).
The issue text comes from anyone: it is only parsed, never run.
"""

import json
import re
import sys
from pathlib import Path
from urllib.parse import quote

sys.path.insert(0, str(Path(__file__).resolve().parent))
import platforms  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
CONFIRMED_RS = ROOT / "crates/perihub-core/src/confirmed.rs"
MARK = "<!-- perihub-triage -->"
LOGITECH = "046d"
DEVICES_MD = "https://github.com/hex-dump/perihub/blob/main/docs/devices.md"
TEST_FORM = "https://github.com/hex-dump/perihub/issues/new?template=01_test_report.yml"
CAPTURE_GUIDE = "https://github.com/hex-dump/perihub/blob/main/docs/captures.md"
UPSTREAM_URLS = {
    "libratbag": "https://github.com/libratbag/libratbag",
    "openrazer": "https://github.com/openrazer/openrazer",
    "rivalcfg": "https://github.com/flozz/rivalcfg",
    "OpenRGB": "https://gitlab.com/CalcProgrammer1/OpenRGB",
}
# The vendors whose protocol a project documents and PeriHub's driver speaks, by project. A project that
# lists an id of another vendor tells nothing PeriHub can build on: OpenRGB is for lights only, and its
# ids are of many protocols PeriHub has no driver for.
PROJECT_VENDORS = {"libratbag": {"046d"}, "openrazer": {"1532"}, "rivalcfg": {"1038"}}
# The (project, vendor) pairs whose table a generator writes from the project's source, so that
# tools/auto_add.py can add an id to it without a person (it keeps the same pairs, GENERATORS: test_triage.py
# checks they agree). Not the others: libratbag's Logitech devices have no generator (their drivers and
# models are written by hand), rivalcfg's table already has every id of its pinned commit (a newer one needs
# the pin bumped, by a person), and OpenRGB is for lights only.
MECHANICAL = {("openrazer", "1532")}
# Levels of a device PeriHub sends something to, which a driver exists for.
DRIVER_LEVELS = {"supported", "experimental", "read_only", "by_memory_format"}
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


# The ways a USB id is written in a request: 046d:c094 (lsusb, PeriHub), 046D:C094, 0x046d:0xc094, VID_046D&PID_C094
# (Windows' Hardware IDs), "VID 046d PID c094", 0x046d 0xc094, and macOS' System Information ("Product ID: 0xc094",
# "Vendor ID: 0x046d", in that order). Any case, with spaces around the colon.
ID_FORMS = (
    re.compile(r"(?<![0-9a-z])(?:0x)?([0-9a-f]{4})\s*:\s*(?:0x)?([0-9a-f]{4})(?![0-9a-z])", re.I),
    re.compile(r"\bVID[\s_:=#-]*(?:0x)?([0-9a-f]{4})\b.{0,3}?\bPID[\s_:=#-]*(?:0x)?([0-9a-f]{4})\b", re.I),
    re.compile(r"\b0x([0-9a-f]{4})\W+0x([0-9a-f]{4})\b", re.I),
)
VENDOR_FIELD = re.compile(r"\bVendor\s*ID\s*[:=]?\s*(?:0x)?([0-9a-f]{4})\b", re.I)
PRODUCT_FIELD = re.compile(r"\bProduct\s*ID\s*[:=]?\s*(?:0x)?([0-9a-f]{4})\b", re.I)
# A field that is nothing but the id, written with a space, a dash or a slash between the halves: "046d c094".
BARE_ID = re.compile(r"\s*`?(?:0x)?([0-9a-f]{4})[\s/_-]+(?:0x)?([0-9a-f]{4})`?\s*", re.I)


def usb_ids(text: str) -> list[str]:
    """Every USB id the text gives, as vvvv:pppp, in the order they come, without repeats."""
    text = text or ""
    found = [(m.start(), f"{m.group(1)}:{m.group(2)}".lower()) for form in ID_FORMS for m in form.finditer(text)]
    vendor, product = VENDOR_FIELD.search(text), PRODUCT_FIELD.search(text)
    if vendor and product:
        found.append((min(vendor.start(), product.start()), f"{vendor.group(1)}:{product.group(1)}".lower()))
    bare = BARE_ID.fullmatch(text)
    if bare:
        found.append((0, f"{bare.group(1)}:{bare.group(2)}".lower()))
    out = []
    for _, usb in sorted(found, key=lambda f: f[0]):
        if usb not in out:
            out.append(usb)
    return out


def usb_id(text: str) -> str | None:
    """The first USB id of the text (usb_ids), as vvvv:pppp: 046d:c094, 046D:C094, VID_046D&PID_C094, 0x046d 0xc094..."""
    ids = usb_ids(text)
    return ids[0] if ids else None


# The ids PeriHub's own report gives (app/src/lib/testReport.ts): "USB ID: 046d:c094 (USB)" in What PeriHub saw and in
# a mouse's report, "Device: X (mouse), USB ID 1532:0078", "Device: USB ID 046d:c547, on its ... receiver",
# "Receiver: LIGHTSPEED receiver, USB ID 046d:c539", and the
# id a Logitech device has behind its receiver, "Connection: LIGHTSPEED receiver 046d:c539, slot 1, wireless id 4087".
REPORT_ID = re.compile(r"^\s*(?:USB ID: |(?:Device|Receiver): (?:.*?, )?USB ID )([0-9a-f]{4}:[0-9a-f]{4})\b", re.I | re.M)
WIRELESS_ID = re.compile(r"\breceiver ([0-9a-f]{4}):[0-9a-f]{4}, slot \d+, wireless id ([0-9a-f]{4})\b", re.I)


def report_ids(report: str) -> list[str]:
    """The ids PeriHub's own report gives: the device's behind a receiver first, then the others in order."""
    wireless = [f"{v}:{p}".lower() for v, p in WIRELESS_ID.findall(report or "")]
    out = []
    for usb in wireless + [m.lower() for m in REPORT_ID.findall(report or "")]:
        if usb not in out:
            out.append(usb)
    return out


def request_ids(answers: dict[str, str], report: str = "") -> tuple[list[str], list[str]]:
    """The ids of a request or report: (those of PeriHub's own report, those of the form's USB ID field). Both are used;
    where they differ, PeriHub's report comes first: it is what the app read from the device, while the field is typed
    (or left as another device's id, issue #17)."""
    return report_ids(report), usb_ids(answers.get("USB ID (vendor:product)", ""))


def catalogue_ids(device: dict) -> dict[str, str]:
    """Every id the catalogue gives a device, and how it connects with it: its USB ids ("usb", the cable or its own
    dongle), the id of a Logitech device behind its receiver ("wireless", wireless_id: 046d:4087 for the G903 HERO) and
    its Bluetooth id ("bluetooth", bluetooth_id: 046d:b024 for the G604)."""
    out = {usb.lower(): "usb" for usb in device.get("usb_ids", [])}
    vendors = sorted({usb.split(":")[0].lower() for usb in device.get("usb_ids", [])})
    if not vendors and device.get("brand") == "Logitech":
        vendors = [LOGITECH]
    for key, how in (("bluetooth_id", "bluetooth"), ("wireless_id", "wireless")):
        product = (device.get(key) or "").lower()
        if re.fullmatch(r"[0-9a-f]{4}", product):
            for vendor in vendors:
                # The Bluetooth id is in usb_ids too (the G604's): it is still said to be the Bluetooth one.
                if how == "bluetooth" or f"{vendor}:{product}" not in out:
                    out[f"{vendor}:{product}"] = how
    return out


_INDEXES: dict[int, tuple[list[dict], int, dict[str, list[tuple[dict, str]]]]] = {}


def index(devices: list[dict]) -> dict[str, list[tuple[dict, str]]]:
    """Every id of the devices (catalogue_ids), with the devices that have it and how: made once per list."""
    cached = _INDEXES.get(id(devices))
    if cached and cached[0] is devices and cached[1] == len(devices):
        return cached[2]
    out: dict[str, list[tuple[dict, str]]] = {}
    for d in devices:
        for usb, how in catalogue_ids(d).items():
            out.setdefault(usb, []).append((d, how))
    if len(_INDEXES) > 16:
        _INDEXES.clear()  # a run uses two or three lists: the release's, main's, the branches'
    _INDEXES[id(devices)] = (devices, len(devices), out)
    return out


def knows(usb: str, device: dict) -> bool:
    """Whether the catalogue gives this device this id, in any of its forms (catalogue_ids)."""
    return usb.lower() in catalogue_ids(device)


def find(ids: list[str], devices: list[dict]) -> tuple[dict, str, str] | None:
    """The device of the catalogue the ids name: (device, the id, how it connects with it). A mouse or keyboard is
    preferred to a receiver (a request can give both the receiver's id and the device's), else the first id wins."""
    by_id = index(devices)
    hits = [(d, usb, how) for usb in ids for d, how in by_id.get(usb.lower(), [])]
    devices_first = [h for h in hits if h[0].get("kind") != "receiver"] + [h for h in hits if h[0].get("kind") == "receiver"]
    return devices_first[0] if devices_first else None


# The first line of a report: "PeriHub 0.9.5 on windows x86_64", and from a test build
# "PeriHub 0.9.5 (test build test/x@abc1234) on windows x86_64".
BUILD = r"(?: \(test build ([A-Za-z0-9._/-]+@[0-9a-f]{4,40})\))?"
REPORT_FIRST = re.compile(rf"PeriHub (\d+)\.(\d+)\.(\d+){BUILD} on \S+")


def version(text: str) -> tuple | None:
    """The version number at the start of "0.9.5", "0.9.5 (test build ...)" or "PeriHub 0.9.5 on ..."; a branch
    name later in the text, with digits in it, is not read."""
    m = re.match(r"\s*(?:PeriHub\s+)?v?(\d+)\.(\d+)\.(\d+)", text or "") or re.search(r"(\d+)\.(\d+)\.(\d+)", text or "")
    return tuple(int(n) for n in m.groups()) if m else None


def test_build(text: str) -> str | None:
    """The branch and commit ("test/x@abc1234") a report or a version field names as its test build, if any."""
    m = re.search(rf"PeriHub \d+\.\d+\.\d+{BUILD} on \S+", text or "") or re.match(rf"\s*\d+\.\d+\.\d+{BUILD}\s*$", text or "")
    return m.group(1) if m else None


def brand_labels(usb: str | None, devices: list[dict]) -> list[str]:
    """"brand: X" for a vendor PeriHub knows, and "mouse" or "keyboard" for a device it knows."""
    if not usb:
        return []
    labels = []
    vendor = usb.split(":")[0]
    brands = sorted({d["brand"] for i, having in index(devices).items() if i.startswith(vendor + ":") for d, _ in having})
    if len(brands) == 1:
        labels.append(f"brand: {brands[0]}")
    known = [d for d, _ in index(devices).get(usb, [])]
    if known and known[0]["kind"] in ("mouse", "keyboard"):
        labels.append(known[0]["kind"])
    return labels


def listed_by(usb: str, upstream: dict[str, list[str]]) -> list[str]:
    """The projects whose list of devices has this id."""
    return [project for project, ids in upstream.items() if usb in ids]


def driver_brand(vendor: str, devices: list[dict]) -> str | None:
    """The brand of the devices of this vendor that PeriHub has a driver for, if there are any."""
    brands = sorted(
        {d["brand"] for d in devices if d["level"] in DRIVER_LEVELS and any(i.startswith(vendor + ":") for i in d["usb_ids"])}
    )
    return brands[0] if brands else None


# What happens after "ready to add": the device is added to the next release as experimental, without waiting for a
# tester, and a clean report from Test my device makes it supported. A test build comes earlier when one is made. Only
# the devices auto-add.yml can write (AUTO_TEST_BUILD_STEPS) are added without a person: for the others a maintainer
# writes the support, and only the link of its test build comes by itself, so the answer does not promise more.
TEST_BUILD_STEPS = (
    "A maintainer adds it, as read-only or experimental, to the **next release**: it does not wait for a test. If a "
    "**test build** with it is made earlier, its link is posted on this issue automatically. Install it, connect the "
    "device, and try it with **Test my device**: it reads the device, and the report says which test build it is from. "
    "Once a clean report comes in, the device is confirmed and becomes **supported**. Nothing is needed from you for "
    "it to be added."
)


# The same, when the support is written by the repository's own workflow: nobody has to do anything.
AUTO_TEST_BUILD_STEPS = (
    "The support is being **written and built automatically** right now (a pull request and a **test build** on a "
    "`test/` branch): it goes into the **next release** as {level} once the maintainer merges it, without waiting "
    "for a test. The link to the test build is posted on this issue when it is ready. Install it, connect the device, "
    "and try it with **Test my device**: it reads the device, and the report says which test build it is from. Once a "
    "clean report comes in, the device is confirmed and becomes **supported**. Nothing is needed from you for it to be "
    "added. If the automatic step cannot add it, a maintainer takes over and the link still comes here."
)


ON_BRANCH_STEPS = (
    "A **test build** with it already exists, on the branch `{branch}`. To get it, open the "
    "[test build runs of that branch]({url}), open the latest successful run, and download the file for your system "
    "from the bottom of its page. Install it, connect the device, and try it with **Test my device**: it reads the "
    "device, and the report says which test build it is from. The device goes into the next release as read-only or "
    "experimental whether or not you try it; once a clean report comes in, it is confirmed and becomes **supported**."
)
TEST_BUILD_RUNS = "https://github.com/hex-dump/perihub/actions/workflows/test-build.yml?query=branch%3A{branch}"
def speaks_hidpp(usb: str, saw: str) -> bool:
    """Whether a Logitech device's "What PeriHub saw" block shows HID++ collections (platforms.py, "logitech": usage page
    ff00 with usage 0001 or 0002, or ff43): HID++ 2.0 is self-describing, the Logitech driver reads what the device has."""
    return platforms.level(platforms.BY_KEY["logitech"], usb, saw) is not None


COLLECTION = re.compile(r"usage page ([0-9a-f]{4}), usage [0-9a-f]{4}", re.I)


def vendor_interface(saw: str) -> bool | None:
    """Whether "What PeriHub saw" lists a vendor-defined collection (usage page ff00 to ffff), the channel a
    configurable device is set through; None when it lists no collections at all (nothing to go by)."""
    pages = [int(p, 16) for p in COLLECTION.findall(saw or "")]
    return any(p >= 0xFF00 for p in pages) if pages else None


# A device with only standard collections (mouse, keyboard, media keys): usually a plain device, nothing to set.
NO_VENDOR_INTERFACE = (
    "What PeriHub saw has only standard interfaces (mouse, keyboard, media keys) and no vendor-specific one, which "
    "is the channel configurable devices usually use, so there may be nothing PeriHub can change on it."
)
# How to send what the official software does: the recorder in the app (0.9.6 and newer), or the guide.
RECORD_STEPS = (
    "If its official software does change settings on it (DPI, lighting, buttons...), please say which program and "
    "what it changes. The way to add it is then a recording of that software: in PeriHub, **Send a report** → "
    "**Record the official software** on this device, and change each setting while it records (only the device's "
    f"own configuration traffic is kept). The [capture guide]({CAPTURE_GUIDE}) explains the same by hand."
)


def branch_with(usb: str, branches: dict[str, list[dict]] | None) -> str | None:
    """The first test/* branch (by name) that has this id."""
    having = sorted(b for b, ds in (branches or {}).items() if any(knows(usb, d) for d in ds))
    return having[0] if having else None


def auto_addable(
    usb: str, upstream: dict[str, list[str]], devices: list[dict], branches: dict[str, list[dict]] | None = None, saw: str = ""
) -> bool:
    """Whether tools/auto_add.py can add this id without a person, from what is known without the sources: a project
    whose table a generator writes (MECHANICAL) lists it, or the request's descriptors match the collection and reports
    of a family whose table takes a new id read only (platforms.py, a "full" match with "sibling"); and no device of the
    catalogue (the devices of main) or of a test branch has it already. Whether it really can be added is found out by
    the auto-add workflow (it checks the request again), which hands the issue to a person when it cannot."""
    if not re.fullmatch(r"[0-9a-f]{4}:[0-9a-f]{4}", usb):
        return False
    if find([usb], devices) or any(find([usb], ds) for ds in (branches or {}).values()):
        return False
    if any((project, usb.split(":")[0]) in MECHANICAL for project in listed_by(usb, upstream)):
        return True
    match = platforms.recognize(usb, saw)
    return bool(match and match.auto)


def checked(usb: str, saw: str, projects: list[str]) -> str:
    """What the bot looked at for a device it does not recognize, in a few sentences."""
    sources = ", ".join(f"[{p.replace('openrazer', 'OpenRazer')}]({url})" for p, url in UPSTREAM_URLS.items())
    out = [f"`{usb}` is not in PeriHub's catalogue (main included) or on a test branch."]
    # Only reached when no project lists it (next_steps answers the others): never "lists it, but...".
    out.append(f"None of the projects PeriHub takes facts from ({sources}) lists it.")
    seen = platforms.collections_of(saw)
    vendor = sorted({(i, p, u) for i, p, u in seen if p >= 0xFF00})
    if not seen:
        out.append(
            "The request has no HID descriptors to compare with the device families PeriHub supports: **Send a report** "
            "→ **Request support** in PeriHub (0.9.7 or newer) adds them."
        )
    elif vendor:
        names = "; ".join(f"usage page {p:04x}, usage {u:04x}" + (f" on interface {i}" if i >= 0 else "") for i, p, u in vendor[:4])
        out.append(
            f"Its vendor-defined collections ({names}) are not the configuration collection of any device family "
            "PeriHub has a driver for."
        )
    return " ".join(out)


def next_steps(
    usb: str,
    latest: str,
    devices: list[dict],
    upstream: dict[str, list[str]],
    branches: dict[str, list[dict]] | None = None,
    saw: str = "",
) -> tuple[str, str]:
    """What happens next for a device PeriHub does not know: the answer and the label.
    Only the fact that a project lists the id is used from it (or, for Logitech, that the device speaks HID++)."""
    projects = listed_by(usb, upstream)
    brand = driver_brand(usb.split(":")[0], devices)
    branch = branch_with(usb, branches)
    if branch:
        url = TEST_BUILD_RUNS.format(branch=quote(branch, safe=""))
        return (
            f"Thanks! `{usb}` is not in PeriHub {latest} yet, but it is already added on a test branch. "
            + ON_BRANCH_STEPS.format(branch=branch, url=url),
            "ready to add",
        )
    documented = [p for p in projects if brand and usb.split(":")[0] in PROJECT_VENDORS.get(p, ())]
    vendor = vendor_interface(saw)
    # libratbag's Logitech devices are set through HID++, on a vendor collection: a device with only standard ones
    # (046d:c077, a plain optical mouse, issue #95) has nothing for that driver, whatever libratbag lists.
    if usb.startswith("046d:") and vendor is False:
        documented = [p for p in documented if p != "libratbag"]
    if documented:
        project = documented[0]
        steps = AUTO_TEST_BUILD_STEPS.format(level="experimental") if auto_addable(usb, upstream, devices, branches) else TEST_BUILD_STEPS
        return (
            f"Thanks! `{usb}` is not in PeriHub {latest} yet, but [{project}]({UPSTREAM_URLS[project]}) lists it, and "
            f"PeriHub already has a driver for {brand} devices of that kind. So it **can be added directly**, as "
            f"**read-only or experimental**. {steps}",
            "ready to add",
        )
    match = platforms.recognize(usb, saw)
    if match and match.platform.key != "logitech":
        p = match.platform
        if p.found_by_collection:
            return (
                f"Thanks! `{usb}` has the collection of **{p.name}** ({platforms.signature(p)}), which PeriHub finds "
                f"by itself, whatever the USB id: see [{p.name}]({DEVICES_MD}#keyboards-with-qmk-firmware-vial-via). If "
                "PeriHub does not show it, please say here what it shows.",
                "ready to add",
            )
        if match.auto and auto_addable(usb, upstream, devices, branches, saw):
            linux = f" {p.sibling.linux}" if p.sibling.linux else ""
            return (
                f"Thanks! `{usb}` is not in PeriHub {latest} yet, but its configuration collection "
                f"({platforms.signature(p)}) and its reports are the ones of the **{p.name}** PeriHub already reads. So it "
                f"**is added automatically, read-only**.{linux} {AUTO_TEST_BUILD_STEPS.format(level='read-only')}",
                "ready to add",
            )
        why = (
            "the request has no report descriptor for that collection (PeriHub 0.9.7 and newer show it), so a maintainer "
            "checks it before it is added"
            if p.sibling
            else "the models of this family are written by a maintainer"
        )
        return (
            f"Thanks! `{usb}` is not in PeriHub {latest} yet, but it has the configuration collection of the "
            f"**{p.name}** PeriHub already has a driver for ({platforms.signature(p)}): {why}. {TEST_BUILD_STEPS}",
            "ready to add",
        )
    if brand and speaks_hidpp(usb, saw):
        return (
            f"Thanks! `{usb}` is not in PeriHub {latest} yet, but the device speaks **HID++** (Logitech's protocol, "
            f"which PeriHub already supports for {brand} devices): it describes its own features, so it **can be added "
            f"directly**, as **read-only or experimental**. {TEST_BUILD_STEPS}",
            "ready to add",
        )
    if vendor is False and not projects:
        return (
            f"Thanks! `{usb}` is not in PeriHub {latest}. {NO_VENDOR_INTERFACE} {RECORD_STEPS} Until then PeriHub "
            "sends nothing to it.",
            "needs captures",
        )
    # An id a project lists never gets "needs captures" nor "no documentation". The one case that is not "ready to
    # add": libratbag lists a Logitech id, but the request shows no vendor collection for its HID++ driver (046d:c077,
    # issue #95, which libratbag gives to the MX Vertical too): a person checks which device it is.
    if projects == ["libratbag"] and usb.startswith(LOGITECH + ":") and vendor is False:
        return (
            f"Thanks! `{usb}` is not in PeriHub {latest} yet. [libratbag]({UPSTREAM_URLS['libratbag']}) lists this id, "
            "but what PeriHub saw has only standard interfaces (mouse, keyboard, media keys) and not the HID++ one "
            "libratbag's Logitech driver talks to, so it may be another model with the same id, or a mouse with "
            "nothing to set. A maintainer checks it: **no USB captures are needed** for that. If its official software "
            "does change settings on it (DPI, lighting, buttons...), please say which program and what it changes.",
            "needs maintainer",
        )
    if projects:
        project = projects[0]
        return (
            f"Thanks! `{usb}` is not in PeriHub {latest} yet, but [{project}]({UPSTREAM_URLS[project]}) documents its "
            "protocol, so a new driver can be written from that documentation: **no USB captures are needed**. It may "
            "take a little longer than a device PeriHub already has a driver for. It will be "
            f"**read-only or experimental**. {TEST_BUILD_STEPS}",
            "ready to add",
        )
    return (
        f"Thanks for the request! What was checked automatically: {checked(usb, saw, projects)}\n\n"
        "The request stays open and counts. If you want to help, the one thing that lets PeriHub add a device like this "
        "is a recording of its official software changing each setting: in PeriHub, **Send a report** → **Record the "
        f"official software** on this device (the [capture guide]({CAPTURE_GUIDE}) explains the same by hand). It is "
        "optional. Until the device is added, PeriHub sends nothing to it.",
        "needs captures",
    )


def asked_device(reported: list[str], typed: list[str], devices: list[dict]) -> tuple[dict, str, str] | None:
    """The device of the catalogue a request or report is about: by PeriHub's report's ids if it has any, else by the
    form's; a receiver's id in the report and a mouse or keyboard's in the form is the mouse or keyboard."""
    hit = find(reported or typed, devices)
    if hit and hit[0].get("kind") == "receiver":
        behind = find(typed, devices)
        if behind and behind[0].get("kind") != "receiver":
            return behind
    return hit


def how_text(usb: str, how: str) -> str:
    """The id, and how the device has it when it is not its USB one."""
    if how == "wireless":
        return f"`{usb}`, the id it has behind its receiver"
    if how == "bluetooth":
        return f"`{usb}`, its Bluetooth id"
    return f"`{usb}`"


def known_answer(d: dict, usb: str, how: str, latest: str) -> str:
    """The answer to a request for a device the latest release knows, under any of its ids (catalogue_ids)."""
    if d.get("kind") == "receiver":
        return (
            f"Thanks! PeriHub {latest} already recognizes this receiver: **{d['brand']} {d['name']}** (`{usb}`), "
            f"{LEVELS[d['level']]}. The mouse or keyboard connected through it is reached through the receiver, under "
            f"its own name: the [device list]({DEVICES_MD}) says which models PeriHub knows.\n\n"
            "If your device does not show up in PeriHub, please say so here, with its model and the message PeriHub "
            f"shows. If it does, a [device test report]({TEST_FORM}) (**Open test form** on its page) is the most useful "
            "thing you can send."
        )
    return (
        f"Thanks! PeriHub {latest} already recognizes this device: **{d['brand']} {d['name']}** "
        f"({how_text(usb, how)}), {LEVELS[d['level']]}. The [device list]({DEVICES_MD}) has the details.\n\n"
        "If it does not show up in PeriHub, please say so here, with the message PeriHub shows. "
        + (
            "If something on its page does not work, a bug report helps."
            if d["level"] == "supported"
            else f"If it does, a [device test report]({TEST_FORM}) (**Open test form** on its page) is the most "
            "useful thing you can send: one report moves the device up for everyone who has it."
        )
    )


def next_release_answer(d: dict, usb: str, how: str, latest: str) -> str:
    """The answer to a request for a device that is on main but in no release yet: it is added, and comes with the next
    release (not "not in PeriHub", which is what these requests used to be told, nor nothing at all)."""
    return (
        f"Thanks! This device is **already added** to PeriHub: **{d['brand']} {d['name']}** ({how_text(usb, how)}), "
        f"{LEVELS[d['level']]}. It is not in PeriHub {latest}, the latest release, yet: it comes with the **next "
        "release**, and nothing is needed from you for that. Once it is out, a "
        f"[device test report]({TEST_FORM}) (**Test my device** on its page) is the most useful thing you can send: "
        "one clean report moves the device up for everyone who has it."
    )


def divergence(reported: list[str], typed: list[str], devices: list[dict], current: list[dict], latest: str) -> str:
    """A sentence for when the form's USB ID field names another id than PeriHub's own report: the answer is about the
    report's id, and says what the field's is, so that neither is taken for "not in the list" by mistake."""
    other = [usb for usb in typed if usb not in reported]
    if not reported or not other:
        return ""
    usb = other[0]
    hit = find([usb], devices)
    if hit:
        what = f"which PeriHub {latest} recognizes as **{hit[0]['brand']} {hit[0]['name']}**"
    elif find([usb], current):
        hit = find([usb], current)
        what = f"**{hit[0]['brand']} {hit[0]['name']}**, already added for the next release"
    else:
        what = "which is not in PeriHub's catalogue"
    return (
        f"\n\nThe form's USB ID field says `{usb}` ({what}), while PeriHub's own report says `{reported[0]}`: this "
        "answer goes by the report, which PeriHub read from the device. If the field is the right one, please say so here."
    )


def best_next_steps(
    ids: list[str],
    latest: str,
    devices: list[dict],
    upstream: dict[str, list[str]],
    branches: dict[str, list[dict]] | None,
    saw: str,
) -> tuple[str, tuple[str, str]]:
    """next_steps for the first of the ids that can be added (a test branch, a project, a family), else for the
    first one: a request with several ids is not told "needs captures" for one while another is documented."""
    answers = [(usb, next_steps(usb, latest, devices, upstream, branches, saw)) for usb in ids]
    return next((a for a in answers if a[1][1] == "ready to add"), answers[0])


TYPES = ("device request", "test report")
# The brands that have a label of their own: the options of the "Brand" dropdown of the forms, and "Other".
BRANDS = ("Logitech", "SteelSeries", "Corsair", "Razer", "ASUS")
KINDS = ("mouse", "keyboard")  # "Receiver" has no label
HEADSET_WORDS = re.compile(r"\b(headsets?|headphones?|earbuds?|microphones?)\b", re.I)
TELEPHONY_COLLECTION = re.compile(r"usage page 000b\b", re.I)
STANDARD_INPUT_COLLECTION = re.compile(r"usage page 0001,\s*usage 000[26]\b", re.I)


def is_headset_request(issue: dict, answers: dict[str, str], body: str) -> bool:
    """Whether a device request asks for a headset or audio device, which PeriHub does not support."""
    saw = answers.get("What PeriHub saw", "")
    # Pasted reports (from_form=False) keep their PeriHub lines in the issue body instead of a form field.
    report_text = saw or (body or "")
    if re.search(r"^Kind:\s*Headset\b", report_text, re.M | re.I):
        return True
    if TELEPHONY_COLLECTION.search(report_text) and not STANDARD_INPUT_COLLECTION.search(report_text):
        return True
    device_line = "\n".join(re.findall(r"^Device:\s*(.+)$", report_text, re.M | re.I))
    names = " ".join(part for part in (issue.get("title", ""), answers.get("Device model", ""), device_line) if part)
    return bool(HEADSET_WORDS.search(names))


def form_type(body: str) -> str | None:
    """The kind of issue a form made, from its headings, for when GitHub did not put the label on
    (a label that does not exist yet is dropped when the issue is made)."""
    headings = set(re.findall(r"^### (.+?)\s*$", body or "", re.M))
    if "What PeriHub shows correctly" in headings:
        return "test report"
    if "What the device can do in its official software" in headings:
        return "device request"
    return None


def form_labels(answers: dict[str, str], from_catalogue: list[str], reported: list[str] = ()) -> list[str]:
    """The kind of device and the brand. What the catalogue knows for the USB ID comes first, whatever the
    form says: a tester sometimes picks "Keyboard" for a mouse PeriHub knows (issue #97). For an id the
    catalogue does not have, the form's Device type comes next, before the app's own report ("Device: X
    (mouse)", "Kind: Mouse", in reported): the app fills that dropdown in from its guess (deviceKind in
    testReport.ts), so a different value is a person's correction, as when PeriHub took the SteelSeries Apex Pro
    for a mouse (issue #111) and the tester set Keyboard. A pasted report has no form: the app's kind is what
    is left. A brand nobody knows from the id is the dropdown's, and "brand: Other" for "Other"."""
    catalogue_brand = next((l for l in from_catalogue if l.startswith("brand: ")), None)
    catalogue_kind = next((l for l in from_catalogue if l in KINDS), None)
    reported_kind = next((l for l in reported if l in KINDS), None)
    kind = answers.get("Device type", "").strip().lower()
    kind = catalogue_kind or (kind if kind in KINDS else None) or reported_kind
    chosen = answers.get("Brand", "").strip()
    named = next((b for b in BRANDS if b.lower() == chosen.lower()), None)
    if catalogue_brand:
        brand = catalogue_brand
    elif named:
        brand = f"brand: {named}"
    elif chosen.lower() == "other":
        brand = "brand: Other"
    else:
        brand = None
    return [l for l in (brand, kind) if l]


def pasted(body: str) -> tuple[str, dict[str, str]] | None:
    """A report from PeriHub pasted as plain text (Copy test report) instead of sent with its form:
    the kind of issue it is, and the form fields it holds."""
    lines = [line.strip() for line in (body or "").splitlines()]
    start = next((i for i, line in enumerate(lines) if REPORT_FIRST.match(line)), None)
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


# What a report confirms. The lines it reads are the ones app/src/lib/testReport.ts writes (the vitest
# file testReport.triage.test.ts keeps them in step), the answers are the ones the test report form in
# .github/ISSUE_TEMPLATE asks for (test_triage.py keeps these in step with the form).
DEVICE_LINE = re.compile(r"Device: (.+) \((mouse|keyboard)\), USB ID ([0-9a-f]{4}:[0-9a-f]{4})(?:,|$)")
MEMORY_LINE = re.compile(
    r"Memory: model \d+, profile format (\d+), macro format \d+, \d+ sectors of (\d+) bytes, \d+ factory profiles$"
)
# Anything of these in a line the app wrote means something went wrong or was refused.
TROUBLE = re.compile(
    r"\b(errors?|problems?|unexpected|failed|failure|refused|denied|damaged|did not|could not|cannot|no answer|"
    r"not as documented|timed out|timeout)\b",
    re.I,
)
# The families whose form checkbox says what PeriHub shows matches the device.
OTHER_BRANDS = {
    "asus", "steelseries-mice", "razer", "razer-keyboard", "glorious", "roccat", "pulsar", "coolermaster", "nordic",
    "evga", "yjx", "wraith", "steelseries-apex",
}
SHOWS_OTHER = (
    "Other brands (ASUS, SteelSeries, Razer, Glorious, Roccat, Pulsar, Lamzu, Cooler Master, EVGA, YJX, Wraith, EVision, "
    "QMK keyboards): the DPI levels, polling rate, lighting and buttons PeriHub shows match the device"
)
SHOWS_CORSAIR = ("Firmware version", "Polling rate (report rate)")
SHOWS_CORSAIR_MOUSE = ("DPI (mice)",)
SHOWS_CORSAIR_LIGHTING = (
    "Corsair: experimental lighting showed the colour, and the device went back to its own lighting when it was turned off",
    "Corsair: every button or key kept working while experimental lighting was on",
)
SHOWS_MOUSE = (
    "Logitech mouse: profiles (number of profiles, which one is active)",
    "Logitech mouse: DPI levels and the default level",
)
SAVING_MOUSE = (
    "A change I saved took effect on the device",
    "It was still there after unplugging the device and plugging it back in",
)
# What mouse.rs writes: profile formats 1 to 4, in 255- or 256-byte sectors.
WRITABLE_FORMATS = (1, 2, 3, 4)
WRITABLE_SECTORS = (255, 256)
CHANGES_OK = ("Changes: available (experimental)", "Changes: off (read-only)")
TRANSCRIPT = "Transcript (every request and answer"
# The lines of the log of writes: the header counts the writes PeriHub refused (read-only mode), not a problem.
WRITES_HEADER = "Writes this session:"


def ticked(value: str) -> set[str]:
    """The boxes ticked in a checkboxes answer of an issue form."""
    return {m.group(1).strip() for m in re.finditer(r"^- \[[xX]\] (.+)$", value or "", re.M)}


def app_report(answers: dict[str, str]) -> list[str] | None:
    """The lines of the app's report: the Test report answer if it starts the way PeriHub's report does. A tester
    sometimes leaves that field as it was and pastes the report in another one (issue #117: "Messages and
    errors", after a sentence of their own, without a code block): then it is the lines from the first
    "PeriHub x.y.z on <os> <arch>" line of that field up to the "Writes this session" line, or the field's end.
    The placeholder of the Test report field has no such line and is never taken for a report."""
    lines = [line.rstrip() for line in answers.get("Test report", "").splitlines() if not line.startswith("```")]
    if lines and REPORT_FIRST.match(lines[0]):
        return lines
    for label, value in answers.items():
        if label == "Test report":
            continue
        lines = [line.rstrip() for line in value.splitlines() if not line.startswith("```")]
        start = next((i for i, line in enumerate(lines) if REPORT_FIRST.match(line.strip())), None)
        if start is None:
            continue
        lines = [line.strip() if i == 0 else line for i, line in enumerate(lines[start:])]
        end = next((i for i, line in enumerate(lines) if line.startswith(WRITES_HEADER)), len(lines) - 1)
        return lines[: end + 1]
    return None


def tested_memory_formats(source: str) -> set[tuple[int, int]]:
    """The memory formats listed in perihub-core/src/confirmed.rs."""
    table = re.search(r"pub const MEMORY: [^=]*= &\[(.*?)\];", source, re.S)
    return {(int(a), int(b)) for a, b in re.findall(r"memory\((\d+),\s*(\d+),\s*\d+\)", table.group(1))} if table else set()


def without_transcript(lines: list[str]) -> list[str] | None:
    """The report without its transcript (JSON, after a heading line), which holds no sentences of the app.
    None when the transcript does not end where a transcript does: a cut one hides what comes after it."""
    out, i = [], 0
    while i < len(lines):
        if not lines[i].startswith(TRANSCRIPT):
            out.append(lines[i])
            i += 1
            continue
        i += 1
        if i < len(lines) and lines[i].startswith("{") and lines[i].endswith("}"):
            i += 1
            continue
        while i < len(lines) and lines[i] != "}":
            i += 1
        if i == len(lines):
            return None
        i += 1
    return out


def trouble_in(lines: list[str]) -> bool:
    """A line that says something went wrong, among those after the first. A few lines say so in words
    that are no trouble: they are checked on their own, or are what a device that cannot be read back says."""
    lines = without_transcript(lines)
    if lines is None:
        return True
    for line in lines[1:]:
        if line.startswith(("Could not read:", "Changes:", "Support:", "Settings: this device cannot be read back", WRITES_HEADER)):
            continue
        if TROUBLE.search(line):
            return True
    return False


# The checklist of Test my device (app/src/lib/reportWizard.ts, checklistText): "What I tried:", then one line for
# each check the tester answered, "  worked: ..." or "  did not work: ...".
TRIED = "What I tried:"
NAME_CHECK = "The name and USB ID PeriHub shows are right"


def tried_clean(lines: list[str]) -> bool:
    """The tester went through Test my device's checklist and everything they answered worked: the name, and at
    least one thing PeriHub shows about the device (DPI, polling rate, lighting, keys...). The same as the form's
    box for the other brands, which the procedure does not tick (issue #97)."""
    if TRIED not in lines:
        return False
    tried = [line.strip() for line in lines[lines.index(TRIED) + 1 :] if line.strip()]
    worked = [line.removeprefix("worked: ") for line in tried if line.startswith("worked: ")]
    if any(line.startswith("did not work: ") for line in tried):
        return False
    return NAME_CHECK in worked and any("PeriHub shows" in w for w in worked if w != NAME_CHECK)


def confirm_device(lines: list[str], answers: dict[str, str], devices: list[dict]) -> dict | None:
    """A test report of a mouse or keyboard of another brand, or a Corsair one, nobody had confirmed."""
    found = DEVICE_LINE.match(lines[1]) if len(lines) > 1 else None
    if not found or found.group(3) != usb_id(answers.get("USB ID (vendor:product)", "")):
        return None
    name, kind, uid = found.groups()
    known = [d for d in devices if uid in d["usb_ids"]]
    if len(known) != 1:
        return None
    d = known[0]
    if d["level"] not in ("experimental", "read_only") or d["kind"] != kind or f"{d['brand']} {d['name']}" != name:
        return None
    boxes = ticked(answers.get("What PeriHub shows correctly", ""))
    if d["family"].startswith("corsair-"):
        needed = set(SHOWS_CORSAIR) | (set(SHOWS_CORSAIR_MOUSE) if kind == "mouse" else set())
        if "Experimental lighting: on" in lines:
            needed |= set(SHOWS_CORSAIR_LIGHTING)
        if "Could not read: nothing" not in lines:
            return None
    elif d["family"] in OTHER_BRANDS:
        needed = set() if tried_clean(lines) else {SHOWS_OTHER}
        if not any(line in CHANGES_OK for line in lines):
            return None
    else:
        return None
    if not needed <= boxes or trouble_in(lines):
        return None
    return {"usb": uid}


def confirm_mouse(lines: list[str], answers: dict[str, str], devices: list[dict], tested: set[tuple[int, int]]) -> dict | None:
    """A test report of a Logitech mouse with a memory format nobody had saved to, saved to over the cable."""
    fields_ = {line.split(": ", 1)[0]: line.split(": ", 1)[1] for line in lines[1:] if ": " in line}
    uid = fields_.get("USB ID", "")
    if uid != usb_id(answers.get("USB ID (vendor:product)", "")) or fields_.get("Connection") != "USB cable":
        return None
    known = [d for d in devices if uid in d["usb_ids"]]
    if len(known) != 1 or known[0]["level"] != "by_memory_format" or known[0]["brand"] != "Logitech":
        return None
    memory = next((m for m in map(MEMORY_LINE.fullmatch, lines) if m), None)
    if not memory:
        return None
    fmt = (int(memory.group(1)), int(memory.group(2)))
    if fmt in tested or fmt[0] not in WRITABLE_FORMATS or fmt[1] not in WRITABLE_SECTORS:
        return None
    # The app itself says PeriHub can write this memory and that nobody has saved to it.
    if fields_.get("Support") != "experimental (memory format not tested yet)":
        return None
    if fields_.get("Profile list") != "saved in the mouse" or not fields_.get("Firmware"):
        return None
    if fields_.get("Features", "not read") == "not read" or trouble_in(lines):
        return None
    if not set(SHOWS_MOUSE) <= ticked(answers.get("What PeriHub shows correctly", "")):
        return None
    if not set(SAVING_MOUSE) <= ticked(answers.get("Saving, if you tried it", "")):
        return None
    return {"memory_format": list(fmt)}


def is_test_build(answers: dict[str, str]) -> bool:
    """The report, or its version field, says it is from a test build: as new as its branch, whatever
    release came out since."""
    lines = app_report(answers)
    return bool(test_build(lines[0] if lines else "") or test_build(answers.get("PeriHub version", "")))


def branch_for(usb: str, header: str | None, branches: dict[str, list[dict]]) -> str | None:
    """The test/* branch a device that no release and not main has is on: the one the report's test build
    names if it has the device, else the only one that has it. Several without a name: none (a person decides)."""
    having = sorted(b for b, devices in branches.items() if any(usb in d["usb_ids"] for d in devices))
    named = (header or "").split("@")[0]
    if named in having:
        return named
    return having[0] if len(having) == 1 else None


def confirmation(
    answers: dict[str, str],
    latest: str,
    devices: list[dict],
    current: list[dict],
    tested: set[tuple[int, int]],
    branches: dict[str, list[dict]] | None = None,
) -> dict | None:
    """What a test report from PeriHub's form (the app's own, or the wizard's) confirms: {"usb": ...} or
    {"memory_format": [...]}, or None when anything is missing, doubtful or already confirmed. A device
    must be known to the latest release, and not supported yet there or on main (a confirmation that is
    waiting for a release is not made twice). A device that is in no release and not on main, only on a
    test/* branch (branches), is confirmed there: {"usb": ..., "branch": "test/x"}; the report is then
    from a test build, whose version is that of its branch and is not compared with the latest one."""
    lines = app_report(answers)
    if lines is None or len(lines) < 2:
        return None
    used, newest = version(lines[0]), version(latest)
    if not used or not newest or (used < newest and not is_test_build(answers)):
        return None
    if answers.get("Messages and errors") or answers.get("What is wrong or missing"):
        return None
    if lines[1].startswith("Mouse: "):
        return confirm_mouse(lines, answers, devices, tested)
    found = confirm_device(lines, answers, devices)
    if found and any(d["level"] == "supported" for d in current if found["usb"] in d["usb_ids"]):
        return None
    if found or not branches:
        return found
    # In no release and not on main: only a test branch can have it.
    uid = usb_id(answers.get("USB ID (vendor:product)", ""))
    if not uid or any(uid in d["usb_ids"] for d in [*devices, *current]):
        return None
    branch = branch_for(uid, test_build(lines[0]) or test_build(answers.get("PeriHub version", "")), branches)
    found = confirm_device(lines, answers, branches[branch]) if branch else None
    return {**found, "branch": branch} if found else None


def triage(
    issue: dict,
    latest: str,
    devices: list[dict],
    current: list[dict] | None = None,
    tested: set[tuple[int, int]] | None = None,
    upstream: dict[str, list[str]] | None = None,
    branches: dict[str, list[dict]] | None = None,
) -> dict:
    """devices: those the latest release knows. current: those on main, for the labels and for what is
    confirmed already. tested: the Logitech memory formats confirmed.rs lists (read from it by default).
    upstream: the ids other projects list, for what happens to a request. branches: the devices of each
    test/* branch, for a report of a test build of a device that is only there."""
    current = devices if current is None else current
    from_form = True
    labels = {label["name"] for label in issue.get("labels", [])}
    present = set(labels)
    answers = fields(issue.get("body", ""))
    # What the app's report says the device is, in a form or pasted (form_labels says which of it, the
    # catalogue and the form's dropdown counts).
    reported_kind = kind_label(issue.get("body", ""))
    if not labels & set(TYPES):
        kind = form_type(issue.get("body", ""))
        if not kind:
            found = pasted(issue.get("body", ""))
            if not found:
                return {"comment": None, "labels": []}
            kind, answers = found
            from_form = False
        labels.add(kind)
    reported, typed = request_ids(answers, answers.get("What PeriHub saw", "") or "\n".join(app_report(answers) or []) or issue.get("body", ""))
    hit = asked_device(reported, typed, current)
    catalogue = brand_labels(hit[1] if hit else (reported or typed or [None])[0], current)
    wanted = sorted(labels - present) + form_labels(answers, catalogue, reported_kind)
    result = {"comment": None, "labels": [l for i, l in enumerate(wanted) if l not in present and l not in wanted[:i]]}
    # A kind label the issue has that the device is not (the app's guess or the form's dropdown, whichever lost): taken off.
    right_kind = next((l for l in wanted if l in KINDS), None)
    wrong = sorted(l for l in present & set(KINDS) if right_kind and l != right_kind)
    if wrong:
        result["remove_labels"] = wrong

    if "device request" in labels:
        if is_headset_request(issue, answers, issue.get("body", "")):
            result["labels"] = [l for l in result["labels"] if l not in KINDS]
            if "wontfix" not in present and "wontfix" not in result["labels"]:
                result["labels"].append("wontfix")
            stale = sorted(present & (set(KINDS) | {"needs captures", "ready to add"}))
            if stale:
                result["remove_labels"] = sorted(set(result.get("remove_labels", [])) | set(stale))
            result["close"] = "not_planned"
            result["comment"] = (
                "Thanks for the request! PeriHub is only for **mice, keyboards, and their receivers** — headsets "
                "and other audio devices are out of scope, so this issue is being closed as not planned."
            )
            return result
        saw = answers.get("What PeriHub saw", "") or issue.get("body", "")
        reported, typed = request_ids(answers, saw)
        if not reported and not typed:
            result["comment"] = (
                "Thanks for the request! The USB ID is missing or not in the form `vvvv:pppp`, and it is what "
                "tells devices apart. On Linux, `lsusb` shows it; on Windows, Device Manager → the device → "
                "Properties → Details → Hardware IDs shows `VID_xxxx&PID_xxxx`. Could you add it?"
            )
            return result
        # PeriHub's own report first, where it has an id: the form's field is typed by hand.
        released_hit, main_hit = asked_device(reported, typed, devices), asked_device(reported, typed, current)
        note = divergence(reported, typed, devices, current, latest)
        if released_hit:
            result["comment"] = known_answer(*released_hit, latest) + note
        elif main_hit:
            result["comment"] = next_release_answer(*main_hit, latest) + note
        else:
            # The report's ids first; the field's too, so that a documented id is never told "needs captures".
            # (Not one the catalogue has: that one is named by the note, never told "not in PeriHub".)
            ids = reported + [usb for usb in typed if usb not in reported and not (find([usb], devices) or find([usb], current))]
            found, (comment, label) = best_next_steps(ids, latest, current, upstream or {}, branches, saw)
            if reported and found not in reported:
                note = (
                    f"\n\nPeriHub's own report says `{reported[0]}`, which nothing PeriHub checks knows; this answer is about "
                    f"`{found}`, the id the form's USB ID field gives. If the report's is the right one, please say so here."
                )
            result["comment"] = comment + note
            if label not in present:
                result["labels"].append(label)
            # The workflow that writes and builds it (auto-add.yml) is started with the id and this issue: only an id
            # of the device the descriptors are of (PeriHub's report's, when it has one).
            from_report = not reported or found in reported
            if label == "ready to add" and issue.get("number") and from_report and auto_addable(found, upstream or {}, current, branches, saw):
                result["auto_add"] = found
                result["issue"] = issue["number"]
                match = platforms.recognize(found, saw)
                if match and match.auto:
                    result["platform"] = match.platform.key
        return result

    if "test report" in labels:
        used, newest = version(answers.get("PeriHub version", "")), version(latest)
        if used and newest and used < newest and not is_test_build(answers):
            result["comment"] = (
                f"Thanks for testing! This report is from PeriHub {'.'.join(map(str, used))}, and {latest} is out: "
                "it may already behave differently with this device. If you can, please try it and add a new test report "
                "here. The [changelog](https://github.com/hex-dump/perihub/blob/main/CHANGELOG.md) says what changed."
            )
        # Only a report sent with its form has the answers a confirmation needs, not one pasted as text.
        if from_form:
            if tested is None:
                tested = tested_memory_formats(CONFIRMED_RS.read_text())
            confirms = confirmation(answers, latest, devices, current, tested, branches)
            if confirms and issue.get("number"):
                result["confirm"] = confirms
                result["issue"] = issue["number"]
    return result


def main() -> None:
    if len(sys.argv) not in (3, 4, 5):
        sys.exit(__doc__)
    event = json.loads(Path(sys.argv[1]).read_text())
    current = json.loads((ROOT / "site/devices.json").read_text())["devices"]
    # With BRANCHES.json too (5 arguments, as in the workflow) RELEASED_DEVICES.json is still the third.
    released = json.loads(Path(sys.argv[3]).read_text())["devices"] if len(sys.argv) >= 4 else current
    upstream = json.loads((ROOT / "tools/upstream-seen.json").read_text())
    branches = json.loads(Path(sys.argv[4]).read_text()) if len(sys.argv) == 5 else None
    result = triage(event["issue"], sys.argv[2], released, current, upstream=upstream, branches=branches)
    if result["comment"]:
        result["comment"] += f"\n\n<sub>Automatic answer from the repository's triage workflow.</sub>\n{MARK}"
    print(json.dumps(result))


if __name__ == "__main__":
    main()
