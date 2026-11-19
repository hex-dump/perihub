#!/usr/bin/env python3
"""Builds the website into OUT_DIR with the links and sizes of the latest release.

Usage: gh release view --json tagName,assets > release.json
       python3 site/build.py release.json OUT_DIR

Besides the pages in site/, it writes one page per model (devices/<brand>-<model>.html, from
site/devices.json and site/templates/device.html), and checks the result: every internal link and
fragment resolves, every tag is closed, no placeholder is left, and the sitemap lists every page.
"""

import hashlib
import html
import json
import posixpath
import re
import shutil
import sys
from html.parser import HTMLParser
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SITE = ROOT / "site"
BASE_URL = "https://hex-dump.github.io/perihub/"

# Placeholder → end of the asset name. File names contain the version, so the
# releases/latest/download/<name> links cannot be written by hand.
ASSETS = {
    "EXE": "_x64-setup.exe",
    "MSI": "_x64_en-US.msi",
    "APPIMAGE": "_amd64.AppImage",
    "DEB": "_amd64.deb",
    "RPM": ".x86_64.rpm",
    "FLATPAK": "_x86_64.flatpak",
    # 0.8.4 has it without the extension.
    "SUMS": ("SHA256SUMS.txt", "SHA256SUMS"),
}
# Releases before 0.6.8 have no Flatpak. When the latest release lacks one of these, the page drops
# what is written between <!--if KEY--> and <!--end KEY-->.
OPTIONAL = {"FLATPAK"}

# What the level headings of the device list say (site/index.html), for the pages of the models.
# The texts are checked against index.html, so the two cannot drift apart.
LEVELS = {
    "experimental": (
        "Experimental",
        "test",
        "Recognized, but nobody has tried it with PeriHub yet. Changes are at your own risk, with a warning on screen. "
        "Saving to the device's memory asks you to tick a <b>No warranty</b> box first, and each save is backed up "
        "and undone after 15 seconds unless you keep it.",
    ),
    "read_only": ("Read-only", "ro", "Recognized: PeriHub shows its settings and changes nothing."),
    "by_memory_format": (
        "Depends on the memory format",
        "dep",
        "Supported, experimental or read-only, by the memory each mouse reports.",
    ),
    "supported": ("Supported", "ok", "Tested on real hardware: everything on its page works."),
    "recognized": ("Recognized", "ro", "Recognized, but PeriHub sends nothing to it yet."),
}
# The apps the main page names as the ones PeriHub replaces.
REPLACES = {"SteelSeries": "SteelSeries GG", "Logitech": "Logitech G HUB", "Corsair": "Corsair iCUE", "Razer": "Razer Synapse"}
KINDS = {"mouse": "Mouse", "keyboard": "Keyboard", "receiver": "Receiver"}
DEVICE_REQUEST = "https://github.com/hex-dump/perihub/issues/new?template=device_request.yml"


def size(n: int) -> str:
    return f"{n / 1_000_000:.1f} MB"


def esc(text: str) -> str:
    return html.escape(text, quote=True)


def plural(n: int, one: str, many: str) -> str:
    return f"{n} {one if n == 1 else many}"


# ---------------------------------------------------------------- pages from templates


def render(text: str, values: dict, missing: set, root: str) -> str:
    """Includes, the optional blocks, the placeholders and the fingerprints of the stylesheet and script."""
    for _ in range(2):  # a partial may include another
        text = re.sub(r"<!--include (\w[\w-]*)-->", lambda m: (SITE / "partials" / f"{m.group(1)}.html").read_text().strip("\n"), text)
    text = re.sub(
        r"<!--if (\w+)-->(.*?)<!--end \1-->",
        lambda m: "" if m.group(1) in missing else m.group(2),
        text,
        flags=re.S,
    )
    text = re.sub(r"%%(\w+)%%", lambda m: values[m.group(1)], text)
    # A fingerprint of the file in the link: after a change, browsers fetch the new stylesheet and
    # script instead of mixing the new page with the old ones from their cache.
    for name in ("style.css", "main.js"):
        stamp = hashlib.sha256((SITE / name).read_bytes()).hexdigest()[:10]
        text = text.replace(f'"{root}{name}"', f'"{root}{name}?v={stamp}"')
    return text


# ---------------------------------------------------------------- the models


def models(devices: list) -> list:
    """One entry per model: devices with the same brand and name (a wireless mouse, or a receiver with
    several ids) share a page, and their USB ids are put together."""
    pages: dict = {}
    for d in devices:
        m = pages.setdefault(d["page"], {**d, "usb_ids": [], "linux": [], "wireless_ids": []})
        m["usb_ids"] += [i for i in d["usb_ids"] if i not in m["usb_ids"]]
        if d.get("wireless_id") and d["wireless_id"] not in m["wireless_ids"]:
            m["wireless_ids"].append(d["wireless_id"])
        if d["linux"] not in m["linux"]:
            m["linux"].append(d["linux"])
        assert m["level"] == d["level"] and m["kind"] == d["kind"], f"{d['name']}: its entries disagree"
    return list(pages.values())


def full_name(m: dict) -> str:
    return m["name"] if m["name"].lower().startswith(m["brand"].lower()) else f"{m['brand']} {m['name']}"


def rows_of(index_html: str) -> dict:
    """What the main page says about each model: its row of the device list, by the names in data-models."""
    rows = {}
    strip = lambda s: re.sub(r"<!--.*?-->", "", s, flags=re.S)
    for attrs, body in re.findall(r'<tr class="dev"([^>]*)>(.*?)</tr>', index_html, re.S):
        names = [html.unescape(n) for n in re.search(r'data-models="([^"]*)"', attrs).group(1).split("|") if n]
        field = lambda cls: strip(m.group(1)) if (m := re.search(rf'class="{cls}">(.*?)</span>', body, re.S)) else ""
        row = {"name": re.sub(r"<[^>]+>", "", field("dev-name")), "sub": field("dev-sub"), "does": field("dev-does"), "count": len(names)}
        for n in names:
            rows[n] = row
    return rows


def connection(m: dict) -> str:
    if m["kind"] == "receiver":
        return "A USB receiver: wireless devices connect through it."
    if not m["usb_ids"]:
        return "Through a LIGHTSPEED receiver only."
    if m["wireless_ids"]:
        return "With its USB cable, or wirelessly through a LIGHTSPEED receiver."
    return "USB."


def opened(m: dict) -> str:
    out = []
    for access in m["linux"]:
        kind = access["kind"]
        if kind == "interface":
            text = f"Interface {access['interface']} of the device."
        elif kind == "whole":
            text = "Every interface of the device."
        elif kind == "receiver_hidpp":
            text = "The receiver's own HID++ interface."
        elif m["usb_ids"]:
            text = "Not opened yet: PeriHub's USB rule on Linux leaves it alone."
        else:
            text = "Through a receiver."
        if text not in out:
            out.append(text)
    return " ".join(out)


def spec(m: dict) -> str:
    rows = [("Type", KINDS[m["kind"]]), ("How it connects", connection(m))]
    if m["usb_ids"]:
        ids = ", ".join(f"<code>{i}</code>" for i in m["usb_ids"])
        rows.append(("USB id" if len(m["usb_ids"]) == 1 else "USB ids", ids))
    if m["wireless_ids"]:
        ids = ", ".join(f"<code>{i}</code>" for i in m["wireless_ids"])
        rows.append(("Id through a LIGHTSPEED receiver", ids))
    rows.append(("Opened on Linux", opened(m)))
    rows.append(("Windows", "No driver needed."))
    body = "".join(f"<div><dt>{k}</dt><dd>{v if v.startswith('<code>') else esc(v)}</dd></div>" for k, v in rows)
    return f'          <dl class="spec">{body}</dl>'


def does(m: dict, row: dict | None) -> str:
    out = []
    if row and m["level"] != "by_memory_format":  # its row only says "by the memory each mouse reports"
        # A row that holds several models says what PeriHub does with all of them.
        subject = f"<b>{row['name']}:</b> " if row["count"] > 1 else ""
        out.append(f"          <p>{subject}{row['does']}</p>")
    if m.get("note"):
        out.append(f"          <p><b>Particular to this model:</b> {esc(m['note'])}.</p>")
    if m["level"] == "by_memory_format":
        out.append(
            "          <p>For a Logitech G mouse with onboard memory, what counts is the format of the memory the mouse "
            "reports, not the model. PeriHub reads it every time the mouse is plugged in, and the mouse page shows the level. "
            '<a href="%%ROOT%%#logitech">Which Logitech G mice are supported?</a></p>'
        )
    if m["level"] == "experimental":
        text = LEVELS["experimental"][2].removeprefix("Recognized, but nobody has tried it with PeriHub yet. ")
        out.append(f'          <p class="small">{text}</p>')
    return "\n".join(out)


def lead(m: dict) -> str:
    full, kind = esc(full_name(m)), m["kind"]
    if m["level"] == "supported":
        return f"PeriHub supports the {full}: it was tested on real hardware, and everything PeriHub offers for it works, on Windows and on Linux."
    if m["level"] == "by_memory_format":
        return (
            f"PeriHub recognizes the {full}, a Logitech G mouse with onboard memory. Whether it is supported, experimental or "
            "read-only depends on the memory format it reports."
        )
    if m["level"] == "read_only":
        return f"PeriHub recognizes the {full}, a {kind}, on Windows and on Linux. It shows its settings and changes nothing."
    if m["level"] == "recognized":
        return f"PeriHub recognizes the {full}, a {kind}, but sends nothing to it yet."
    return f"PeriHub recognizes the {full}, a {kind}, on Windows and on Linux. Nobody has tried it with PeriHub yet."


def description(m: dict) -> str:
    full = full_name(m)
    state = {
        "supported": "tested on real hardware",
        "experimental": "experimental, nobody has tried it yet",
        "read_only": "read-only, nobody has tried it yet",
        "by_memory_format": "support depends on the memory format the mouse reports",
        "recognized": "recognized, not supported yet",
    }[m["level"]]
    replaces = f" instead of {REPLACES[m['brand']]}" if m["brand"] in REPLACES else ""
    ids = ", ".join(m["usb_ids"][:3]) + ("…" if len(m["usb_ids"]) > 3 else "")
    ids = f" USB id {ids}." if ids else ""
    return f"{full} in PeriHub, a free open-source app{replaces} for Windows and Linux: {state}.{ids}"


def help_box(m: dict, full: str) -> str:
    if m["level"] == "supported":
        return (
            '    <section class="section">\n      <div class="wrap">\n        <h2>Get PeriHub</h2>\n'
            f"        <p class=\"lead\">It is free, with no account and no telemetry. If something does not work on your {esc(full)}, "
            '<a href="https://github.com/hex-dump/perihub/issues">report a problem</a>.</p>\n'
            "        <!--include get-->\n      </div>\n    </section>"
        )
    if m["level"] == "recognized":
        return (
            '    <section class="section" id="help-test">\n      <div class="wrap">\n        <div class="help">\n'
            "          <h2>Not supported yet</h2>\n"
            f"          <p>PeriHub sends nothing to the {esc(full)} yet, so a test report cannot confirm it. If you own one, "
            "tell us what it does and which software it uses: a device request is how support starts.</p>\n"
            f'          <a class="btn primary" href="{DEVICE_REQUEST}"><span class="btn-text"><span class="btn-title">Request support</span></span></a>\n'
            "        </div>\n      </div>\n    </section>"
        )
    one = (
        "One report confirms the memory format, and that moves every mouse that uses it."
        if m["level"] == "by_memory_format"
        else "One report moves the model up for everyone."
    )
    return (
        '    <section class="section" id="help-test">\n      <div class="wrap">\n        <div class="help">\n'
        "          <h2>Own one? Help confirm it</h2>\n"
        f"          <p>Nobody has confirmed the {esc(full)} with PeriHub on real hardware yet. A test report takes about 30 seconds "
        f"and only reads the device. {one}</p>\n"
        '          <ol class="steps">\n'
        "            <li><b>Install PeriHub</b>, quit the maker's app (G HUB, GG, iCUE, Synapse…) and connect the device.</li>\n"
        "            <li>Open <b>Test my device</b> in the sidebar, pick the device and follow the steps.</li>\n"
        "            <li>Press <b>Open the form</b> and send the report. You need a free GitHub account.</li>\n"
        "          </ol>\n"
        "          <!--include get-->\n"
        '          <p class="small">Other models that need a test are on <a href="%%ROOT%%help-test.html">Wanted: testers</a>.</p>\n'
        "        </div>\n      </div>\n    </section>"
    )


def related(m: dict, all_models: list) -> str:
    """Up to eight neighbours in the same family, in the order of the device list."""
    same = [x for x in all_models if x["family"] == m["family"] and x["brand"] == m["brand"]]
    at = same.index(m)
    nearest = sorted((i for i in range(len(same)) if i != at), key=lambda i: abs(i - at))[:8]
    near = [same[i] for i in sorted(nearest)]
    if not near:
        return ""
    items = "\n".join(f'            <li><a href="%%ROOT%%{x["page"]}">{esc(x["name"])}</a></li>' for x in near)
    return (
        '    <section class="section">\n      <div class="wrap">\n'
        f"        <h2>Other {esc(m['brand'])} models handled the same way</h2>\n"
        f'        <ul class="models">\n{items}\n        </ul>\n'
        '        <div class="buttons">\n'
        '          <a class="btn" href="%%ROOT%%#devices"><span class="btn-text"><span class="btn-title">The full device list</span></span></a>\n'
        '          <a class="btn" href="%%ROOT%%help-test.html"><span class="btn-text"><span class="btn-title">Wanted: testers</span></span></a>\n'
        "        </div>\n      </div>\n    </section>"
    )


def wanted(all_models: list) -> str:
    """The models a test report would confirm, by level and brand, in the markup of the main page's list
    so that the same search finds them."""
    out = []
    groups = [("experimental", "test", "Experimental"), ("read_only", "ro", "Read-only"), ("by_memory_format", "dep", "Depends on the mouse")]
    for level, led, title in groups:
        found = [m for m in all_models if m["level"] == level]
        if not found:
            continue
        out.append(f'          <div class="level-group" data-group="{led}">')
        out.append(
            f'            <div class="level-head">\n              <h3><span class="led {led}" aria-hidden="true"></span>{title}</h3>\n'
            f"              <p>{LEVELS[level][2]}</p>\n            </div>"
        )
        out.append('            <div class="brand-rows">')
        for brand in sorted({m["brand"] for m in found}, key=str.lower):
            of = [m for m in found if m["brand"] == brand]
            counts = [plural(n, one, many) for kind, one, many in (("mouse", "mouse", "mice"), ("keyboard", "keyboard", "keyboards"), ("receiver", "receiver", "receivers")) if (n := sum(m["kind"] == kind for m in of))]
            items = "\n".join(
                f'                    <li class="dev" data-models="{esc(m["name"])}" data-keys=""><a href="{m["page"]}">{esc(m["name"])}</a><span class="dev-match" hidden></span></li>'
                for m in of
            )
            out.append(
                f'              <details class="brand-row">\n'
                f'                <summary><span class="brand-name">{esc(brand)}</span><span class="brand-peek">{", ".join(counts[:-1]) + " and " + counts[-1] if len(counts) > 1 else counts[0]}</span></summary>\n'
                f'                <div class="brand-body">\n                  <ul class="models">\n{items}\n                  </ul>\n                </div>\n'
                "              </details>"
            )
        out.append("            </div>\n          </div>")
    return "\n".join(out)


# ---------------------------------------------------------------- checks


class Scan(HTMLParser):
    """Collects ids and links, and finds tags that are not closed or closed in the wrong order."""

    VOID = {"meta", "link", "img", "input", "br", "hr", "source", "area", "base", "col", "wbr"}

    def __init__(self) -> None:
        super().__init__()
        self.ids, self.links, self.errors, self.stack = set(), [], [], []

    def _attrs(self, attrs) -> None:
        for k, v in attrs:
            if k == "id":
                self.ids.add(v)
            elif k in ("href", "src") and v:
                self.links.append(v)

    def handle_starttag(self, tag, attrs):
        self._attrs(attrs)
        if tag not in self.VOID:
            self.stack.append(tag)

    def handle_startendtag(self, tag, attrs):
        self._attrs(attrs)

    def handle_endtag(self, tag):
        if tag in self.VOID:
            return
        if not self.stack or self.stack[-1] != tag:
            self.errors.append(f"</{tag}> closes <{self.stack[-1] if self.stack else 'nothing'}> (line {self.getpos()[0]})")
            if tag in self.stack:
                while self.stack.pop() != tag:
                    pass
            return
        self.stack.pop()


def check(out: Path, device_pages: list, wanted_pages: list) -> None:
    problems = []
    scans = {}
    for page in sorted(out.rglob("*.html")):
        rel = page.relative_to(out).as_posix()
        text = page.read_text()
        if "%%" in text:
            problems.append(f"{rel}: a placeholder is left: {re.search(r'%%\w*%%?', text).group(0)}")
        scan = Scan()
        scan.feed(text)
        scan.close()
        problems += [f"{rel}: {e}" for e in scan.errors]
        if scan.stack:
            problems.append(f"{rel}: never closed: {scan.stack}")
        scans[rel] = scan
    for rel, scan in scans.items():
        for link in scan.links:
            if re.match(r"[a-z][a-z0-9+.-]*:", link):
                continue  # https:, mailto:
            path, _, fragment = link.partition("#")
            path = path.partition("?")[0]
            target = posixpath.normpath(posixpath.join(posixpath.dirname(rel), path)) if path else rel
            if path.endswith("/") or target == ".":
                target = posixpath.join(target, "index.html") if target != "." else "index.html"
            if not (out / target).is_file():
                problems.append(f"{rel}: {link} does not exist")
            elif fragment and fragment not in scans.get(target, Scan()).ids:
                problems.append(f"{rel}: {link} has no such id")
    main_page = (out / "index.html").read_text()
    for page in device_pages:
        if page not in scans:
            problems.append(f"{page} was not written")
        if f'href="{page}"' not in main_page:
            problems.append(f"index.html does not link to {page}")
    help_test = (out / "help-test.html").read_text()
    for page in wanted_pages:
        if f'href="{page}"' not in help_test:
            problems.append(f"help-test.html does not link to {page}")
    sitemap = (out / "sitemap.xml").read_text()
    for rel in scans:
        loc = BASE_URL + ("" if rel == "index.html" else rel)
        if f"<loc>{loc}</loc>" not in sitemap:
            problems.append(f"sitemap.xml lacks {loc}")
    if len(re.findall("<loc>", sitemap)) != len(scans):
        problems.append("sitemap.xml lists a page that does not exist")
    if problems:
        sys.exit("The site has problems:\n  " + "\n  ".join(problems))


# ---------------------------------------------------------------- main


def main() -> None:
    release = json.loads(Path(sys.argv[1]).read_text())
    out = Path(sys.argv[2])

    values = {"VERSION": release["tagName"].removeprefix("v")}
    missing = set()
    for key, suffixes in ASSETS.items():
        suffixes = suffixes if isinstance(suffixes, tuple) else (suffixes,)
        found = next(
            (f for f in ([a for a in release["assets"] if a["name"].endswith(s)] for s in suffixes) if f), []
        )
        suffix = suffixes[0]
        if not found and key in OPTIONAL:
            missing.add(key)
            continue
        if len(found) != 1:
            sys.exit(f"expected one asset ending with {suffix!r}, found {len(found)}")
        values[f"{key}_URL"] = found[0]["url"]
        values[f"{key}_SIZE"] = size(found[0]["size"])

    index_html = (SITE / "index.html").read_text()
    devices = json.loads((SITE / "devices.json").read_text())["devices"]
    all_models = models(devices)
    rows = rows_of(index_html)
    for level, (_, _, text) in LEVELS.items():
        if level not in ("supported", "recognized"):
            assert text in index_html, f"the level heading for {level} is not on the main page: update LEVELS in build.py"
    disclaimer = re.search(r'<p class="small disclaimer">(.*?)</p>', index_html, re.S).group(1)
    for brand in {d["brand"] for d in devices}:
        assert brand in disclaimer, f"the footer of index.html does not say it is not affiliated with {brand}"
    values.update(SITE_URL=BASE_URL, DISCLAIMER=disclaimer)

    if out.exists():
        shutil.rmtree(out)
    (out / "screenshots").mkdir(parents=True)
    (out / "devices").mkdir()

    def page_values(page: str, root: str, **extra) -> dict:
        url = BASE_URL + ("" if page == "index.html" else page)
        return {**values, "ROOT": root, "PAGE_URL": url, "PAGE_URL_SHORT": url.removeprefix("https://").rstrip("/"), **extra}

    wanted_models = [m for m in all_models if m["level"] not in ("supported", "recognized")]
    for page in SITE.glob("*.html"):
        extra = {}
        root = "./"
        if page.name == "help-test.html":
            extra = {
                "TITLE": "Wanted: testers for gaming mice and keyboards | PeriHub",
                "DESCRIPTION": (
                    f"{len(wanted_models)} mice, keyboards and receivers are recognized by PeriHub but nobody has tried them yet. "
                    "If you own one, a test report of about 30 seconds confirms it for everyone."
                ),
                "COUNT": str(len(wanted_models)),
                "WANTED": wanted(all_models),
            }
        elif page.name in ("index.html", "privacy.html"):
            root = ""  # these two write their own links
        text = render(page.read_text(), page_values(page.name, root, **extra), missing, root)
        (out / page.name).write_text(text)

    template = (SITE / "templates" / "device.html").read_text()
    for m in all_models:
        full = full_name(m)
        label, led, _ = LEVELS[m["level"]]
        row = rows.get(m["name"])
        assert row, f"{full}: no row of the device list on the main page says what PeriHub does with it"
        body = template
        for key, chunk in {
            "%%HELP%%": help_box(m, full),
            "%%DOES%%": does(m, row),
            "%%SPEC%%": spec(m),
            "%%RELATED%%": related(m, all_models),
        }.items():
            body = body.replace(key, chunk)
        extra = {
            "TITLE": esc(f"{full} on Windows and Linux | PeriHub"),
            "DESCRIPTION": esc(description(m)),
            "KIND": KINDS[m["kind"]],
            "BRAND": esc(m["brand"]),
            "FULL_NAME": esc(full),
            "LED": led,
            "LEVEL": label,
            "LEAD": lead(m),
        }
        text = render(body, page_values(m["page"], "../", **extra), missing, "../")
        (out / m["page"]).write_text(text)

    for name in ("style.css", "main.js", "devices.json"):
        shutil.copy(SITE / name, out / name)
    for shot in (ROOT / "docs" / "screenshots").glob("*.png"):
        shutil.copy(shot, out / "screenshots" / shot.name)
    shutil.copy(ROOT / "docs" / "social-preview.png", out / "social-preview.png")
    shutil.copy(ROOT / "app" / "src-tauri" / "icons" / "64x64.png", out / "icon.png")
    # robots.txt is read only at the root of hex-dump.github.io, outside this site, so there is none here.
    # The sitemap is submitted by hand in Google Search Console and Bing Webmaster Tools.
    pages = sorted(p.relative_to(out).as_posix() for p in out.rglob("*.html"))
    urls = "".join(f"  <url><loc>{BASE_URL}{'' if page == 'index.html' else page}</loc></url>\n" for page in pages)
    (out / "sitemap.xml").write_text(
        f'<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n{urls}</urlset>\n'
    )
    check(out, [m["page"] for m in all_models], [m["page"] for m in wanted_models])
    print(f"PeriHub {values['VERSION']} → {out}: {len(all_models)} model pages")


if __name__ == "__main__":
    main()
