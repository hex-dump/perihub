#!/usr/bin/env python3
"""Tests of tools/status_board.py: python3 -I -m unittest discover -s tools -p "test_*.py"
"""

import sys
import unittest
from datetime import datetime, timezone
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import status_board  # noqa: E402

NOW = datetime(2026, 11, 20, tzinfo=timezone.utc)
RELEASED = [{"brand": "Razer", "name": "Viper", "level": "experimental", "usb_ids": ["1532:0078"]},
            {"brand": "SteelSeries", "name": "Apex 3 TKL", "level": "supported", "usb_ids": ["1038:1622"]}]
MAIN = RELEASED + [{"brand": "EVision", "name": "RGB Keyboard", "level": "experimental", "usb_ids": ["320f:505b"]}]
BRANCHES = {"test/g213": [{"usb_ids": ["046d:c336"]}]}


def request(number, title, usb, labels=()):
    body = f"### Device model\n\n{title}\n\n### USB ID (vendor:product)\n\n{usb}\n"
    return {"number": number, "title": f"Device request: {title}", "body": body,
            "labels": [{"name": n} for n in ("device request", *labels)]}


class Board(unittest.TestCase):
    def test_each_request_lands_where_it_stands(self):
        issues = [
            request(1, "Razer Viper", "1532:0078"),
            request(2, "Evision RGB Keyboard", "320f:505b", ["ready to add"]),
            request(3, "Logitech G213", "046d:c336", ["ready to add", "waiting for reporter"]),
            request(4, "Some Mouse", "dead:beef", ["needs captures"]),
            request(5, "Logitech MX Master 3S", "1038:1622"),
            {"number": 6, "title": "Bug", "body": "", "labels": [{"name": "bug"}]},
        ]
        text = status_board.board(issues, RELEASED, MAIN, BRANCHES, NOW, {})
        sections = {s.split("\n", 1)[0]: s for s in text.split("### ")[1:]}
        def section(start):
            return next(v for k, v in sections.items() if start in k)
        self.assertIn("| #1 | Razer Viper | `1532:0078` | experimental |", section("latest release"))
        self.assertIn("#2", section("next release"))
        self.assertIn("`test/g213` · ⏳ waiting for an answer", section("Test build"))
        self.assertIn("#4", section("Needs a recording"))
        # The id of another device (an Apex for an MX Master): not called supported.
        self.assertIn("the USB ID given is of another device", section("Being looked at"))
        self.assertNotIn("#6", text)
        self.assertIn("**5 open requests.**", text)
        self.assertTrue(text.endswith(status_board.MARK))


if __name__ == "__main__":
    unittest.main()
