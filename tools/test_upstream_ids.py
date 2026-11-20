#!/usr/bin/env python3
"""Tests of tools/upstream_ids.py: python3 -I -m unittest discover -s tools -p "test_*.py"

The trees made here imitate the layout of the cloned projects (invented classes, only the shape of OpenRazer's
hardware files). Nothing is read from the projects themselves.
"""

import contextlib
import io
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

STEELSERIES_H = '''
#ifndef STEELSERIESDEVICES_H
#define STEELSERIESDEVICES_H
#define STEELSERIES_VID                 0x1038
#define STEELSERIES_APEX_PRO_PID        0x1610
#define STEELSERIES_APEX_PRO_TKL_PID    0x1614
#define STEELSERIES_RIVAL_PID           0x1622
#define DETECTOR_NAME                   "SteelSeries Demo"
#endif
'''
STEELSERIES_CPP = '''
#include "SteelSeriesDevices.h"

REGISTER_HID_DETECTOR_I  ("SteelSeries Apex Pro",     DetectSteelSeriesKeyboard, STEELSERIES_VID, STEELSERIES_APEX_PRO_PID, 1);
REGISTER_HID_DETECTOR_IPU("SteelSeries Rival",        DetectSteelSeriesMouse,    STEELSERIES_VID, STEELSERIES_RIVAL_PID,    0, 0xFFC0, 0x0001);
REGISTER_HID_DETECTOR_IP
(
    "SteelSeries Apex Pro TKL", /* a comment, with a comma and a ) in it */
    DetectSteelSeriesKeyboard,
    STEELSERIES_VID,
    STEELSERIES_APEX_PRO_TKL_PID,   // another one
    1,
    0xFFC1
);
REGISTER_HID_DETECTOR_PU (DETECTOR_NAME, DetectSteelSeriesDemo, STEELSERIES_VID, 0x1700, 0xFF00, 0x0001);
'''
OTHER_H = '''
#define STEELSERIES_VID                 0x9999
#define DETECTOR_NAME                   "Other Demo"
'''
OTHER_CPP = '''
REGISTER_HID_DETECTOR_P ("Other Demo", DetectOther, STEELSERIES_VID, 0x0001, 0xFF00);
REGISTER_HID_WRAPPED_DETECTOR_I ("Other Wrapped", DetectOther, STEELSERIES_VID, 0x0002, 0);
REGISTER_HID_DETECTOR_PU_ONLY("Any vendor", DetectOther, 0x59, 0x01);
REGISTER_HID_DETECTOR_BPU("Any product", DetectOther, STEELSERIES_VID, HID_PID_ANY, 0, 0xFF00, 0x0001);
REGISTER_USB_SERIAL_DETECTOR("A serial device", DetectOther, STEELSERIES_VID, 0x0003);
REGISTER_DETECTOR("Not USB", DetectOther);
'''


class OpenRGBReader(unittest.TestCase):
    def read(self, folders: dict[str, dict[str, str]]):
        """folders: {"SteelSeriesController": {"file.h": text}}. Returns {id: (name, folder)}."""
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        for folder, files in folders.items():
            (Path(tmp.name) / "Controllers" / folder).mkdir(parents=True)
            for name, text in files.items():
                (Path(tmp.name) / "Controllers" / folder / name).write_text(text)
        errors = io.StringIO()
        with contextlib.redirect_stderr(errors):
            found = {i: (name, where) for i, name, where in upstream_ids.openrgb(Path(tmp.name))}
        self.errors = errors.getvalue()
        return found

    def steelseries(self):
        return self.read(
            {
                "SteelSeriesController": {"SteelSeriesDevices.h": STEELSERIES_H, "SteelSeriesControllerDetect.cpp": STEELSERIES_CPP},
                "OtherController": {"OtherDevices.h": OTHER_H, "OtherControllerDetect.cpp": OTHER_CPP},
            }
        )

    def test_a_call_with_spaces_before_the_parenthesis_is_read(self):
        # REGISTER_HID_DETECTOR_I  ("SteelSeries Apex Pro", ...) is how OpenRGB writes the Apex Pro, 1038:1610 (issue #111):
        # the reader wanted the parenthesis right after the macro name and never saw it.
        found = self.steelseries()
        self.assertEqual(found["1038:1610"], ("SteelSeries Apex Pro", "SteelSeriesController"))
        self.assertEqual(found["1038:1622"], ("SteelSeries Rival", "SteelSeriesController"))  # no space: as before

    def test_a_call_over_several_lines_with_comments_between_the_arguments_is_read(self):
        found = self.steelseries()
        self.assertEqual(found["1038:1614"], ("SteelSeries Apex Pro TKL", "SteelSeriesController"))

    def test_the_ids_and_the_name_come_from_the_defines_of_the_folder_of_the_call(self):
        found = self.steelseries()
        self.assertEqual(found["1038:1700"], ("SteelSeries Demo", "SteelSeriesController"))  # a name that is a #define
        # The same define names mean other things in another folder (OpenRGB does this with ALIENWARE_VID, MSI_VID…).
        self.assertEqual(found["9999:0001"], ("Other Demo", "OtherController"))
        self.assertEqual(found["9999:0002"], ("Other Wrapped", "OtherController"))  # REGISTER_HID_WRAPPED_DETECTOR_I

    def test_what_has_no_id_is_not_made_up_and_a_form_nobody_foresaw_is_written_on_stderr(self):
        found = self.steelseries()
        self.assertEqual(
            sorted(found),
            ["1038:1610", "1038:1614", "1038:1622", "1038:1700", "9999:0001", "9999:0002"],
        )  # not 0059:0001 (…_ONLY has no ids), not a serial device, not "any product"
        self.assertEqual(self.errors, "")  # HID_PID_ANY is on purpose
        self.read({"X": {"XDetect.cpp": 'REGISTER_HID_DETECTOR ("X", DetectX, X_VID, 0x0001);\n'}})
        self.assertIn("XDetect.cpp:1", self.errors)

    def test_an_unterminated_call_does_not_stop_the_reader(self):
        found = self.read({"X": {"XDetect.cpp": 'REGISTER_HID_DETECTOR("X", DetectX, 0x1234, 0x0001\n'}})
        self.assertEqual(found, {})


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

    def test_the_baseline_has_the_openrgb_ids_the_reader_used_to_miss(self):
        seen = json.loads(upstream_ids.SEEN.read_text())["OpenRGB"]
        # The SteelSeries Apex Pro (issue #111) and the other detectors OpenRGB writes with spaces before the parenthesis.
        for usb in ("1038:1610", "1038:1612", "1038:1614", "1038:1642"):
            self.assertIn(usb, seen)
        self.assertNotIn("0059:0001", seen)  # made up from a REGISTER_HID_DETECTOR_PU_ONLY, which has no ids

    def test_the_baseline_has_every_project_with_sorted_unique_ids(self):
        seen = json.loads(upstream_ids.SEEN.read_text())
        self.assertEqual(sorted(seen), sorted(upstream_ids.READERS))
        for project, ids in seen.items():
            self.assertEqual(ids, sorted(set(ids)), project)


if __name__ == "__main__":
    unittest.main()
