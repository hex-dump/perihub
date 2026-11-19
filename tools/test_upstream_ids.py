#!/usr/bin/env python3
"""Tests of tools/upstream_ids.py: python3 -I -m unittest discover -s tools -p "test_*.py"

The trees made here imitate the layout of the cloned projects (invented classes, only the shape of OpenRazer's
hardware files). Nothing is read from the projects themselves.
"""

import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import upstream_ids  # noqa: E402

MOUSE = '''
class RazerDemoWired(__RazerDevice):
    """
    Class for the Razer Demo (Wired)
    """
    USB_VID = 0x1532
    USB_PID = 0x00C0
    METHODS = ['get_battery']


class RazerDemoWireless(RazerDemoWired):
    """
    Class for the Razer Demo (Wireless)
    """

    USB_PID = 0x00C1


class RazerDemoReceiver(RazerDemoWireless):
    USB_PID = 0x00C2


class RazerOtherVendor(__RazerDevice):
    USB_VID = 0x1234
    USB_PID = 0x0001
'''
KEYBOARDS = '''
class RazerBoardWireless(RazerDemoWired):
    """
    Class for the Razer Board (Wireless)
    """
    USB_PID = 0x00C3


class RazerNoPid(RazerDemoWired):
    METHODS = []


class RazerLoop(RazerLoop):
    USB_PID = 0x0001
'''


def tree(files: dict[str, str]) -> tuple[tempfile.TemporaryDirectory, Path]:
    tmp = tempfile.TemporaryDirectory()
    hardware = Path(tmp.name) / "daemon/openrazer_daemon/hardware"
    hardware.mkdir(parents=True)
    for name, text in files.items():
        (hardware / name).write_text(text)
    return tmp, Path(tmp.name)


class OpenRazerReader(unittest.TestCase):
    def read(self, files):
        tmp, root = tree(files)
        self.addCleanup(tmp.cleanup)
        return {i: (name, where) for i, name, where in upstream_ids.openrazer(root)}

    def test_a_subclass_that_sets_only_its_product_id_has_the_vendor_of_its_base(self):
        # RazerViperV3ProWireless(RazerViperV3ProWired) is the real case: 1532:00c1 was never read.
        found = self.read({"mouse.py": MOUSE})
        self.assertEqual(found["1532:00c0"], ("Razer Demo (Wired)", "mouse"))
        self.assertEqual(found["1532:00c1"], ("Razer Demo (Wireless)", "mouse"))

    def test_a_chain_of_subclasses_and_a_base_in_another_file(self):
        found = self.read({"mouse.py": MOUSE, "keyboards.py": KEYBOARDS})
        self.assertIn("1532:00c2", found)
        self.assertEqual(found["1532:00c2"][0], "RazerDemoReceiver")  # no docstring: the class name
        self.assertEqual(found["1532:00c3"], ("Razer Board (Wireless)", "keyboards"))

    def test_a_class_with_its_own_vendor_keeps_it(self):
        self.assertIn("1234:0001", self.read({"mouse.py": MOUSE}))

    def test_nothing_is_made_up_without_a_vendor_or_a_product_id(self):
        found = self.read({"mouse.py": MOUSE, "keyboards.py": KEYBOARDS})
        self.assertEqual(sorted(found), ["1234:0001", "1532:00c0", "1532:00c1", "1532:00c2", "1532:00c3"])


class LibratbagReader(unittest.TestCase):
    def test_bluetooth_ids_are_read_with_the_usb_ones(self):
        # The G604: its .device file has DeviceMatch=usb:046d:c539;bluetooth:046d:b024 style entries.
        with tempfile.TemporaryDirectory() as tmp:
            devices = Path(tmp) / "data/devices"
            devices.mkdir(parents=True)
            (devices / "logitech-g604.device").write_text(
                "[Device]\nName=Logitech G604\nDeviceMatch=usb:046d:c539;bluetooth:046d:B024\nDriver=hidpp20\n"
            )
            found = {i for i, _, _ in upstream_ids.libratbag(Path(tmp))}
        self.assertEqual(found, {"046d:c539", "046d:b024"})

    def test_the_baseline_has_the_bluetooth_id_of_the_g604(self):
        self.assertIn("046d:b024", json.loads(upstream_ids.SEEN.read_text())["libratbag"])


class Baseline(unittest.TestCase):
    def test_the_baseline_has_the_ids_the_reader_used_to_miss(self):
        seen = json.loads(upstream_ids.SEEN.read_text())
        # Wireless and receiver variants of OpenRazer devices, subclasses of the wired ones.
        for usb in ("1532:00c1", "1532:00c0"):
            self.assertIn(usb, seen["openrazer"])

    def test_the_baseline_has_every_project_with_sorted_unique_ids(self):
        seen = json.loads(upstream_ids.SEEN.read_text())
        self.assertEqual(sorted(seen), sorted(upstream_ids.READERS))
        for project, ids in seen.items():
            self.assertEqual(ids, sorted(set(ids)), project)


if __name__ == "__main__":
    unittest.main()
