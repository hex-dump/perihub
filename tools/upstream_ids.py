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


def call_args(text: str, start: int) -> list[str] | None:
    """The arguments of the macro call whose opening parenthesis is text[start], split at the commas outside
    parentheses and strings. Comments inside the call are dropped; None when the parenthesis is never closed."""
    args, current, depth, i = [], [], 0, start
    while i < len(text):
        c = text[i]
        if c == '"' or c == "'":
            j = i + 1
            while j < len(text) and text[j] != c and text[j] != "\n":
                j += 2 if text[j] == "\\" else 1
            current.append(text[i : j + 1])
            i = j + 1
            continue
        if text.startswith("//", i) or text.startswith("/*", i):
            end = text.find("\n" if text[i + 1] == "/" else "*/", i)
            i = len(text) if end < 0 else end + (0 if text[i + 1] == "/" else 2)
            continue
        if c == "(":
            depth += 1
            if depth > 1:
                current.append(c)
        elif c == ")":
            depth -= 1
            if depth == 0:
                args.append("".join(current).strip())
                return args
            current.append(c)
        elif c == "," and depth == 1:
            args.append("".join(current).strip())
            current = []
        else:
            current.append(c)
        i += 1
    return None


def openrgb(root: Path):
    """HID detectors: REGISTER_HID_DETECTOR*("Name", function, VID, PID, …), and the REGISTER_HID_WRAPPED_DETECTOR*
    ones, with the #defines of their folder (names and ids). The USB serial detectors are left out: those
    devices are not HID, and their ids are often those of a generic serial chip.

    OpenRGB writes a call in many shapes: with spaces or a new line before the parenthesis or between the
    arguments, with a name that is a #define string, with ids that are #defines of the folder (or of a folder
    above it). The macros ending in _ONLY have no ids (they take any vendor), and HID_PID_ANY means any product:
    neither is an id. A call whose ids cannot be found is written on stderr, so that a form nobody foresaw is
    noticed instead of silently missing (a keyboard OpenRGB documents was told "no documentation" that way)."""
    controllers = root / "Controllers"
    files = [p for p in controllers.rglob("*") if p.suffix in (".h", ".cpp")]
    texts = {p: p.read_text(errors="replace") for p in files}
    define = re.compile(r'#define\s+(\w+)\s+(0x[0-9a-fA-F]{1,4}\b|"[^"\n]*")')
    by_dir, everywhere = {}, {}
    for p, text in texts.items():
        for name, literal in define.findall(text):
            by_dir.setdefault(p.parent, {})[name] = literal
            everywhere.setdefault(name, literal)
    head = re.compile(r"\bREGISTER_(?:HID|HID_WRAPPED)_DETECTOR(\w*)\s*\(")

    def literal(token: str, folder: Path) -> str | None:
        if re.fullmatch(r'0x[0-9a-fA-F]{1,4}|"[^"\n]*"', token):
            return token
        for d in (folder, *folder.parents):
            if token in by_dir.get(d, {}):
                return by_dir[d][token]
        return everywhere.get(token)

    for p in sorted(files):
        yield from wrapped(texts[p], p, controllers, literal)
        for m in head.finditer(texts[p]):
            if "_ONLY" in m.group(1):
                continue
            args = call_args(texts[p], m.end() - 1) or []
            ids = [literal(a, p.parent) for a in args[2:4]]
            ids = [i[2:] for i in ids if i and i.startswith("0x")]
            if len(ids) == 2:
                yield hexid(*ids), (literal(args[0], p.parent) or args[0]).strip('"'), str(p.parent.relative_to(controllers))
            elif not any(a.endswith("_ANY") for a in args[2:4]):
                line = texts[p].count("\n", 0, m.start()) + 1
                print(f"upstream_ids: OpenRGB: no ids in {p.relative_to(root)}:{line}: {m.group(0)[:-1].strip()}({', '.join(args[:4])})", file=sys.stderr)


# A detector macro a folder defines for itself, which registers one HIDDeviceDetector per variant of a product id:
# Wooting's REGISTER_WOOTING_DETECTOR(name, vid, pid) registers pid | 0x0000, pid | 0x0001 and pid | 0x0002 (one id per
# gamepad mode), through the macros XINP_PID(pid), DINP_PID(pid) and NONE_PID(pid). The reader of REGISTER_HID_DETECTOR
# never saw these calls, and the Wooting keyboards OpenRGB documents were missing from the baseline.
WRAPPER_DEF = re.compile(r"#define\s+(REGISTER_\w+)\s*\(\s*(\w+)\s*,\s*(\w+)\s*,\s*(\w+)\s*\)((?:[^\n]*\\\n)*[^\n]*)")
MASK_DEF = re.compile(r"#define\s+(\w+)\s*\(\s*(\w+)\s*\)\s*\(\s*\2\s*\|\s*(0x[0-9a-fA-F]{1,4})\s*\)")
DETECTOR = re.compile(r"\bHIDDeviceDetector\s+[\w#]+\s*\(")


def wrapped(text: str, path: Path, controllers: Path, literal):
    """The ids of the calls of the detector macros defined in this file (WRAPPER_DEF), one per HIDDeviceDetector of
    the macro: its product id is the call's, or the call's passed through a "(pid | 0x0002)" macro of the file."""
    masks = {name: int(value, 16) for name, _, value in MASK_DEF.findall(text)}
    for macro, name_arg, vid_arg, pid_arg, body in WRAPPER_DEF.findall(text):
        body = body.replace("\\\n", "\n")
        variants = []
        for d in DETECTOR.finditer(body):
            args = call_args(body, d.end() - 1) or []
            if len(args) < 4 or args[0] != name_arg or args[2] != vid_arg:
                continue
            pid = re.fullmatch(rf"(?:(\w+)\s*\(\s*)?{pid_arg}\s*\)?", args[3])
            if pid and (pid.group(1) is None or pid.group(1) in masks):
                variants.append(masks.get(pid.group(1), 0))
        if not variants:
            continue
        for m in re.finditer(rf"(?<!#define )\b{macro}\s*\(", text):
            args = call_args(text, m.end() - 1) or []
            ids = [literal(a, path.parent) for a in args[1:3]]
            if len(ids) != 2 or not all(i and i.startswith("0x") for i in ids):
                continue
            name = (literal(args[0], path.parent) or args[0]).strip('"')
            for mask in variants:
                yield hexid(ids[0][2:], f"{int(ids[1], 16) | mask:x}"), name, str(path.parent.relative_to(controllers))



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
