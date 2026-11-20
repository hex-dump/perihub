#!/usr/bin/env python3
"""Tests of tools/changelog_carry.py: python3 -I -m unittest discover -s tools -p "test_*.py"
"""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from changelog_carry import carry  # noqa: E402

HEAD = "# Changelog\n\n<!-- sections -->\n\n"
BASE = HEAD + "## 0.9.4 — 2026-11-17\n\nSummary.\n\n### Fixed\n- old fix\n"
MAIN = HEAD + "## 0.9.5 — 2026-11-19\n\nRelease.\n\n### Added\n- main feature\n\n" + BASE[len(HEAD):]


class Carry(unittest.TestCase):
    def test_the_branch_entries_go_under_unreleased_whatever_heading_they_had(self):
        # The branch put its device under 0.9.5, which main has released since.
        branch = HEAD + "## 0.9.5 — 2026-11-19\n\nA device.\n\n### New devices\n- **Keyboard X**\n  - detail\n\n" + BASE[len(HEAD):]
        out = carry(BASE, MAIN, branch)
        self.assertTrue(out.startswith(HEAD + "## Unreleased\n\nA device.\n\n### New devices\n- **Keyboard X**\n  - detail\n\n## 0.9.5"))
        self.assertEqual(out.count("Keyboard X"), 1)
        self.assertIn("- main feature", out)

    def test_an_unreleased_section_of_main_is_kept_and_sections_stay_in_order(self):
        main = HEAD + "## Unreleased\n\n### Fixed\n- a fix on main\n\n" + MAIN[len(HEAD):]
        branch = HEAD + "## Unreleased\n\n### New devices\n- **Mouse Y**\n\n" + BASE[len(HEAD):]
        out = carry(BASE, main, branch)
        self.assertIn("## Unreleased\n\n### New devices\n- **Mouse Y**\n\n### Fixed\n- a fix on main\n\n## 0.9.5", out)

    def test_nothing_new_is_main_unchanged(self):
        self.assertEqual(carry(BASE, MAIN, BASE), MAIN)


if __name__ == "__main__":
    unittest.main()
