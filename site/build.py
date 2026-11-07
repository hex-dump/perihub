#!/usr/bin/env python3
"""Builds the website into OUT_DIR with the links and sizes of the latest release.

Usage: gh release view --json tagName,assets > release.json
       python3 site/build.py release.json OUT_DIR
"""

import json
import re
import shutil
import sys
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
    "SUMS": "SHA256SUMS.txt",
}


def size(n: int) -> str:
    return f"{n / 1_000_000:.1f} MB"


def main() -> None:
    release = json.loads(Path(sys.argv[1]).read_text())
    out = Path(sys.argv[2])

    values = {"VERSION": release["tagName"].removeprefix("v")}
    for key, suffix in ASSETS.items():
        found = [a for a in release["assets"] if a["name"].endswith(suffix)]
        if len(found) != 1:
            sys.exit(f"expected one asset ending with {suffix!r}, found {len(found)}")
        values[f"{key}_URL"] = found[0]["url"]
        values[f"{key}_SIZE"] = size(found[0]["size"])

    if out.exists():
        shutil.rmtree(out)
    (out / "screenshots").mkdir(parents=True)
    for page in SITE.glob("*.html"):
        html = re.sub(r"%%(\w+)%%", lambda m: values[m.group(1)], page.read_text())
        (out / page.name).write_text(html)
    for name in ("style.css", "main.js"):
        shutil.copy(SITE / name, out / name)
    for shot in (ROOT / "docs" / "screenshots").glob("*.png"):
        shutil.copy(shot, out / "screenshots" / shot.name)
    shutil.copy(ROOT / "docs" / "social-preview.png", out / "social-preview.png")
    shutil.copy(ROOT / "app" / "src-tauri" / "icons" / "64x64.png", out / "icon.png")
    # robots.txt is read only at the root of hex-dump.github.io, outside this site, so there is none here.
    # The sitemap is submitted by hand in Google Search Console and Bing Webmaster Tools.
    urls = "".join(f"  <url><loc>{BASE_URL}{'' if page == 'index.html' else page}</loc></url>\n" for page in sorted(p.name for p in SITE.glob("*.html")))
    (out / "sitemap.xml").write_text(
        f'<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n{urls}</urlset>\n'
    )
    print(f"PeriHub {values['VERSION']} → {out}")


if __name__ == "__main__":
    main()
