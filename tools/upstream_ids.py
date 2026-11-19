#!/usr/bin/env python3
"""The device ids other projects know and PeriHub has not looked at yet.

  upstream_ids.py SRC_DIR            Markdown list of the new ids (nothing when there are none)
  upstream_ids.py SRC_DIR --update   records every id found as looked at (tools/upstream-seen.json)

SRC_DIR holds clones of libratbag, openrazer, rivalcfg and OpenRGB. Only facts are read from them:
USB ids and device names. An id counts as new when it is neither in site/devices.json nor in
tools/upstream-seen.json, which lists what was already looked at, supported or not.
"""

import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SEEN = ROOT / "tools/upstream-seen.json"
URLS = {
    "libratbag": "https://github.com/libratbag/libratbag",
    "openrazer": "https://github.com/openrazer/openrazer",
    "rivalcfg": "https://github.com/flozz/rivalcfg",
    "OpenRGB": "https://gitlab.com/CalcProgrammer1/OpenRGB",
}


def hexid(vid: str, pid: str) -> str:
    return f"{int(vid, 16):04x}:{int(pid, 16):04x}"


def libratbag(root: Path):
    for f in sorted((root / "data/devices").glob("*.device")):
        text = f.read_text(errors="replace")
        name = re.search(r"^Name=(.*)$", text, re.M)
        driver = re.search(r"^Driver=(.*)$", text, re.M)
        for vid, pid in re.findall(r"(?:usb|bluetooth):([0-9a-fA-F]{4}):([0-9a-fA-F]{4})", text):
            yield hexid(vid, pid), name.group(1).strip() if name else f.stem, f"driver {driver.group(1).strip() if driver else '?'}"


def openrazer(root: Path):
    """The classes of the daemon's hardware files. A wireless or receiver variant is often a subclass
    of the wired one that sets only USB_PID (RazerViperV3ProWireless(RazerViperV3ProWired)), so the
    vendor id is the class's own, else its base class's, which may be in another file."""
    classes = {}  # name -> (base, vendor, product, name, file)
    for f in sorted((root / "daemon/openrazer_daemon/hardware").glob("*.py")):
        text = f.read_text(errors="replace")
        for block in re.split(r"\n(?=class \w+)", text):
            head = re.match(r"class (\w+)\s*(?:\(\s*(\w+)\s*\))?", block)
            if not head:
                continue
            vid = re.search(r"^\s+USB_VID\s*=\s*0x([0-9a-fA-F]+)", block, re.M)
            pid = re.search(r"^\s+USB_PID\s*=\s*0x([0-9a-fA-F]+)", block, re.M)
            doc = re.search(r'"""\s*Class for the (.+?)\s*"""', block, re.S)
            classes[head.group(1)] = (
                head.group(2),
                vid.group(1) if vid else None,
                pid.group(1) if pid else None,
                doc.group(1).strip() if doc else None,
                f.stem,
            )

    def vendor(name: str, seen=()) -> str | None:
        base, vid, _, _, _ = classes.get(name, (None, None, None, None, None))
        if vid or not base or base in seen or base not in classes:
            return vid
        return vendor(base, (*seen, name))

    for cls, (_, _, pid, doc, stem) in classes.items():
        vid = vendor(cls)
        if vid and pid:
            yield hexid(vid, pid), doc or cls, stem


def rivalcfg(root: Path):
    for f in sorted((root / "rivalcfg/devices").glob("*.py")):
        text = f.read_text(errors="replace")
        # Each model: its name, then its vendor and product ids.
        for name, vid, pid in re.findall(
            r'"name"\s*:\s*"([^"]+)"\s*,\s*"vendor_id"\s*:\s*0x([0-9a-fA-F]+)\s*,\s*"product_id"\s*:\s*0x([0-9a-fA-F]+)', text
        ):
            yield hexid(vid, pid), name, f.stem


def openrgb(root: Path):
    """HID detectors: REGISTER_HID_DETECTOR*("Name", function, VID, PID, …), with the #defines of their folder."""
    controllers = root / "Controllers"
    files = [p for p in controllers.rglob("*") if p.suffix in (".h", ".cpp")]
    define = re.compile(r"#define\s+(\w+)\s+0x([0-9a-fA-F]{1,4})\b")
    by_dir, everywhere = {}, {}
    for p in files:
        for name, value in define.findall(p.read_text(errors="replace")):
            by_dir.setdefault(p.parent, {})[name] = value
            everywhere.setdefault(name, value)
    call = re.compile(r'REGISTER_HID_DETECTOR\w*\(\s*"([^"]+)"\s*,\s*\w+\s*,\s*(\w+)\s*,\s*(\w+)')

    def value(token: str, folder: Path) -> str | None:
        if re.fullmatch(r"0x[0-9a-fA-F]{1,4}", token):
            return token[2:]
        for d in (folder, *folder.parents):
            if token in by_dir.get(d, {}):
                return by_dir[d][token]
        return everywhere.get(token)

    for p in sorted(files):
        for name, vid, pid in call.findall(p.read_text(errors="replace")):
            v, i = value(vid, p.parent), value(pid, p.parent)
            if v and i:
                yield hexid(v, i), name, str(p.parent.relative_to(controllers))


READERS = {"libratbag": libratbag, "openrazer": openrazer, "rivalcfg": rivalcfg, "OpenRGB": openrgb}


def commit(path: Path) -> str:
    return subprocess.run(["git", "-C", str(path), "rev-parse", "--short", "HEAD"], capture_output=True, text=True).stdout.strip()


def main() -> None:
    if len(sys.argv) not in (2, 3) or (len(sys.argv) == 3 and sys.argv[2] != "--update"):
        sys.exit(__doc__)
    src = Path(sys.argv[1])
    ours = {i for d in json.loads((ROOT / "site/devices.json").read_text())["devices"] for i in d["usb_ids"]}
    seen = json.loads(SEEN.read_text()) if SEEN.exists() else {}
    found = {name: sorted(set(read(src / name))) for name, read in READERS.items()}

    if len(sys.argv) == 3:
        SEEN.write_text(json.dumps({name: sorted({i for i, _, _ in ids}) for name, ids in found.items()}, indent=1) + "\n")
        print(f"{SEEN.relative_to(ROOT)}: " + ", ".join(f"{name} {len(ids)}" for name, ids in found.items()))
        return

    out = []
    for name, ids in found.items():
        new = {}
        for i, device, where in ids:
            if i not in ours and i not in seen.get(name, []):
                new.setdefault(i, (device, where))
        if new:
            out.append(f"### {name} ({URLS[name]}, commit `{commit(src / name)}`)\n")
            out += [f"- [ ] `{i}` {device} ({where})" for i, (device, where) in sorted(new.items())]
            out.append("")
    if out:
        print("\n".join(out))


if __name__ == "__main__":
    main()
