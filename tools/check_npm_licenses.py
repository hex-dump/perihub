#!/usr/bin/env python3
"""The licenses of the npm packages that end up in the app (not the build tools), from
app/package-lock.json: the same list deny.toml allows for the Rust crates."""

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ALLOWED = {
    "MIT", "MIT-0", "Apache-2.0", "BSD-2-Clause", "BSD-3-Clause", "ISC", "Zlib", "0BSD", "Unlicense", "CC0-1.0", "MPL-2.0",
}


def ok(expression: str) -> bool:
    """An SPDX expression with OR: one allowed license is enough; with AND, all of them."""
    expression = expression.strip("() ")
    return any(all(part.strip("() ") in ALLOWED for part in alt.split(" AND ")) for alt in re.split(r"\s+OR\s+", expression))


def main() -> None:
    lock = json.loads((ROOT / "app/package-lock.json").read_text())
    bad, count = [], 0
    for path, package in lock["packages"].items():
        if not path or package.get("dev") or package.get("devOptional"):
            continue
        count += 1
        license_ = package.get("license") or "none"
        if not ok(license_):
            bad.append(f"{path.split('node_modules/')[-1]}: {license_}")
    if bad:
        sys.exit("npm packages in the app with a license that is not allowed:\n  " + "\n  ".join(bad))
    print(f"npm: {count} packages in the app, licenses ok")


if __name__ == "__main__":
    main()
