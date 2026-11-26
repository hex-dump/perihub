"""Tests of tools/platforms.py: python3 -I -m unittest discover -s tools -p 'test_*.py'"""

import re
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import platforms  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent

# What PeriHub 0.9.7 wrote for the X68HE of issue #123 (the lines the recognition reads).
X68HE = """HID collections:
  interface 0, usage page 0001, usage 0006
  interface 1, usage page 0001, usage 0002
  interface 2, usage page ff1b, usage 0091
Report descriptors (lengths in bytes, with the id byte):
  interface 0, 66 bytes: usage page 0001, usage 0006: no id (in 8, out 1)
  interface 1, 66 bytes: usage page 0001, usage 0002: id 02 (in 7)
  interface 2, 40 bytes: usage page ff1b, usage 0091: id 01 (in 64, out 64)"""


class Parsing(unittest.TestCase):
    def test_collections_and_descriptors(self):
        self.assertIn((2, 0xFF1B, 0x0091), platforms.collections_of(X68HE))
        self.assertEqual(platforms.descriptors_of(X68HE)[(2, 0xFF1B, 0x0091)], {platforms.Report(1, 64, 64)})
        self.assertEqual(platforms.descriptors_of(X68HE)[(0, 1, 6)], {platforms.Report(0, 8, 1)})

    def test_several_collections_on_one_line_and_no_interface_number(self):
        saw = (
            "  interface 2, 50 bytes: usage page ff00, usage 0001: id 10 (in 7, out 7); usage page ff00, usage 0002: id 11 (in 20, out 20)\n"
            "  no interface number, usage page ff43, usage 0202\n"
            "  interface 1, 51 bytes: usage page ffc0, usage 0001: no id (in 64, out 64, feature 642)"
        )
        d = platforms.descriptors_of(saw)
        self.assertEqual(d[(2, 0xFF00, 2)], {platforms.Report(0x11, 20, 20)})
        self.assertEqual(d[(1, 0xFFC0, 1)], {platforms.Report(0, 64, 64, 642)})
        self.assertIn((-1, 0xFF43, 0x0202), platforms.collections_of(saw))

    def test_text_that_is_not_a_report_gives_nothing(self):
        self.assertEqual(platforms.collections_of("usage page ff1b; rm -rf /\ninterface x, usage page zz"), set())
        self.assertIsNone(platforms.recognize("2e3c:c365", ""))


class Recognize(unittest.TestCase):
    def test_the_x68he_is_the_w669_family_with_its_reports(self):
        m = platforms.recognize("2e3c:c365", X68HE)
        self.assertEqual((m.platform.key, m.level, m.auto), ("wraith", "full", True))

    def test_the_collection_alone_is_a_likely_match_a_person_confirms(self):
        saw = X68HE.split("Report descriptors")[0]
        m = platforms.recognize("2e3c:c365", saw)
        self.assertEqual((m.platform.key, m.level, m.auto), ("wraith", "collection", False))

    def test_other_reports_other_interface_or_vendor_are_no_match(self):
        self.assertIsNone(platforms.recognize("2e3c:c365", X68HE.replace("(in 64, out 64)", "(in 32, out 32)")))
        self.assertIsNone(platforms.recognize("2e3c:c365", X68HE.replace("interface 2", "interface 3")))
        self.assertIsNone(platforms.recognize("1234:c365", X68HE))

    def test_evision_and_yjx(self):
        evision = "  interface 1, usage page ff1c, usage 0092\n  interface 1, 30 bytes: usage page ff1c, usage 0092: id 04 (in 64, out 64)"
        self.assertEqual(platforms.recognize("0c45:8513", evision).platform.key, "evision")
        self.assertTrue(platforms.recognize("0c45:8513", evision).auto)
        yjx = "  interface 2, 30 bytes: usage page ff01, usage 0010: no id (in 64, out 64)"
        self.assertTrue(platforms.recognize("a8a5:2256", yjx).auto)
        self.assertIsNone(platforms.recognize("0b05:1234", yjx))  # ASUS uses ff01 too: the vendor tells them apart

    def test_logitech_and_qmk_are_recognized_but_never_added_by_themselves(self):
        hidpp = "  interface 2, usage page ff00, usage 0001\n  interface 2, usage page ff00, usage 0002"
        m = platforms.recognize("046d:c999", hidpp)
        self.assertEqual(m.platform.key, "logitech")
        self.assertFalse(m.auto)
        self.assertIsNone(platforms.recognize("24ae:2015", hidpp))
        m = platforms.recognize("feed:6060", "  interface 1, usage page ff60, usage 0061")
        self.assertTrue(m.platform.found_by_collection)
        self.assertFalse(m.auto)

    def test_two_families_at_once_are_none(self):
        # The W669 collection and QMK's: which firmware answers is not sure, so nothing is recognized.
        both = X68HE + "\n  interface 1, usage page ff60, usage 0061"
        self.assertIsNone(platforms.recognize("2e3c:c365", both))

    def test_the_signature_in_words(self):
        self.assertEqual(
            platforms.signature(platforms.BY_KEY["wraith"]),
            "interface 2, usage page ff1b, usage 0091, report id 01 (in 64, out 64 bytes)",
        )


class TheDriversSaySo(unittest.TestCase):
    """Each signature is the one of the driver it names: if a driver changes its collection, this fails."""

    def test_every_driver_exists_and_names_the_usage_pages(self):
        for p in platforms.PLATFORMS:
            with self.subTest(p.key):
                source = (ROOT / p.driver).read_text()
                for c in p.collections:
                    self.assertRegex(source, rf"(?i)0x{c.usage_page:04x}\b")

    def test_the_families_that_take_a_new_id_match_their_tables(self):
        for p in platforms.PLATFORMS:
            if not p.sibling:
                continue
            with self.subTest(p.key):
                source = (ROOT / p.driver).read_text()
                c = p.collections[0]
                self.assertIn("pub const MODELS: &[Model] = &[", source)
                # The interface and the report the driver uses.
                self.assertRegex(source, rf"(?:interface: Some\(|const INTERFACE: i32 = ){c.interface}\b")
                self.assertRegex(source, rf"const REPORT: usize = {c.reports[0].input};")
                if c.reports[0].id:
                    self.assertRegex(source, rf"(?i)const REPORT_ID: u8 = 0x{c.reports[0].id:02x};")
                # The entry it writes is Rust the table already uses: the same constructor or the same fields.
                entry = p.sibling.entry.format(vendor="ABCD", product="1234", name="N", params=next(iter(p.sibling.params.values())))
                first = entry.strip().split("(")[0].split(" {")[0]
                self.assertIn(first, source)
                for v, params in p.sibling.params.items():
                    self.assertTrue(v == "" or v in p.vendors, v)
                    self.assertIn(params.split(" {")[0], source)

    def test_the_families_are_the_catalogue_s(self):
        import json

        families = {d["family"] for d in json.loads((ROOT / "site/devices.json").read_text())["devices"]}
        for p in platforms.PLATFORMS:
            if p.sibling:
                self.assertIn(p.key, families)


if __name__ == "__main__":
    unittest.main()
