#!/usr/bin/env python3
"""Reads a new device request or test report and says what the triage workflow should do.

  triage.py EVENT.json LATEST_VERSION [RELEASED_DEVICES.json [BRANCHES.json]]   → {"comment": "..." or null, "labels": [...]}
                                           plus {"confirm": {...}, "issue": N} when the report confirms something,
                                           or {"auto_add": "1532:00c1", "issue": N} when the id can be added without a person

A request for a device the latest release recognizes gets an answer with its level; one for a device
that any project lists is "ready to add": added directly if PeriHub has a driver for it, else by writing a driver from
the documentation, no captures needed. "Ready to add" means it is added to the next release as experimental (read-only
where the documentation does not allow checking it), with a test build earlier if one is made; nothing waits for a tester,
and a clean report from Test my device makes it supported. One nobody lists is told a maintainer will look for documentation, and that
USB captures are optional help ("needs captures"). A test report from an
older version gets asked to try the latest one. A test report from the latest version with nothing
wrong in it, for a device or a Logitech memory format nobody had confirmed, says so in "confirm"
({"usb": "1532:00b7"} or {"memory_format": [4, 255]}): the confirm workflow takes it from there, and
a person reviews what it opens. Anything doubtful confirms nothing. Everything else is left to a person.
RELEASED_DEVICES.json is site/devices.json as of the latest release's tag: a device that is only on
main is not in it, and is left to the maintainer, since no published version has it. Without it,
site/devices.json is used for both.
"Ready to add" with a project and vendor whose table a generator writes (MECHANICAL: OpenRazer's mice) says
{"auto_add": "1532:00c1"}: the auto-add workflow writes the support on a test branch, starts its test
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

ROOT = Path(__file__).resolve().parent.parent
CONFIRMED_RS = ROOT / "crates/perihub-core/src/confirmed.rs"
MARK = "<!-- perihub-triage -->"
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


def usb_id(text: str) -> str | None:
    """046d:c094, 046D:C094, VID_046D&PID_C094 or 0x046d 0xc094, as vvvv:pppp."""
    m = re.search(r"\b([0-9a-fA-F]{4}):([0-9a-fA-F]{4})\b", text) or re.search(
        r"VID_([0-9a-fA-F]{4}).{0,3}PID_([0-9a-fA-F]{4})", text, re.I
    ) or re.search(r"0x([0-9a-fA-F]{4})\W+0x([0-9a-fA-F]{4})", text)
    return f"{m.group(1)}:{m.group(2)}".lower() if m else None


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
    brands = sorted({d["brand"] for d in devices if any(i.startswith(vendor + ":") for i in d["usb_ids"])})
    if len(brands) == 1:
        labels.append(f"brand: {brands[0]}")
    known = [d for d in devices if usb in d["usb_ids"]]
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
    "`test/` branch): it goes into the **next release** as experimental once the maintainer merges it, without waiting "
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
# A Logitech device that has a vendor collection on usage page ff00 with usage 0001 or 0002 (HID++ short and long
# reports) speaks HID++ 2.0, which is self-describing: the Logitech driver of PeriHub reads what the device has.
HIDPP_COLLECTION = re.compile(r"usage page ff00, usage 000[12]\b", re.I)


def speaks_hidpp(usb: str, saw: str) -> bool:
    """Whether a Logitech device's "What PeriHub saw" block shows HID++ report collections."""
    return usb.startswith("046d:") and bool(HIDPP_COLLECTION.search(saw or ""))


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
    having = sorted(b for b, ds in (branches or {}).items() if any(usb in d["usb_ids"] for d in ds))
    return having[0] if having else None


def auto_addable(usb: str, upstream: dict[str, list[str]], devices: list[dict], branches: dict[str, list[dict]] | None = None) -> bool:
    """Whether tools/auto_add.py can add this id without a person, from what is known without the sources: a project
    whose table a generator writes (MECHANICAL) lists it, and no device of the catalogue (the devices of main) or of
    a test branch has it already. Whether the generator really produces it is found out by the auto-add workflow,
    which hands the issue to a person when it does not."""
    if not re.fullmatch(r"[0-9a-f]{4}:[0-9a-f]{4}", usb):
        return False
    if any(usb in d["usb_ids"] for d in devices) or any(usb in d["usb_ids"] for ds in (branches or {}).values() for d in ds):
        return False
    return any((project, usb.split(":")[0]) in MECHANICAL for project in listed_by(usb, upstream))


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
        steps = AUTO_TEST_BUILD_STEPS if auto_addable(usb, upstream, devices, branches) else TEST_BUILD_STEPS
        return (
            f"Thanks! `{usb}` is not in PeriHub {latest} yet, but [{project}]({UPSTREAM_URLS[project]}) lists it, and "
            f"PeriHub already has a driver for {brand} devices of that kind. So it **can be added directly**, as "
            f"**read-only or experimental**. {steps}",
            "ready to add",
        )
    if brand and speaks_hidpp(usb, saw):
        return (
            f"Thanks! `{usb}` is not in PeriHub {latest} yet, but the device speaks **HID++** (Logitech's protocol, "
            f"which PeriHub already supports for {brand} devices): it describes its own features, so it **can be added "
            f"directly**, as **read-only or experimental**. {TEST_BUILD_STEPS}",
            "ready to add",
        )
    if vendor is False and not [p for p in projects if p != "libratbag"]:
        return (
            f"Thanks! `{usb}` is not in PeriHub {latest}. {NO_VENDOR_INTERFACE} {RECORD_STEPS} Until then PeriHub "
            "sends nothing to it.",
            "needs captures",
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
        f"Thanks! `{usb}` is not in PeriHub {latest}, and no public documentation of its protocol was found in the "
        "projects PeriHub learns from. A maintainer will look for documentation of it; you do not need to do anything "
        "for the request to count. If you are comfortable with it, a recording of its official software changing each "
        "setting would help a lot: in PeriHub, **Send a report** → **Record the official software** on this device (the "
        f"[capture guide]({CAPTURE_GUIDE}) explains the same by hand). It is optional and never required. Until the "
        "device is added, PeriHub sends nothing to it.",
        "needs captures",
    )


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
# What mouse.rs writes: profile formats 2, 3 and 4, in 255- or 256-byte sectors.
WRITABLE_FORMATS = (2, 3, 4)
WRITABLE_SECTORS = (255, 256)
CHANGES_OK = ("Changes: available (experimental)", "Changes: off (read-only)")
TRANSCRIPT = "Transcript (every request and answer"
# The lines of the log of writes: the header counts the writes PeriHub refused (read-only mode), not a problem.
WRITES_HEADER = "Writes this session:"


def ticked(value: str) -> set[str]:
    """The boxes ticked in a checkboxes answer of an issue form."""
    return {m.group(1).strip() for m in re.finditer(r"^- \[[xX]\] (.+)$", value or "", re.M)}


def app_report(answers: dict[str, str]) -> list[str] | None:
    """The lines of the Test report answer, if it starts the way PeriHub's report does."""
    lines = [line.rstrip() for line in answers.get("Test report", "").splitlines() if not line.startswith("```")]
    return lines if lines and REPORT_FIRST.match(lines[0]) else None


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
    catalogue = brand_labels(usb_id(answers.get("USB ID (vendor:product)", "")), current)
    wanted = sorted(labels - present) + form_labels(answers, catalogue, reported_kind)
    result = {"comment": None, "labels": [l for i, l in enumerate(wanted) if l not in present and l not in wanted[:i]]}
    # A kind label the issue has that the device is not (the app's guess or the form's dropdown, whichever lost): taken off.
    right_kind = next((l for l in wanted if l in KINDS), None)
    wrong = sorted(l for l in present & set(KINDS) if right_kind and l != right_kind)
    if wrong:
        result["remove_labels"] = wrong

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
        elif not any(found in d["usb_ids"] for d in current):
            # Only on main: no published version has it, so what to say is up to the maintainer.
            result["comment"], label = next_steps(
                found, latest, current, upstream or {}, branches, answers.get("What PeriHub saw", "")
            )
            result["labels"].append(label)
            # The workflow that writes and builds it (auto-add.yml) is started with the id and this issue.
            if label == "ready to add" and issue.get("number") and auto_addable(found, upstream or {}, current, branches):
                result["auto_add"] = found
                result["issue"] = issue["number"]
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
