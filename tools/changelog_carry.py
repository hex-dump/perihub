#!/usr/bin/env python3
"""The CHANGELOG of main with the entries a branch added, for tools/refresh-test-branches.sh.

  changelog_carry.py BASE MAIN BRANCH   → the new CHANGELOG.md on stdout

BASE, MAIN and BRANCH are CHANGELOG.md at the merge base, on main and on the branch. A test branch's entries
are about a device nobody has confirmed yet: whichever version heading the branch put them under, they are in
no release, so they go in "## Unreleased" (made at the top when main has none), each under its own section
("### New devices", ...), in the order release.py's lint wants. A line the branch added that is not an entry (a
summary sentence) goes before the first section. Everything of main is kept as it is.
"""

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from release import SECTIONS  # noqa: E402

UNRELEASED = "## Unreleased"


def added(base: list[str], main: list[str], branch: list[str]) -> tuple[list[str], dict[str, list[str]]]:
    """The summary lines and the entries (with their indented continuation lines), by section, that the branch
    has and neither the base nor main has."""
    known = set(base) | set(main)
    summary, entries, section, carrying = [], {}, None, False
    for line in branch:
        if line.startswith("## "):
            section, carrying = None, False
        elif line.startswith("### "):
            section, carrying = line[4:].strip(), False
        elif line.startswith("- "):
            carrying = line not in known and section is not None
            if carrying:
                entries.setdefault(section, []).append(line)
        elif line.startswith("  ") and line.strip():
            if carrying:
                entries[section].append(line)
        elif line.strip() and not line.startswith(("#", "<!--")) and line not in known and section is None:
            summary.append(line)
    return summary, entries


def carry(base: str, main: str, branch: str) -> str:
    lines = main.splitlines()
    summary, entries = added(base.splitlines(), lines, branch.splitlines())
    if not summary and not entries:
        return main
    heads = [i for i, line in enumerate(lines) if line.startswith("## ")]
    if heads and lines[heads[0]].strip() == UNRELEASED:
        start = heads[0]
        end = heads[1] if len(heads) > 1 else len(lines)
        block = lines[start + 1 : end]
    else:
        start = heads[0] if heads else len(lines)
        end = start
        block = []
    # The block as summary lines and sections, then the branch's lines added to them.
    intro, sections, current = [], {}, None
    for line in block:
        if line.startswith("### "):
            current = line[4:].strip()
            sections.setdefault(current, [])
        elif current is None:
            intro.append(line)
        elif line.strip():
            sections[current].append(line)
    intro = [l for l in intro if l.strip()] + summary
    for name, new in entries.items():
        sections.setdefault(name, []).extend(new)
    order = [n for n in SECTIONS if n in sections] + [n for n in sections if n not in SECTIONS]
    out = [UNRELEASED, ""]
    if intro:
        out += ["\n".join(intro), ""]
    for name in order:
        out += [f"### {name}", *sections[name], ""]
    text = lines[:start] + out + lines[end:]
    return "\n".join(text) + ("\n" if main.endswith("\n") else "")


if __name__ == "__main__":
    if len(sys.argv) != 4:
        sys.exit(__doc__)
    base, main, branch = (Path(p).read_text() for p in sys.argv[1:])
    sys.stdout.write(carry(base, main, branch))
