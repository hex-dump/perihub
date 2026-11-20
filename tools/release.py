#!/usr/bin/env python3
"""Release chores, run by hand or by the release workflows.

  release.py lint                  CHANGELOG.md uses the sections below (CI runs this)
  release.py bump VERSION [DATE]   new version everywhere; "## Unreleased" becomes "## VERSION — DATE"
  release.py notes VERSION         the release notes on GitHub, from that version's CHANGELOG section
  release.py check DIR VERSION     signatures, latest.json and SHA256SUMS.txt of the files in DIR
  release.py aur VERSION DEB DIR   PKGBUILD and .SRCINFO in DIR (a checkout of the aur branch) for that version, from its .deb

A version's section in CHANGELOG.md starts with a summary paragraph, then has some of these
sections, in this order. The release notes show each with its title.
"""

import base64
import hashlib
import json
import re
import subprocess
import sys
import tempfile
from datetime import date
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
REPO = "hex-dump/perihub"
CHANGELOG = ROOT / "CHANGELOG.md"
METAINFO = ROOT / "packaging/linux/io.github.hex_dump.perihub.metainfo.xml"

# CHANGELOG section → title in the release notes.
SECTIONS = {
    "New devices": "🖱️ New devices",
    "Added": "✨ New features",
    "Performance": "⚡ Performance",
    "Changed": "🔧 Changes",
    "Fixed": "🐛 Bug fixes",
    "Security": "🔒 Security",
    "Removed": "🗑️ Removed",
    "Known limitations": "⚠️ Known limitations",
}
# Versions written before the sections above were fixed: lint leaves them alone.
LEGACY = (0, 8, 4)

HEADING = re.compile(r"^## (Unreleased|(\d+\.\d+\.\d+) — (\d{4}-\d{2}-\d{2}))$")


def fail(message: str) -> None:
    sys.exit(f"release.py: {message}")


def version_tuple(v: str) -> tuple:
    return tuple(int(n) for n in v.split("."))


def check_version(v: str) -> str:
    if not re.fullmatch(r"\d+\.\d+\.\d+", v):
        fail(f"{v!r} is not a version like 0.8.5")
    return v


class Release:
    """One version's section of CHANGELOG.md."""

    def __init__(self, name: str, day: str | None, lines: list[str]):
        self.name, self.date = name, day
        self.summary: list[str] = []
        self.sections: list[tuple[str, list[str]]] = []
        for line in lines:
            if line.startswith("### "):
                self.sections.append((line[4:].strip(), []))
            elif self.sections:
                items = self.sections[-1][1]
                if line.startswith("- "):
                    items.append(line[2:].strip())
                elif line.strip() and items:
                    items[-1] += " " + line.strip()
            elif line.strip():
                self.summary.append(line.strip())

    @property
    def summary_text(self) -> str:
        return " ".join(self.summary)


def releases() -> list[Release]:
    found, current = [], None
    for line in CHANGELOG.read_text(encoding="utf-8").splitlines():
        m = HEADING.match(line)
        if m:
            current = (m.group(2) or "Unreleased", m.group(3), [])
            found.append(current)
        elif line.startswith("## "):
            fail(f"CHANGELOG.md: heading {line!r} is neither '## Unreleased' nor '## X.Y.Z — YYYY-MM-DD'")
        elif current:
            current[2].append(line)
    return [Release(*r) for r in found]


def find(version: str) -> Release:
    for r in releases():
        if r.name == version:
            return r
    fail(f"CHANGELOG.md has no section for {version}")


def lint() -> None:
    problems = []
    for r in releases():
        if r.name != "Unreleased" and version_tuple(r.name) <= LEGACY:
            continue
        names = [name for name, _ in r.sections]
        for name in names:
            if name not in SECTIONS:
                problems.append(f"{r.name}: section {name!r} is not one of {', '.join(SECTIONS)}")
        known = [n for n in names if n in SECTIONS]
        if known != sorted(known, key=list(SECTIONS).index):
            problems.append(f"{r.name}: sections out of order, expected {', '.join(n for n in SECTIONS if n in known)}")
        if len(set(names)) != len(names):
            problems.append(f"{r.name}: a section appears twice")
        if r.name != "Unreleased" and not r.summary:
            problems.append(f"{r.name}: no summary paragraph before the first section")
    if problems:
        fail("CHANGELOG.md:\n  " + "\n  ".join(problems))
    print("CHANGELOG.md: ok")


def plain(markdown: str) -> str:
    """Markdown without bold, code and links, for the AppStream metadata."""
    text = re.sub(r"\[([^\]]+)\]\([^)]+\)", r"\1", markdown)
    text = text.replace("**", "").replace("`", "")
    return text.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")


def replace_once(text: str, name: str, pattern: str, new, flags=0) -> str:
    result, n = re.subn(pattern, new, text, count=1, flags=flags)
    if n != 1:
        fail(f"{name}: {pattern!r} not found")
    return result


def bump(version: str, day: str) -> None:
    check_version(version)
    previous = next(r for r in releases() if r.name != "Unreleased").name
    if version_tuple(version) <= version_tuple(previous):
        fail(f"{version} is not after {previous}")
    unreleased = next((r for r in releases() if r.name == "Unreleased"), None)
    if not unreleased or not unreleased.sections:
        fail("CHANGELOG.md: nothing under '## Unreleased'")
    if not unreleased.summary:
        fail("CHANGELOG.md: write the summary paragraph under '## Unreleased' first (it is also the AppStream release text)")
    lint()

    # Every file is changed in memory first: nothing is written if one of them does not look as expected.
    files = {name: (ROOT / name).read_text(encoding="utf-8") for name in (
        "Cargo.toml", "Cargo.lock", "app/package.json", "app/src-tauri/tauri.conf.json", "CHANGELOG.md", "README.md", str(METAINFO.relative_to(ROOT)),
    )}
    # The udev rule in the README is fetched from the release tag, so it matches the version installed.
    files["README.md"] = replace_once(
        files["README.md"], "README.md", r"(perihub/)v\d+\.\d+\.\d+(/packaging/linux/70-perihub\.rules)", rf"\g<1>v{version}\g<2>"
    )
    files["README.md"] = replace_once(files["README.md"], "README.md", r"rule of release \d+\.\d+\.\d+", f"rule of release {version}")
    files["Cargo.toml"] = replace_once(files["Cargo.toml"], "Cargo.toml", r'(\[workspace\.package\]\nversion = )"[^"]+"', rf'\g<1>"{version}"')
    for crate in ("perihub-app", "perihub-core", "perihub-cli", "xtask"):
        files["Cargo.lock"] = replace_once(files["Cargo.lock"], "Cargo.lock", rf'(name = "{crate}"\nversion = )"[^"]+"', rf'\g<1>"{version}"')
    for name in ("app/package.json", "app/src-tauri/tauri.conf.json"):
        files[name] = replace_once(files[name], name, r'("version": )"[^"]+"', rf'\g<1>"{version}"')
    lock = json.loads((ROOT / "app/package-lock.json").read_text(encoding="utf-8"))
    lock["version"] = lock["packages"][""]["version"] = version
    files["app/package-lock.json"] = json.dumps(lock, indent=2, ensure_ascii=False) + "\n"
    files["CHANGELOG.md"] = replace_once(files["CHANGELOG.md"], "CHANGELOG.md", r"^## Unreleased$", f"## {version} — {day}", re.M)
    release = (
        f'    <release version="{version}" date="{day}">\n'
        f"      <description>\n        <p>{plain(unreleased.summary_text)}</p>\n      </description>\n    </release>\n"
    )
    meta = str(METAINFO.relative_to(ROOT))
    files[meta] = replace_once(files[meta], meta, r"(  <releases>\n)", lambda m: m.group(1) + release)
    for name, text in files.items():
        (ROOT / name).write_text(text, encoding="utf-8")
    print(f"Version {version} ({day}): Cargo.toml, Cargo.lock, package.json, package-lock.json, tauri.conf.json, CHANGELOG.md, README.md, metainfo")


def entry(text: str) -> str:
    """One CHANGELOG item: its bold title, with the rest folded away when it is long."""
    m = re.match(r"\*\*(.+?)\*\*\s*((?:\(\[[^\]]+\]\([^)]+\)(?:, \[[^\]]+\]\([^)]+\))*\))?)[\s,:;]*(.*)", text)
    if not m or len(text) < 160:
        return f"- {text}"
    title, links, rest = m.groups()
    head = f"- **{title}**" + (f" {links}" if links else "")
    rest = rest[:1].upper() + rest[1:]
    return f"{head}\n  <details><summary>Details</summary>\n\n  {rest}\n\n  </details>"


def anchor(heading: str) -> str:
    """GitHub's anchor for a markdown heading."""
    return re.sub(r"[^\w\- ]", "", heading.lower()).replace(" ", "-")


def git(*args: str) -> str:
    return subprocess.run(["git", *args], cwd=ROOT, capture_output=True, text=True, check=False).stdout.strip()


def notes(version: str) -> str:
    check_version(version)
    r = find(version)
    ordered = sorted(r.sections, key=lambda s: list(SECTIONS).index(s[0]) if s[0] in SECTIONS else len(SECTIONS))
    previous = next((p.name for p in releases() if p.name not in ("Unreleased", version) and version_tuple(p.name) < version_tuple(version)), None)
    out = [r.summary_text, ""]
    for name, items in ordered:
        out += [f"## {SECTIONS.get(name, name)}", ""] + [entry(i) for i in items] + [""]

    rules_changed = previous and git("tag", "--list", f"v{previous}") and git(
        "diff", "--name-only", f"v{previous}", f"v{version}" if git("tag", "--list", f"v{version}") else "HEAD", "--", "packaging/linux/*.rules"
    )
    v = version
    out += [
        "## 📥 Which file to download",
        "",
        "| System | File |",
        "|---|---|",
        f"| Windows 10/11 | `PeriHub_{v}_x64-setup.exe` (recommended) or `PeriHub_{v}_x64_en-US.msi` |",
        f"| Debian/Ubuntu | `PeriHub_{v}_amd64.deb` |",
        f"| Fedora/openSUSE | `PeriHub-{v}-1.x86_64.rpm` |",
        f"| Linux, any distribution with Flatpak | `PeriHub_{v}_x86_64.flatpak` |",
        f"| Linux, any distribution (including Arch/CachyOS) | `PeriHub_{v}_amd64.AppImage` |",
        "",
        "`SHA256SUMS.txt` has the checksum of every file. The Windows installer is not code-signed yet: if SmartScreen "
        "blocks it, click *More info → Run anyway*. Quit the maker's app (G HUB, GG, iCUE, Synapse…) before using PeriHub "
        "with its devices: two programs talking to the same device get in each other's way.",
        "",
        "## 🔄 Updating",
        "",
        "- **Windows**: PeriHub offers the update in the tray menu and in **Settings → Updates**.",
        "- **Linux**: install the new file as before."
        + (
            " **The udev rules have changed**: the .deb and .rpm install them; with the AppImage or the Flatpak, "
            "install them again as the [README](https://github.com/hex-dump/perihub#linux) says."
            if rules_changed
            else " The udev rules have not changed."
        ),
        "",
        "## 🧪 Help test your device",
        "",
        "On a device's page, **Open test form** fills in a [test report](https://github.com/hex-dump/perihub/issues/new?template=01_test_report.yml). "
        "Sending one only reads the device, and one report moves the device up for everyone who has it.",
        "",
        "---",
        "",
    ]
    heading = f"{version} — {r.date}"
    full = f"**Full changelog**: [CHANGELOG.md](https://github.com/{REPO}/blob/v{v}/CHANGELOG.md#{anchor(heading)})"
    if previous:
        full += f" · [all changes since {previous}](https://github.com/{REPO}/compare/v{previous}...v{v})"
    out.append(full)
    # Links relative to the repository, as CHANGELOG.md has them, point into it at the tag.
    text = re.sub(r"\]\((?!https?://|#)([^)]+)\)", rf"](https://github.com/{REPO}/blob/v{v}/\1)", "\n".join(out))
    return text + "\n"


def minisign_verify(public_key_b64: str, data: bytes, signature_b64: str) -> None:
    """Checks a Tauri updater signature: a minisign signature, base64-encoded, with its trusted comment."""
    key = base64.b64decode(base64.b64decode(public_key_b64).decode().splitlines()[1])
    lines = base64.b64decode(signature_b64).decode().splitlines()
    sig = base64.b64decode(lines[1])
    comment = lines[2].removeprefix("trusted comment: ").encode()
    global_sig = base64.b64decode(lines[3])
    if key[:2] != b"Ed" or sig[2:10] != key[2:10]:
        raise ValueError("signed with another key")
    message = hashlib.blake2b(data).digest() if sig[:2] == b"ED" else data
    with tempfile.TemporaryDirectory() as tmp:
        tmp = Path(tmp)
        der = bytes.fromhex("302a300506032b6570032100") + key[10:]
        (tmp / "key.der").write_bytes(der)
        for name, msg, s in (("file", message, sig[10:]), ("comment", sig[10:] + comment, global_sig)):
            (tmp / "msg").write_bytes(msg)
            (tmp / "sig").write_bytes(s)
            result = subprocess.run(
                ["openssl", "pkeyutl", "-verify", "-pubin", "-keyform", "DER", "-inkey", str(tmp / "key.der"),
                 "-rawin", "-in", str(tmp / "msg"), "-sigfile", str(tmp / "sig")],
                capture_output=True, text=True,
            )
            if result.returncode != 0:
                raise ValueError(f"the {name} signature does not match")


def check(directory: str, version: str) -> None:
    check_version(version)
    folder = Path(directory)
    files = sorted(p for p in folder.iterdir() if p.is_file() and p.name not in ("SHA256SUMS.txt", "SHA256SUMS"))
    names = [p.name for p in files]
    expected = [
        f"PeriHub_{version}_x64-setup.exe", f"PeriHub_{version}_x64_en-US.msi", f"PeriHub_{version}_amd64.AppImage",
        f"PeriHub_{version}_amd64.deb", f"PeriHub-{version}-1.x86_64.rpm", f"PeriHub_{version}_x86_64.flatpak", "latest.json",
    ]
    missing = [n for n in expected if n not in names]
    if missing:
        fail(f"missing: {', '.join(missing)}")

    public_key = json.loads((ROOT / "app/src-tauri/tauri.conf.json").read_text())["plugins"]["updater"]["pubkey"]
    signed = [p for p in files if (folder / (p.name + ".sig")).exists()]
    if len(signed) < 5:
        fail(f"only {len(signed)} signed files")
    for p in signed:
        try:
            minisign_verify(public_key, p.read_bytes(), (folder / (p.name + ".sig")).read_text().strip())
        except (ValueError, IndexError) as e:
            fail(f"{p.name}: {e}")
        print(f"signature ok: {p.name}")

    latest = json.loads((folder / "latest.json").read_text())
    if latest.get("version") != version:
        fail(f"latest.json says {latest.get('version')}, not {version}")
    for platform, entry_ in latest["platforms"].items():
        name = entry_["url"].rsplit("/", 1)[-1]
        if f"/releases/download/v{version}/" not in entry_["url"] or name not in names:
            fail(f"latest.json: {platform} points to {entry_['url']}")
        if entry_["signature"].strip() != (folder / (name + ".sig")).read_text().strip():
            fail(f"latest.json: {platform} has another signature than {name}.sig")
    print(f"latest.json ok: {version}, {len(latest['platforms'])} platforms")

    sums = "".join(f"{hashlib.sha256(p.read_bytes()).hexdigest()}  {p.name}\n" for p in files)
    (folder / "SHA256SUMS.txt").write_text(sums)
    print(f"SHA256SUMS.txt: {len(files)} files")


def aur(version: str, deb: str, folder: str) -> None:
    check_version(version)
    folder = Path(folder)
    deb_sum = hashlib.sha256(Path(deb).read_bytes()).hexdigest()
    # The LICENSE the PKGBUILD downloads is the one at the tag, the same as in the repository.
    if git("tag", "--list", f"v{version}"):
        license_bytes = subprocess.run(["git", "show", f"v{version}:LICENSE"], cwd=ROOT, capture_output=True, check=True).stdout
    else:
        license_bytes = (ROOT / "LICENSE").read_bytes()
    license_sum = hashlib.sha256(license_bytes).hexdigest()
    pkgbuild = folder / "PKGBUILD"
    text = pkgbuild.read_text()
    text = re.sub(r"^pkgver=.*$", f"pkgver={version}", text, flags=re.M)
    text = re.sub(r"^pkgrel=.*$", "pkgrel=1", text, flags=re.M)
    text = re.sub(r"sha256sums=\('[0-9a-f]{64}'\n(\s+)'[0-9a-f]{64}'\)", rf"sha256sums=('{deb_sum}'\n\1'{license_sum}')", text)
    pkgbuild.write_text(text)
    srcinfo = folder / ".SRCINFO"
    s = srcinfo.read_text()
    s = re.sub(r"(pkgver = ).*", rf"\g<1>{version}", s)
    s = re.sub(r"(pkgrel = ).*", r"\g<1>1", s)
    s = re.sub(r"(download/v)[\d.]+(/PeriHub_)[\d.]+(_amd64\.deb)", rf"\g<1>{version}\g<2>{version}\g<3>", s)
    s = re.sub(r"LICENSE-[\d.]+::(.*/v)[\d.]+(/LICENSE)", rf"LICENSE-{version}::\g<1>{version}\g<2>", s)
    sums = re.findall(r"\tsha256sums = [0-9a-f]{64}\n", s)
    if len(sums) != 2:
        fail(".SRCINFO: expected two sha256sums")
    s = s.replace(sums[0], f"\tsha256sums = {deb_sum}\n", 1).replace(sums[1], f"\tsha256sums = {license_sum}\n", 1)
    srcinfo.write_text(s)
    print(f"perihub-bin {version}: .deb {deb_sum[:12]}…, LICENSE {license_sum[:12]}…")


def main() -> None:
    args = sys.argv[1:]
    if args == ["lint"]:
        lint()
    elif len(args) in (2, 3) and args[0] == "bump":
        bump(args[1], args[2] if len(args) == 3 else date.today().isoformat())
    elif len(args) == 2 and args[0] == "notes":
        sys.stdout.write(notes(args[1]))
    elif len(args) == 3 and args[0] == "check":
        check(args[1], args[2])
    elif len(args) == 4 and args[0] == "aur":
        aur(args[1], args[2], args[3])
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main()
