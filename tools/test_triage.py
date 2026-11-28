#!/usr/bin/env python3
"""Tests of tools/triage.py: python3 -I -m unittest discover -s tools -p "test_*.py"

The report texts are the ones the app writes: they were made by the builders of
app/src/lib/testReport.ts (testReport, corsairTest, peripheralTest) with a fixture each, and the
issue bodies are laid out the way GitHub lays out the answers of the test report form in
.github/ISSUE_TEMPLATE/01_test_report.yml.
"""

import json
import re
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import triage  # noqa: E402

ROOT = triage.ROOT
TEMPLATES = ROOT / ".github/ISSUE_TEMPLATE"
FORM = "01_test_report.yml"
# The forms of PeriHub 0.9.0 and older (device_test.yml, mouse_test.yml) have the same fields.
LEGACY_FORMS = ("device_test.yml", "mouse_test.yml")
LATEST = "0.9.0"

# Made by the app, version 0.9.0 on Linux.
ASUS = """PeriHub 0.9.0 on linux x86_64
Device: ASUS ROG Keris (mouse), USB ID 0b05:195c, protocol from libratbag (MIT)
Firmware: 1.02.03
Active profile: 1 of 3, DPI level 2
DPI levels: 400 / 800 / 1600 / 3200, polling: 1000 Hz
Other settings: Button response: 8 ms
LED 1: mode 0, brightness 100, #ff0000
Button Left: Left
Changes: available (experimental)
Requests and answers:
  12 04 00 00 → 12 04 00 00 03 00
  12 05 00 00 → 12 05 00 00 01 e8"""
ASUS_READ_ONLY = """PeriHub 0.9.0 on linux x86_64
Device: ASUS ROG Keris (mouse), USB ID 0b05:195c, protocol from libratbag (MIT)
Firmware: 1.02.03
Active profile: 1 of 3, DPI level 2
DPI levels: 400 / 800 / 1600 / 3200, polling: 1000 Hz
Other settings: Button response: 8 ms
LED 1: mode 0, brightness 100, #ff0000
Button Left: Left
Changes: off (read-only)
Requests and answers:
  12 04 00 00 → 12 04 00 00 03 00
  12 05 00 00 → 12 05 00 00 01 e8"""
ASUS_PROBLEM = """PeriHub 0.9.0 on linux x86_64
Device: ASUS ROG Keris (mouse), USB ID 0b05:195c, protocol from libratbag (MIT)
Firmware: 1.02.03
Active profile: 1 of 3, DPI level 2
DPI levels: 400 / 800 / 1600 / 3200, polling: 1000 Hz
Other settings: Button response: 8 ms
LED 1: mode 0, brightness 100, #ff0000
Button Left: Left
Changes: off (unexpected answer: polling rate code 09)
Requests and answers:
  12 04 00 00 → 12 04 00 00 03 00
  12 05 00 00 → 12 05 00 00 01 e8"""
ASUS_BLIND = """PeriHub 0.9.0 on linux x86_64
Device: ASUS ROG Keris (mouse), USB ID 0b05:195c, protocol from libratbag (MIT)
Firmware: 1.02.03
Settings: this device cannot be read back; PeriHub sends them one at a time
Active profile: 1 of 3, DPI level 2
DPI levels: 400 / 800 / 1600 / 3200, polling: 1000 Hz
Other settings: Button response: 8 ms
LED 1: mode 0, brightness 100, #ff0000
Button Left: Left
Changes: available (experimental)
Requests and answers:
  12 04 00 00 → 12 04 00 00 03 00
  12 05 00 00 → 12 05 00 00 01 e8"""
CORSAIR = """PeriHub 0.9.0 on linux x86_64
Device: Corsair M65 RGB ULTRA (mouse), USB ID 1b1c:1b9e
Connection: USB cable
Firmware: 3.4.5
Mode at connection: 1
Polling: 1000 Hz, DPI: 1600
LEDs: 3
Could not read: nothing
Experimental lighting: off"""
CORSAIR_ERROR = """PeriHub 0.9.0 on linux x86_64
Device: Corsair M65 RGB ULTRA (mouse), USB ID 1b1c:1b9e
Connection: USB cable
Firmware: 3.4.5
Mode at connection: 1
Polling: not read Hz, DPI: 1600
LEDs: 3
Could not read: polling: the device did not answer
Experimental lighting: off"""
CORSAIR_LIGHTING = """PeriHub 0.9.0 on linux x86_64
Device: Corsair M65 RGB ULTRA (mouse), USB ID 1b1c:1b9e
Connection: USB cable
Firmware: 3.4.5
Mode at connection: 1
Polling: 1000 Hz, DPI: 1600
LEDs: 3
Could not read: nothing
Experimental lighting: on"""
MOUSE = """PeriHub 0.9.0 on linux x86_64
Mouse: G403 HERO (reports itself as "G403 HERO Gaming Mouse")
USB ID: 046d:c08f
Connection: USB cable
Support: experimental (memory format not tested yet)
Software control: yes
Firmware: U1 88.5.6
Profiles: 5, buttons: 11
Memory: model 1, profile format 2, macro format 1, 16 sectors of 255 bytes, 5 factory profiles
Profile list: saved in the mouse
G-Shift: yes, DPI shift: yes
DPI: 100-25600, step 50
Polling (ms): 1, 2, 4, 8
LEDs: zone 0 at 1, effects 0 1 3
Features: 0001 v1, 8071 v4 (flags 40)"""
MOUSE_DAMAGED = """PeriHub 0.9.0 on linux x86_64
Mouse: G403 HERO (reports itself as "G403 HERO Gaming Mouse")
USB ID: 046d:c08f
Connection: USB cable
Support: experimental (memory format not tested yet)
Software control: yes
Firmware: U1 88.5.6
Profiles: 5, buttons: 11
Memory: model 1, profile format 2, macro format 1, 16 sectors of 255 bytes, 5 factory profiles
Profile list: damaged when connected (checksum does not match)
G-Shift: yes, DPI shift: yes
DPI: 100-25600, step 50
Polling (ms): 1, 2, 4, 8
LEDs: zone 0 at 1, effects 0 1 3
Features: 0001 v1, 8071 v4 (flags 40)"""
MOUSE_RECEIVER = """PeriHub 0.9.0 on linux x86_64
Mouse: G403 HERO (reports itself as "G403 HERO Gaming Mouse")
USB ID: 046d:c539
Connection: LIGHTSPEED receiver 046d:c539, slot 1, wireless id 4070
Support: experimental (memory format not tested yet)
Software control: yes
Firmware: U1 88.5.6
Profiles: 5, buttons: 11
Memory: model 1, profile format 2, macro format 1, 16 sectors of 255 bytes, 5 factory profiles
Profile list: saved in the mouse
G-Shift: yes, DPI shift: yes
DPI: 100-25600, step 50
Polling (ms): 1, 2, 4, 8
LEDs: zone 0 at 1, effects 0 1 3
Features: 0001 v1, 8071 v4 (flags 40)"""
MOUSE_TESTED = """PeriHub 0.9.0 on linux x86_64
Mouse: G403 HERO (reports itself as "G403 HERO Gaming Mouse")
USB ID: 046d:c08f
Connection: USB cable
Support: tested
Software control: yes
Firmware: U1 88.5.6
Profiles: 5, buttons: 11
Memory: model 1, profile format 2, macro format 1, 16 sectors of 256 bytes, 5 factory profiles
Profile list: saved in the mouse
G-Shift: yes, DPI shift: yes
DPI: 100-25600, step 50
Polling (ms): 1, 2, 4, 8
LEDs: zone 0 at 1, effects 0 1 3
Features: 0001 v1, 8071 v4 (flags 40)"""
MOUSE_UNWRITABLE = """PeriHub 0.9.0 on linux x86_64
Mouse: G403 HERO (reports itself as "G403 HERO Gaming Mouse")
USB ID: 046d:c08f
Connection: USB cable
Support: read-only (memory format PeriHub cannot write)
Software control: yes
Firmware: U1 88.5.6
Profiles: 5, buttons: 11
Memory: model 1, profile format 5, macro format 1, 16 sectors of 255 bytes, 5 factory profiles
Profile list: saved in the mouse
G-Shift: yes, DPI shift: yes
DPI: 100-25600, step 50
Polling (ms): 1, 2, 4, 8
LEDs: zone 0 at 1, effects 0 1 3
Features: 0001 v1, 8071 v4 (flags 40)"""

# What site/devices.json says of these (cargo xtask gen), at the time of writing.
DEVICES = [
    {"family": "asus", "brand": "ASUS", "name": "ROG Keris", "kind": "mouse", "level": "experimental", "usb_ids": ["0b05:195c"]},
    {"family": "razer", "brand": "Razer", "name": "Viper", "kind": "mouse", "level": "supported", "usb_ids": ["1532:0078"]},
    {"family": "corsair-mouse", "brand": "Corsair", "name": "M65 RGB ULTRA", "kind": "mouse", "level": "experimental", "usb_ids": ["1b1c:1b9e"]},
    {"family": "logitech-onboard", "brand": "Logitech", "name": "G403 HERO", "kind": "mouse", "level": "by_memory_format", "usb_ids": ["046d:c08f"]},
    {"family": "logitech-onboard", "brand": "Logitech", "name": "G502 HERO", "kind": "mouse", "level": "supported", "usb_ids": ["046d:c08b"]},
    {"family": "lightspeed", "brand": "Logitech", "name": "LIGHTSPEED receiver", "kind": "receiver", "level": "experimental", "usb_ids": ["046d:c539"]},
    {"family": "roccat", "brand": "Roccat", "name": "Kone", "kind": "mouse", "level": "read_only", "usb_ids": ["1e7d:2c88"]},
    {"family": "g600", "brand": "Logitech", "name": "G600", "kind": "mouse", "level": "experimental", "usb_ids": ["046d:c24a"]},
]
TESTED = {(2, 256), (3, 255)}


def options(form: str, group: str) -> list[str]:
    """The labels of a checkboxes element of a form, in order."""
    text = (TEMPLATES / form).read_text()
    block = re.search(rf"- type: checkboxes\n    id: {group}\n(.*?)(?=\n  - type:|\Z)", text, re.S).group(1)
    return [m.group(1).strip().strip('"') for m in re.finditer(r"^        - label: (.+)$", block, re.M)]


def answer(label: str, value: str | None) -> str:
    return f"### {label}\n\n{value if value else '_No response_'}\n\n"


def boxes(form: str, group: str, label: str, ticked: list[str]) -> str:
    lines = [f"- [{'X' if o in ticked else ' '}] {o}" for o in options(form, group)]
    return f"### {label}\n\n" + "\n".join(lines) + "\n\n"


def test_report_issue(report, usb, *, name="Device", kind="Mouse", brand="Other", connection="USB cable", shows=(), saved=(),
                      notices=None, wrong=None, number=40, version=LATEST, labels=("test report",)):
    body = (
        answer("Device type", kind) + answer("Brand", brand) + answer("Device", name)
        + answer("USB ID (vendor:product)", usb) + answer("Connection", connection)
        + answer("PeriHub version", version) + answer("Operating system", "Linux")
        + answer("Test report", f"```text\n{report}\n```")
        + boxes(FORM, "matches", "What PeriHub shows correctly", list(shows))
        + boxes(FORM, "saving", "Saving, if you tried it", list(saved))
        + answer("Messages and errors", notices) + answer("What is wrong or missing", wrong)
    )
    return {"number": number, "labels": [{"name": n} for n in labels], "body": body}


def device_issue(report, usb, *, ticked=(triage.SHOWS_OTHER,), **kwargs):
    return test_report_issue(report, usb, shows=ticked, **kwargs)


MOUSE_SHOWS = list(triage.SHOWS_MOUSE)
MOUSE_SAVED = list(triage.SAVING_MOUSE)


def mouse_issue(report, usb="046d:c08f", *, shows=MOUSE_SHOWS, saved=MOUSE_SAVED, number=41, **kwargs):
    return test_report_issue(report, usb, name="G403 HERO", brand="Logitech", shows=shows, saved=saved, number=number, **kwargs)


def test_labels(issue):
    return triage.triage(issue, LATEST, DEVICES)["labels"]


def run(issue, latest=LATEST, devices=DEVICES, tested=TESTED, current=None):
    return triage.triage(issue, latest, devices, current, tested)


class Confirms(unittest.TestCase):
    def test_another_brand_device_with_a_clean_report(self):
        result = run(device_issue(ASUS, "0b05:195c"))
        self.assertEqual(result["confirm"], {"usb": "0b05:195c"})
        self.assertEqual(result["issue"], 40)

    def test_a_read_only_device_and_one_that_cannot_be_read_back(self):
        roccat = ASUS_READ_ONLY.replace("ASUS ROG Keris", "Roccat Kone").replace("0b05:195c", "1e7d:2c88")
        self.assertEqual(run(device_issue(roccat, "1e7d:2c88"))["confirm"], {"usb": "1e7d:2c88"})
        self.assertIn("Settings: this device cannot be read back", ASUS_BLIND)
        self.assertEqual(run(device_issue(ASUS_BLIND, "0b05:195c"))["confirm"], {"usb": "0b05:195c"})

    def test_a_corsair_mouse_needs_firmware_polling_and_dpi(self):
        ticked = [*triage.SHOWS_CORSAIR, *triage.SHOWS_CORSAIR_MOUSE]
        self.assertEqual(run(device_issue(CORSAIR, "1b1c:1b9e", ticked=ticked))["confirm"], {"usb": "1b1c:1b9e"})
        for missing in ticked:
            partial = [t for t in ticked if t != missing]
            self.assertNotIn("confirm", run(device_issue(CORSAIR, "1b1c:1b9e", ticked=partial)), missing)

    def test_corsair_lighting_that_was_on_needs_its_boxes_too(self):
        ticked = [*triage.SHOWS_CORSAIR, *triage.SHOWS_CORSAIR_MOUSE]
        self.assertNotIn("confirm", run(device_issue(CORSAIR_LIGHTING, "1b1c:1b9e", ticked=ticked)))
        ticked += triage.SHOWS_CORSAIR_LIGHTING
        self.assertIn("confirm", run(device_issue(CORSAIR_LIGHTING, "1b1c:1b9e", ticked=ticked)))

    def test_a_mouse_memory_format_nobody_saved_to(self):
        result = run(mouse_issue(MOUSE))
        self.assertEqual(result["confirm"], {"memory_format": [2, 255]})
        self.assertEqual(result["issue"], 41)

    def test_a_g903_hero_saved_to_with_its_cable_confirms_format_4(self):
        # Issue #115: format 4 is written like 2 and 3, and one clean report with the cable makes it tested.
        report = MOUSE.replace("profile format 2", "profile format 4")
        self.assertEqual(run(mouse_issue(report))["confirm"], {"memory_format": [4, 255]})

    def test_the_boxes_of_the_form_are_the_ones_the_script_asks_for(self):
        for form in (FORM, *LEGACY_FORMS):
            shows = options(form, "matches")
            for label in (
                triage.SHOWS_OTHER, *triage.SHOWS_CORSAIR, *triage.SHOWS_CORSAIR_MOUSE, *triage.SHOWS_CORSAIR_LIGHTING,
                *triage.SHOWS_MOUSE,
            ):
                self.assertIn(label, shows, form)
            for label in triage.SAVING_MOUSE:
                self.assertIn(label, options(form, "saving"), form)

    def test_the_old_forms_ask_the_same_as_the_new_one(self):
        for form in LEGACY_FORMS:
            for group in ("matches", "saving"):
                self.assertEqual(options(form, group), options(FORM, group), (form, group))

    def test_the_form_has_the_fields_the_script_reads(self):
        text = (TEMPLATES / FORM).read_text()
        for label in ("Device type", "Brand", "USB ID (vendor:product)", "PeriHub version", "Test report",
                      "What PeriHub shows correctly", "Saving, if you tried it", "Messages and errors", "What is wrong or missing"):
            self.assertIn(f"label: {label}\n", text)

    def test_the_memory_rules_are_the_core_s(self):
        source = (ROOT / "crates/perihub-core/src/mouse.rs").read_text()
        self.assertIn("WRITABLE_PROFILE_FORMATS: &[u8] = &[2, 3, 4];", source)
        self.assertIn("profile::MIN_WRITE_SECTOR_SIZE..=profile::VERIFIED_SECTOR_SIZE", source)
        profile = (ROOT / "crates/perihub-core/src/hidpp/profile.rs").read_text()
        self.assertIn("VERIFIED_SECTOR_SIZE: usize = 256;", profile)
        self.assertIn("MIN_WRITE_SECTOR_SIZE: usize = 255;", profile)

    def test_the_tested_formats_are_read_from_the_core_table(self):
        # Later confirmations add formats: the two tested first stay.
        self.assertGreaterEqual(triage.tested_memory_formats(triage.CONFIRMED_RS.read_text()), {(2, 256), (3, 255)})
        self.assertEqual(triage.tested_memory_formats("nothing"), set())

    def test_the_confirmation_is_part_of_the_json_the_workflow_reads(self):
        result = json.loads(json.dumps(run(device_issue(ASUS, "0b05:195c"))))
        self.assertEqual(sorted(result), ["comment", "confirm", "issue", "labels"])
        self.assertIsNone(result["comment"])
        self.assertIn("brand: ASUS", test_labels(device_issue(ASUS, "0b05:195c", brand="ASUS")))


class DoesNotConfirm(unittest.TestCase):
    def none(self, issue, **kwargs):
        result = run(issue, **kwargs)
        self.assertNotIn("confirm", result)
        self.assertNotIn("issue", result)
        return result

    def test_a_line_that_says_something_went_wrong(self):
        self.none(device_issue(ASUS_PROBLEM, "0b05:195c"))
        self.none(device_issue(CORSAIR_ERROR, "1b1c:1b9e", ticked=[*triage.SHOWS_CORSAIR, *triage.SHOWS_CORSAIR_MOUSE]))
        self.none(device_issue(ASUS.replace("→ 12 04", "→ no answer"), "0b05:195c"))
        self.none(device_issue(ASUS.replace("Changes: available (experimental)\n", ""), "0b05:195c"))

    def test_messages_or_something_wrong_in_the_form(self):
        self.none(device_issue(ASUS, "0b05:195c", notices="Logi Options+ was running"))
        self.none(device_issue(ASUS, "0b05:195c", wrong="The LED is blue"))
        self.none(device_issue(ASUS, "0b05:195c", wrong="None"))
        self.none(mouse_issue(MOUSE, notices="it asked for a restart"))
        self.none(mouse_issue(MOUSE, wrong="DPI is off"))

    def test_the_box_that_says_it_matches_is_not_ticked(self):
        self.none(device_issue(ASUS, "0b05:195c", ticked=["Firmware version"]))
        self.none(device_issue(ASUS, "0b05:195c", ticked=[]))

    def test_an_id_nobody_listed_or_one_already_supported(self):
        issue = device_issue(ASUS, "0b05:195c")
        self.none(issue, devices=[d for d in DEVICES if d["family"] != "asus"])
        supported = [{**d, "level": "supported"} if d["family"] == "asus" else d for d in DEVICES]
        self.none(issue, devices=supported)
        razer = ASUS.replace("ASUS ROG Keris", "Razer Viper").replace("0b05:195c", "1532:0078")
        self.none(device_issue(razer, "1532:0078"))

    def test_an_id_or_name_that_does_not_match(self):
        self.none(device_issue(ASUS, "0b05:195d"))
        self.none(device_issue(ASUS.replace("ROG Keris", "ROG Gladius"), "0b05:195c"))
        self.none(device_issue(ASUS.replace("(mouse)", "(keyboard)"), "0b05:195c"))
        twice = DEVICES + [{**DEVICES[0], "name": "ROG Keris Wireless"}]
        self.none(device_issue(ASUS, "0b05:195c"), devices=twice)

    def test_families_the_form_has_no_box_for(self):
        g600 = ASUS.replace("ASUS ROG Keris", "Logitech G600").replace("0b05:195c", "046d:c24a")
        self.none(device_issue(g600, "046d:c24a"))
        receiver = ASUS.replace("ASUS ROG Keris (mouse)", "Logitech LIGHTSPEED receiver (mouse)").replace("0b05:195c", "046d:c539")
        self.none(device_issue(receiver, "046d:c539"))

    def test_a_report_that_is_not_the_apps_or_not_from_a_form(self):
        no_first_line = ASUS.split("\n", 1)[1]
        self.none(device_issue(no_first_line, "0b05:195c"))
        self.none(device_issue("Device: ASUS ROG Keris (mouse), USB ID 0b05:195c\n" + ASUS.split("\n", 2)[2], "0b05:195c"))
        self.none(device_issue("My mouse works fine, ASUS ROG Keris, 0b05:195c", "0b05:195c"))
        pasted = {"number": 5, "labels": [], "body": ASUS}
        result = self.none(pasted)
        self.assertIn("test report", result["labels"])
        # A form's label with no answers of the form.
        self.none({"number": 5, "labels": [{"name": "test report"}], "body": ASUS})

    def test_a_report_from_an_older_version(self):
        self.assertIn("comment", self.none(device_issue(ASUS, "0b05:195c"), latest="0.9.1"))
        self.assertIsNotNone(run(device_issue(ASUS, "0b05:195c"), latest="0.9.1")["comment"])

    def test_a_request_is_not_a_report(self):
        issue = device_issue(ASUS, "0b05:195c")
        issue["labels"] = [{"name": "device request"}]
        self.none(issue)

    def test_a_mouse_that_does_not_qualify(self):
        self.none(mouse_issue(MOUSE_DAMAGED))
        self.none(mouse_issue(MOUSE_RECEIVER, usb="046d:c539"))
        self.none(mouse_issue(MOUSE_TESTED))
        self.none(mouse_issue(MOUSE_UNWRITABLE))
        self.none(mouse_issue(MOUSE), tested={(2, 255)})
        self.none(mouse_issue(MOUSE, usb="046d:c08b"))
        self.none(mouse_issue(MOUSE.replace("sectors of 255", "sectors of 512")))
        self.none(mouse_issue(MOUSE.replace("profile format 2", "profile format 5")))
        self.none(mouse_issue(MOUSE.replace("Features: 0001 v1, 8071 v4 (flags 40)", "Features: not read")))
        self.none(mouse_issue(MOUSE.replace("Support: experimental (memory format not tested yet)", "Support: read-only on this connection")))

    def test_a_mouse_whose_saving_was_not_confirmed(self):
        self.none(mouse_issue(MOUSE, saved=[]))
        self.none(mouse_issue(MOUSE, saved=MOUSE_SAVED[:1]))
        self.none(mouse_issue(MOUSE, shows=MOUSE_SHOWS[:1]))
        # The 15-second box alone is not the saving the mouse needs.
        self.none(mouse_issue(MOUSE, saved=["Letting the 15 seconds run out put the old settings back"]))


class Existing(unittest.TestCase):
    """What the script did before, and still does."""

    def test_a_request_for_a_known_device_is_answered(self):
        body = answer("Device", "ROG Keris") + answer("USB ID (vendor:product)", "0b05:195c")
        result = run({"number": 1, "labels": [{"name": "device request"}], "body": body})
        self.assertIn("already recognizes", result["comment"])
        self.assertNotIn("confirm", result)

    def test_a_pasted_report_gets_its_labels(self):
        result = run({"number": 2, "labels": [], "body": CORSAIR})
        self.assertEqual(result["labels"][:1], ["test report"])
        self.assertIsNone(result["comment"])


class WizardReports(unittest.TestCase):
    """The report the "Test my device" wizard makes: the app's report, then the writes of the session and what
    the tester says was tried (reportWizard.ts), and a transcript for the replay test (docs/replay.md)."""

    WRITES = "Writes this session: 0 sent, 0 refused"
    TRIED = "What I tried:\n  worked: The name and USB ID PeriHub shows are right\n  worked: What PeriHub shows matches the device"
    TRANSCRIPT = (
        'Transcript (every request and answer, for a replay test):\n{\n  "format": "perihub-transcript",\n'
        '  "notes": ["an error was shown once, a problem it did not cause"],\n  "events": [\n'
        '    {"op":"write","data":"12 04","len":64}\n  ]\n}'
    )

    def report(self, *parts):
        return "\n".join([ASUS.split("Requests and answers:")[0].rstrip(), *parts])

    def test_the_writes_header_and_the_ticked_checklist_are_no_trouble(self):
        issue = device_issue(self.report(self.WRITES) + "\n\n" + self.TRIED, "0b05:195c")
        self.assertEqual(run(issue)["confirm"], {"usb": "0b05:195c"})

    def test_a_transcript_is_not_read_as_sentences(self):
        issue = device_issue(self.report(self.TRANSCRIPT, self.WRITES) + "\n\n" + self.TRIED, "0b05:195c")
        self.assertEqual(run(issue)["confirm"], {"usb": "0b05:195c"})
        one_line = 'Transcript (every request and answer, for a replay test):\n{"notes":["failed"],"events":[]}'
        self.assertIn("confirm", run(device_issue(self.report(one_line), "0b05:195c")))

    def test_what_comes_after_the_transcript_is_still_read(self):
        failed = self.TRIED + "\n  did not work: The name and USB ID PeriHub shows are right"
        self.assertNotIn("confirm", run(device_issue(self.report(self.TRANSCRIPT, self.WRITES) + "\n\n" + failed, "0b05:195c")))

    def test_a_cut_transcript_confirms_nothing(self):
        cut = self.TRANSCRIPT.split('"events"')[0]
        self.assertNotIn("confirm", run(device_issue(self.report(cut), "0b05:195c")))

    def test_an_answer_that_did_not_work_or_a_write_that_failed(self):
        failed = self.TRIED + "\n  did not work: The DPI PeriHub shows matches the mouse"
        self.assertNotIn("confirm", run(device_issue(self.report(self.WRITES) + "\n\n" + failed, "0b05:195c")))
        write = "Writes this session: 1 sent, 0 refused:\n  14:03:27 ASUS ROG Keris: dpi, 3 bytes, failed: timed out"
        self.assertNotIn("confirm", run(device_issue(self.report(write), "0b05:195c")))


class Triage(unittest.TestCase):
    """What the script did before the confirmations, and still does."""

    def test_labels_come_from_the_form_s_dropdowns(self):
        issue = device_issue(ASUS, "0b05:195c", brand="ASUS", kind="Mouse", labels=())
        # No label from GitHub: the headings of the form say what it is.
        self.assertEqual(run(issue)["labels"], ["test report", "brand: ASUS", "mouse"])
        other = device_issue(ASUS, "0b05:195c", brand="Other", kind="Keyboard")
        # The catalogue and the app's report know it is a mouse: the dropdown's "Keyboard" goes (issue #97).
        result = run({**other, "labels": [{"name": "test report"}, {"name": "keyboard"}]})
        self.assertEqual(result["labels"], ["brand: ASUS", "mouse"])
        self.assertEqual(result["remove_labels"], ["keyboard"])
        # An id the catalogue does not have: the dropdown, which the app fills in and a person may correct (issue
        # #111), beats the report's own "(mouse)"...
        unknown = device_issue(ASUS.replace("0b05:195c", "abcd:0001"), "abcd:0001", brand="Other", kind="Keyboard")
        self.assertEqual(run(unknown)["labels"], ["brand: Other", "keyboard"])
        # ...and the report's kind is what is left when the dropdown says nothing the labels have ("Receiver").
        receiver = device_issue(ASUS.replace("0b05:195c", "abcd:0001"), "abcd:0001", brand="Other", kind="Receiver")
        self.assertEqual(run(receiver)["labels"], ["brand: Other", "mouse"])

    def test_the_released_devices_are_read_with_the_branches_too(self):
        # triage.yml passes 4 files: the third is still the devices of the latest release, not main's.
        import io
        from contextlib import redirect_stdout
        from tempfile import TemporaryDirectory

        with TemporaryDirectory() as tmp:
            event, released, branches = (Path(tmp) / n for n in ("event.json", "released.json", "branches.json"))
            body = answer("Device model", "X") + answer("USB ID (vendor:product)", "0b05:195c")
            event.write_text(json.dumps({"issue": {"number": 1, "labels": [{"name": "device request"}], "body": body}}))
            released.write_text(json.dumps({"devices": []}))
            branches.write_text("{}")
            out = io.StringIO()
            argv = sys.argv
            try:
                sys.argv = ["triage.py", str(event), "0.9.0", str(released), str(branches)]
                with redirect_stdout(out):
                    triage.main()
            finally:
                sys.argv = argv
            # Only on main, in no release: never "already recognizes", and never "not in PeriHub" either: it is added,
            # and comes with the next release.
            comment = json.loads(out.getvalue())["comment"]
            self.assertIn("already added", comment)
            self.assertIn("next release", comment)
            self.assertNotIn("already recognizes", comment)

    def test_a_report_from_an_older_version_is_asked_to_try_the_latest(self):
        result = run(device_issue(ASUS.replace("0.9.0", "0.8.4"), "0b05:195c", version="0.8.4"))
        self.assertIn("0.8.4", result["comment"])
        self.assertNotIn("confirm", result)

    def test_a_request_for_a_device_only_on_main_is_told_it_comes_with_the_next_release(self):
        # It used to get no answer at all (issue #123 was even told "no documentation" before its id reached main).
        released = [d for d in DEVICES if d["family"] != "asus"]
        body = answer("Device model", "ROG Keris") + answer("USB ID (vendor:product)", "0b05:195c")
        issue = {"number": 1, "labels": [{"name": "device request"}], "body": body}
        result = triage.triage(issue, LATEST, released, DEVICES, upstream={"libratbag": ["0b05:195c"]})
        self.assertIn("**already added** to PeriHub: **ASUS ROG Keris**", result["comment"])
        self.assertIn("next release", result["comment"])
        self.assertNotIn("already recognizes", result["comment"])
        self.assertNotIn("`0b05:195c` is not in", result["comment"])
        self.assertNotIn("ready to add", result["labels"])
        self.assertNotIn("needs captures", result["labels"])
        self.assertNotIn("auto_add", result)
        self.assertEqual(result["labels"], ["brand: ASUS", "mouse"])

    def test_a_device_main_confirmed_already_is_not_confirmed_again(self):
        current = [{**d, "level": "supported"} if d["family"] == "asus" else d for d in DEVICES]
        self.assertNotIn("confirm", run(device_issue(ASUS, "0b05:195c"), current=current))
        self.assertIn("confirm", run(device_issue(ASUS, "0b05:195c"), current=DEVICES))

    def test_a_pasted_report_confirms_nothing(self):
        self.assertNotIn("confirm", run({"number": 9, "labels": [], "body": ASUS}))


BUILD = "test/asus-keris@02fd962"
BUILT = ASUS.replace("PeriHub 0.9.0 on linux", f"PeriHub 0.9.5 (test build {BUILD}) on linux")
KERIS = next(d for d in DEVICES if d["family"] == "asus")
NO_KERIS = [d for d in DEVICES if d is not KERIS]
BRANCHES = {"test/asus-keris": [KERIS], "test/other": [d for d in DEVICES if d["family"] == "razer"]}


def build_run(issue, branches=BRANCHES, latest="0.9.4", devices=NO_KERIS, current=None):
    return triage.triage(issue, latest, devices, NO_KERIS if current is None else current, TESTED, branches=branches)


class TestBuilds(unittest.TestCase):
    """A device only a test/* branch has (not on main yet) is confirmed on it by a clean report of its test build."""

    def test_the_first_line_of_a_test_build_parses(self):
        self.assertEqual(BUILT.splitlines()[0], f"PeriHub 0.9.5 (test build {BUILD}) on linux x86_64")
        self.assertEqual(triage.version(BUILT.splitlines()[0]), (0, 9, 5))
        self.assertEqual(triage.version(f"0.9.5 (test build test/g915-1.2.3@abc1234)"), (0, 9, 5))
        self.assertEqual(triage.test_build(BUILT.splitlines()[0]), BUILD)
        self.assertEqual(triage.test_build(f"0.9.5 (test build {BUILD})"), BUILD)
        self.assertIsNone(triage.test_build(ASUS.splitlines()[0]))
        self.assertIsNone(triage.test_build("0.9.5"))

    def test_a_clean_report_of_a_test_build_confirms_on_its_branch(self):
        result = build_run(device_issue(BUILT, "0b05:195c", version=f"0.9.5 (test build {BUILD})"))
        self.assertEqual(result["confirm"], {"usb": "0b05:195c", "branch": "test/asus-keris"})
        self.assertEqual(result["issue"], 40)
        self.assertIsNone(result["comment"])

    def test_a_test_build_is_never_told_to_update(self):
        # Built from 0.9.3's main while 0.9.4 is out: still the newest of its branch.
        old = BUILT.replace("PeriHub 0.9.5", "PeriHub 0.9.3")
        result = build_run(device_issue(old, "0b05:195c", version="0.9.3"))
        self.assertIsNone(result["comment"])
        self.assertEqual(result["confirm"]["branch"], "test/asus-keris")
        # The same report from a release is.
        plain = build_run(device_issue(ASUS.replace("0.9.0", "0.9.3"), "0b05:195c", version="0.9.3"))
        self.assertIn("0.9.3", plain["comment"])
        self.assertNotIn("confirm", plain)

    def test_the_branch_the_report_names_is_preferred(self):
        both = {**BRANCHES, "test/asus-more": [KERIS]}
        self.assertEqual(build_run(device_issue(BUILT, "0b05:195c"), both)["confirm"]["branch"], "test/asus-keris")
        other = BUILT.replace(BUILD, "test/asus-more@1234567")
        self.assertEqual(build_run(device_issue(other, "0b05:195c"), both)["confirm"]["branch"], "test/asus-more")

    def test_several_branches_and_no_name_leave_it_to_a_person(self):
        both = {**BRANCHES, "test/asus-more": [KERIS]}
        self.assertNotIn("confirm", build_run(device_issue(ASUS.replace("0.9.0", "0.9.5"), "0b05:195c", version="0.9.5"), both))
        # One branch has it: a report that does not name it still finds it.
        self.assertEqual(
            build_run(device_issue(ASUS.replace("0.9.0", "0.9.5"), "0b05:195c", version="0.9.5"))["confirm"]["branch"],
            "test/asus-keris",
        )

    def test_a_report_with_trouble_confirms_nothing_on_a_branch_either(self):
        bad = BUILT.replace("Changes: available (experimental)", "Changes: off (unexpected answer: polling rate code 09)")
        self.assertNotIn("confirm", build_run(device_issue(bad, "0b05:195c")))
        self.assertNotIn("confirm", build_run(device_issue(BUILT, "0b05:195c", wrong="the DPI is wrong")))
        self.assertNotIn("confirm", build_run(device_issue(BUILT, "0b05:195c", ticked=())))

    def test_a_device_on_main_or_in_a_release_does_not_use_a_branch(self):
        # On main (not released): as before, left to the maintainer.
        self.assertNotIn("confirm", build_run(device_issue(BUILT, "0b05:195c"), current=DEVICES))
        # In the release: confirmed as before, without a branch.
        self.assertEqual(build_run(device_issue(BUILT, "0b05:195c"), devices=DEVICES, current=DEVICES)["confirm"], {"usb": "0b05:195c"})

    def test_a_branch_that_has_it_confirmed_already_confirms_nothing(self):
        done = {"test/asus-keris": [{**KERIS, "level": "supported"}]}
        self.assertNotIn("confirm", build_run(device_issue(BUILT, "0b05:195c"), done))

    def test_without_branches_nothing_changes(self):
        self.assertNotIn("confirm", build_run(device_issue(BUILT, "0b05:195c"), None))
        self.assertNotIn("confirm", build_run(device_issue(BUILT, "0b05:195c"), {}))

    def test_a_pasted_report_of_a_test_build_is_recognized_and_not_told_to_update(self):
        body = BUILT.replace("PeriHub 0.9.5", "PeriHub 0.9.3").replace("Device: ", "Device: ", 1)
        result = build_run({"number": 7, "labels": [], "body": body})
        self.assertIn("test report", result["labels"])
        self.assertIsNone(result["comment"])
        self.assertNotIn("confirm", result)

    def test_the_branch_lookup_does_not_trust_the_report_for_a_branch_without_the_device(self):
        # The report names a branch that does not have the device: only the real one counts.
        liar = BUILT.replace(BUILD, "test/other@1234567")
        self.assertEqual(build_run(device_issue(liar, "0b05:195c"))["confirm"]["branch"], "test/asus-keris")


REQ_DEVICES = [
    {"brand": "Razer", "name": "Viper", "kind": "mouse", "level": "supported", "usb_ids": ["1532:0078"]},
    {"brand": "Logitech", "name": "G502", "kind": "mouse", "level": "supported", "usb_ids": ["046d:c08b"]},
    {"brand": "Cooler Master", "name": "MM711", "kind": "mouse", "level": "experimental", "usb_ids": ["2516:0101"]},
    # Recognized but nothing is sent to it: no driver to build on.
    {"brand": "Acme", "name": "Zap", "kind": "mouse", "level": "recognized", "usb_ids": ["abcd:0001"]},
]
REQ_UPSTREAM = {
    "libratbag": ["046d:4099", "abcd:0002"],
    "openrazer": ["1532:0099"],
    "rivalcfg": [],
    "OpenRGB": ["2516:0777", "1532:0098"],
}


def req(usb: str, labels=("device request",)) -> dict:
    body = f"### Device model\n\nSome device\n\n### USB ID (vendor:product)\n\n{usb}\n\n### What PeriHub saw\n\n_No response_"
    return {"labels": [{"name": n} for n in labels], "body": body}


def run_req(usb: str) -> dict:
    return triage.triage(req(usb), "0.9.0", REQ_DEVICES, upstream=REQ_UPSTREAM)


class DeviceRequestAnswers(unittest.TestCase):
    def test_a_known_id_gets_its_level_as_before(self):
        result = run_req("1532:0078")
        self.assertIn("already recognizes this device", result["comment"])
        self.assertIn("Razer Viper", result["comment"])
        self.assertEqual(result["labels"], ["brand: Razer", "mouse"])

    def test_an_id_another_project_documents_for_a_driver_perihub_has_can_be_added_directly(self):
        result = run_req("1532:0099")
        self.assertIn("can be added directly", result["comment"])
        self.assertIn("read-only or experimental", result["comment"])
        self.assertIn("test build", result["comment"])
        self.assertIn("posted on this issue", result["comment"])
        self.assertIn("Test my device", result["comment"])
        self.assertIn("next release", result["comment"])
        self.assertNotIn("next version", result["comment"])
        self.assertIn("openrazer", result["comment"])
        self.assertEqual(result["labels"], ["brand: Razer", "ready to add"])

    def test_an_id_only_a_project_of_another_vendor_lists_is_ready_to_add_too(self):
        # OpenRGB lists a Razer id: the protocol is documented, PeriHub's Razer driver is not needed to say so.
        result = run_req("1532:0098")
        self.assertEqual(result["labels"][-1], "ready to add")
        self.assertIn("OpenRGB", result["comment"])

    def test_a_documented_id_without_a_driver_needs_a_new_one_and_no_captures(self):
        # libratbag lists it, but PeriHub only recognizes Acme: a driver has to be written from the documentation.
        for usb, project in (("abcd:0002", "libratbag"), ("beef:0777", "OpenRGB")):
            with self.subTest(usb=usb):
                upstream = dict(REQ_UPSTREAM, OpenRGB=REQ_UPSTREAM["OpenRGB"] + ["beef:0777"])
                result = triage.triage(req(usb), "0.9.0", REQ_DEVICES, upstream=upstream)
                self.assertIn("documents its protocol", result["comment"])
                self.assertIn(project, result["comment"])
                self.assertIn("no USB captures are needed", result["comment"])
                self.assertIn("a little longer", result["comment"])
                self.assertIn("posted on this issue", result["comment"])
                self.assertIn("Test my device", result["comment"])
                self.assertNotIn("next version", result["comment"])
                self.assertNotIn(triage.CAPTURE_GUIDE, result["comment"])
                self.assertEqual(result["labels"][-1], "ready to add")
                self.assertNotIn("needs captures", result["labels"])

    def test_an_id_nobody_lists_gets_optional_captures_and_links_the_guide(self):
        result = run_req("dead:beef")
        # What the bot checked, not a promise of a search nobody makes (the answer #123 got).
        self.assertIn("What was checked automatically", result["comment"])
        self.assertIn("not in PeriHub's catalogue", result["comment"])
        self.assertIn("None of the projects PeriHub takes facts from", result["comment"])
        for gone in ("maintainer will look", "no public documentation", "look for documentation"):
            self.assertNotIn(gone, result["comment"])
        self.assertIn("optional", result["comment"])
        self.assertIn("Record the official software", result["comment"])
        self.assertIn(triage.CAPTURE_GUIDE, result["comment"])
        self.assertEqual(result["labels"], ["needs captures"])
        self.assertNotIn("OpenRGB lists", result["comment"])

    def test_a_missing_usb_id_is_asked_for_and_gets_no_next_step_label(self):
        result = triage.triage(req("_No response_"), "0.9.0", REQ_DEVICES, upstream=REQ_UPSTREAM)
        self.assertIn("USB ID is missing", result["comment"])
        self.assertEqual(result["labels"], [])

    def test_the_id_is_read_in_the_forms_windows_shows(self):
        result = run_req("VID_1532&PID_0099")
        self.assertEqual(result["labels"][-1], "ready to add")

    def test_without_upstream_data_an_unknown_id_needs_captures(self):
        result = triage.triage(req("1532:0099"), "0.9.0", REQ_DEVICES)
        self.assertEqual(result["labels"][-1], "needs captures")

    def test_other_issues_are_left_alone(self):
        result = triage.triage(req("dead:beef", labels=("bug",)), "0.9.0", REQ_DEVICES, upstream=REQ_UPSTREAM)
        self.assertEqual(result, {"comment": None, "labels": []})

    def test_a_request_pasted_as_text_gets_the_same_answer(self):
        body = "PeriHub 0.9.0 on Linux\nDevice: Some Mouse\nKind: Mouse\nUSB ID: 1532:0099 (USB)\n"
        result = triage.triage({"labels": [], "body": body}, "0.9.0", REQ_DEVICES, upstream=REQ_UPSTREAM)
        self.assertIn("device request", result["labels"])
        self.assertIn("ready to add", result["labels"])


G604 = """### Device model

Logitech G604_LIGHTSPEED

### USB ID (vendor:product)

046d:b024

### What PeriHub saw

```text
PeriHub 0.9.0 on windows x86_64
Device: Logitech G604_LIGHTSPEED
USB ID: 046d:b024 (Bluetooth)
HID collections:
  no interface number, usage page 0001, usage 0002
  no interface number, usage page 0001, usage 0006
  no interface number, usage page 000c, usage 0001
  no interface number, usage page ff43, usage 0202
```
"""
HIDPP = G604.replace("046d:b024", "046d:b999").replace(
    "no interface number, usage page ff43, usage 0202",
    "interface 2, usage page ff00, usage 0001\n  interface 2, usage page ff00, usage 0002",
)


class NoDocsButHidpp(unittest.TestCase):
    def run_body(self, body, upstream=None):
        issue = {"labels": [{"name": "device request"}], "body": body}
        return triage.triage(issue, "0.9.0", REQ_DEVICES, upstream=REQ_UPSTREAM if upstream is None else upstream)

    def test_the_g604_over_bluetooth_is_ready_to_add_when_libratbag_lists_it(self):
        upstream = dict(REQ_UPSTREAM, libratbag=REQ_UPSTREAM["libratbag"] + ["046d:b024"])
        result = self.run_body(G604, upstream)
        self.assertEqual(result["labels"][-1], "ready to add")
        self.assertIn("libratbag", result["comment"])

    def test_the_g604_over_bluetooth_is_recognized_once_a_release_lists_its_bluetooth_id(self):
        # Issue #43: the catalogue (site/devices.json) gives a Bluetooth id with the ids a request names, and as bluetooth_id.
        g604 = {
            "family": "logitech-onboard", "brand": "Logitech", "name": "G604", "kind": "mouse", "level": "by_memory_format",
            "usb_ids": ["046d:b024"], "wireless_id": "4085", "bluetooth_id": "b024",
        }
        issue = {"labels": [{"name": "device request"}], "body": G604}
        released = triage.triage(issue, "0.9.7", REQ_DEVICES + [g604], upstream=REQ_UPSTREAM)
        self.assertIn("already recognizes this device", released["comment"])
        self.assertIn("Logitech G604", released["comment"])
        # Only on main, in no release: never "already recognizes".
        on_main = triage.triage(issue, "0.9.7", REQ_DEVICES, current=REQ_DEVICES + [g604], upstream=REQ_UPSTREAM)
        self.assertNotIn("already recognizes", on_main["comment"] or "")

    def test_a_logitech_device_with_hidpp_collections_is_ready_to_add_without_any_project(self):
        result = self.run_body(HIDPP)
        self.assertEqual(result["labels"][-1], "ready to add")
        self.assertIn("HID++", result["comment"])
        self.assertIn("already supports", result["comment"])
        self.assertNotIn("auto_add", result)

    def test_without_hidpp_collections_the_same_logitech_id_needs_captures(self):
        result = self.run_body(G604.replace("046d:b024", "046d:b999").replace("usage page ff43, usage 0202", "usage page ff43, usage 0001"))
        self.assertEqual(result["labels"][-1], "needs captures")

    def test_the_bluetooth_hidpp_collection_counts_as_hidpp(self):
        # ff43/0202 is the HID++ collection of a Logitech mouse over Bluetooth (manager.rs, BLE_HIDPP_PAGE).
        result = self.run_body(G604.replace("046d:b024", "046d:b999"))
        self.assertEqual(result["labels"][-1], "ready to add")
        self.assertIn("HID++", result["comment"])
        self.assertNotIn("auto_add", result)

    def test_another_vendor_with_the_same_collections_is_not_told_it_speaks_hidpp(self):
        result = self.run_body(HIDPP.replace("046d:b999", "24ae:2015"))
        self.assertEqual(result["labels"][-1], "needs captures")

    def test_other_ff00_usages_are_not_hidpp(self):
        result = self.run_body(HIDPP.replace("usage 0001\n", "usage 000e\n").replace("usage 0002\n", "usage 00ff\n"))
        self.assertEqual(result["labels"][-1], "needs captures")


class OnTestBranch(unittest.TestCase):
    BRANCHES = {"test/viper-v3-pro-wireless": [{"usb_ids": ["1532:00c1"]}], "test/evision": [{"usb_ids": ["320f:505b"]}]}

    def run_branch(self, usb, upstream=None):
        return triage.triage(
            req(usb), "0.9.5", REQ_DEVICES, upstream=REQ_UPSTREAM if upstream is None else upstream, branches=self.BRANCHES
        )

    def test_an_id_on_a_test_branch_says_the_build_exists_and_how_to_get_it(self):
        result = self.run_branch("1532:00c1", dict(REQ_UPSTREAM, openrazer=["1532:00c1"]))
        c = result["comment"]
        self.assertIn("already exists, on the branch `test/viper-v3-pro-wireless`", c)
        self.assertIn("test-build.yml?query=branch%3Atest%2Fviper-v3-pro-wireless", c)
        self.assertIn("Test my device", c)
        self.assertNotIn("will be posted", c)
        self.assertEqual(result["labels"][-1], "ready to add")
        self.assertNotIn("auto_add", result)

    def test_the_same_when_no_project_lists_the_id(self):
        result = self.run_branch("320f:505b")
        self.assertIn("test%2Fevision", result["comment"])
        self.assertEqual(result["labels"][-1], "ready to add")

    def test_an_id_on_no_branch_is_unchanged(self):
        self.assertIn("built automatically", self.run_branch("1532:0099")["comment"])


class ReleaseRule(unittest.TestCase):
    """A device written from public documentation goes into the next release as experimental, without waiting for a
    tester: a test build comes earlier when one is made, and a clean report makes the device supported."""

    TEXTS = {
        "a maintainer writes it": triage.TEST_BUILD_STEPS,
        "written automatically": triage.AUTO_TEST_BUILD_STEPS.format(level="experimental"),
        "already on a test branch": triage.ON_BRANCH_STEPS,
    }

    def test_every_answer_says_it_goes_into_the_next_release_without_waiting(self):
        for name, text in self.TEXTS.items():
            with self.subTest(name):
                self.assertIn("next release", text)
                self.assertIn("experimental", text)
                self.assertIn("**supported**", text)
                self.assertIn("**Test my device**", text)
                self.assertIn("test build", text)

    def test_no_answer_says_a_tester_must_confirm_it_first(self):
        for name, text in self.TEXTS.items():
            with self.subTest(name):
                for old in ("draft", "never merged", "until a tester", "before a tester", "goes into the next release. Nothing more"):
                    self.assertNotIn(old, text)
                # The next release comes before the confirmation, not after it.
                self.assertLess(text.index("next release"), text.index("confirmed"))

    def test_the_answers_to_a_request_carry_it(self):
        # A Logitech id libratbag lists (a maintainer writes it), an OpenRazer one (written automatically) and one a
        # test branch has already.
        branches = {"test/evision": [{"usb_ids": ["320f:505b"]}]}
        for usb in ("046d:4099", "1532:0099", "320f:505b"):
            with self.subTest(usb=usb):
                comment = triage.triage(req(usb), "0.9.0", REQ_DEVICES, upstream=REQ_UPSTREAM, branches=branches)["comment"]
                self.assertIn("next release", comment)
                self.assertIn("experimental", comment)
                self.assertNotIn("draft", comment)
                self.assertNotIn("until a tester", comment)

    def test_the_module_says_what_ready_to_add_means(self):
        doc = " ".join(triage.__doc__.split())
        self.assertIn('"Ready to add" means it is added to the next release as experimental', doc)
        self.assertIn("nothing waits for a tester", doc)
        self.assertNotIn("never merged before a tester", doc)


class AutoAdd(unittest.TestCase):
    """"Mechanically addable": a project whose table a generator writes lists the id, nobody has it yet."""

    def issue(self, usb, number=7):
        return {**req(usb), "number": number}

    def test_an_openrazer_id_nobody_has_is_auto_added_and_the_answer_says_so(self):
        result = triage.triage(self.issue("1532:0099"), "0.9.0", REQ_DEVICES, upstream=REQ_UPSTREAM)
        self.assertEqual((result["auto_add"], result["issue"]), ("1532:0099", 7))
        self.assertEqual(result["labels"][-1], "ready to add")
        self.assertIn("written and built automatically", result["comment"])
        self.assertIn("posted on this issue", result["comment"])
        self.assertIn("Test my device", result["comment"])
        self.assertNotIn("A maintainer will look", result["comment"])

    def test_a_listed_id_without_a_generator_is_not(self):
        # libratbag's Logitech devices, rivalcfg's SteelSeries ones and OpenRGB's have no generator to add an id to.
        upstream = {"libratbag": ["046d:4099"], "openrazer": [], "rivalcfg": ["1038:1234"], "OpenRGB": ["1532:0098"]}
        devices = REQ_DEVICES + [{"brand": "SteelSeries", "name": "Rival", "kind": "mouse", "level": "experimental", "usb_ids": ["1038:0001"]}]
        for usb in ("046d:4099", "1038:1234", "1532:0098"):
            with self.subTest(usb=usb):
                result = triage.triage(self.issue(usb), "0.9.0", devices, upstream=upstream)
                self.assertEqual(result["labels"][-1], "ready to add")
                self.assertNotIn("auto_add", result)
                self.assertNotIn("written and built automatically", result["comment"])
                self.assertIn("posted on this issue automatically", result["comment"])
                # No promise of an automatic build: a maintainer writes these.
                self.assertIn("A maintainer adds it, as read-only or experimental, to the **next release**", result["comment"])

    def test_known_ids_and_ids_nobody_lists_are_not(self):
        for usb in ("1532:0078", "dead:beef"):
            with self.subTest(usb=usb):
                self.assertNotIn("auto_add", triage.triage(self.issue(usb), "0.9.0", REQ_DEVICES, upstream=REQ_UPSTREAM))

    def test_an_id_a_test_branch_has_already_is_not_started_again(self):
        branches = {"test/auto-1532-0099": [{"usb_ids": ["1532:0099"]}]}
        result = triage.triage(self.issue("1532:0099"), "0.9.0", REQ_DEVICES, upstream=REQ_UPSTREAM, branches=branches)
        self.assertNotIn("auto_add", result)
        self.assertNotIn("written and built automatically", result["comment"])

    def test_a_pasted_request_without_a_number_starts_nothing(self):
        self.assertNotIn("auto_add", triage.triage(req("1532:0099"), "0.9.0", REQ_DEVICES, upstream=REQ_UPSTREAM))

    def test_the_pure_function(self):
        up = {"openrazer": ["1532:0099", "1532:0078"], "libratbag": ["046d:4099"]}
        self.assertTrue(triage.auto_addable("1532:0099", up, REQ_DEVICES))
        self.assertFalse(triage.auto_addable("1532:0078", up, REQ_DEVICES))  # known
        self.assertFalse(triage.auto_addable("046d:4099", up, REQ_DEVICES))  # no generator
        self.assertFalse(triage.auto_addable("1532:0001", up, REQ_DEVICES))  # nobody lists it
        self.assertFalse(triage.auto_addable("1532:99", up, REQ_DEVICES))  # not a USB id
        self.assertFalse(triage.auto_addable("1532:0099", {}, REQ_DEVICES))

    def test_the_pairs_are_the_ones_auto_add_has_generators_for(self):
        import auto_add

        self.assertEqual(triage.MECHANICAL, set(auto_add.GENERATORS))

    def test_the_real_example_viper_v3_pro_dongle(self):
        seen = json.loads((triage.ROOT / "tools/upstream-seen.json").read_text())
        catalogue = json.loads((triage.ROOT / "site/devices.json").read_text())["devices"]
        if any("1532:00c1" in d["usb_ids"] for d in catalogue):
            self.skipTest("1532:00c1 is in the catalogue now")
        self.assertTrue(triage.auto_addable("1532:00c1", seen, catalogue))


# The device request of issue #111, as GitHub laid it out: PeriHub 0.9.6 took the SteelSeries Apex Pro for a mouse
# ("Kind: Mouse"), and the person who filled in the form picked Keyboard in the Device type dropdown.
APEX_PRO = """### Device model

SteelSeries Apex Pro

### Device type

Keyboard

### Brand

SteelSeries

### USB ID (vendor:product)

1038:1610

### What PeriHub saw

```text
PeriHub 0.9.6 on windows x86_64
Device: SteelSeries Apex Pro
Kind: Mouse
USB ID: 1038:1610 (USB)
Release number (bcdDevice): 0x0416
USB interfaces: not read
HID collections:
  interface 0, usage page 0001, usage 0002
  interface 0, usage page 0001, usage 0006
  interface 0, usage page 000c, usage 0001
  interface 1, usage page ffc0, usage 0001
  interface 2, usage page 0001, usage 0002
  interface 2, usage page 0001, usage 0006
  interface 2, usage page 000c, usage 0001
  interface 3, usage page 0001, usage 0002
  interface 3, usage page 000c, usage 0001
  interface 4, usage page ffc1, usage 0001
Report descriptors (lengths in bytes, with the id byte):
  interface 0, 81 bytes: usage page 0001, usage 0002: id 01 (in 8)
  interface 0, 62 bytes: usage page 0001, usage 0006: id 02 (in 34, out 2)
  interface 0, 25 bytes: usage page 000c, usage 0001: id 03 (in 5)
  interface 1, 51 bytes: usage page ffc0, usage 0001: no id (in 64, out 64, feature 642)
  interface 2, 81 bytes: usage page 0001, usage 0002: id 01 (in 8)
  interface 2, 62 bytes: usage page 0001, usage 0006: id 02 (in 34, out 2)
  interface 2, 25 bytes: usage page 000c, usage 0001: id 03 (in 5)
  interface 3, 73 bytes: usage page 0001, usage 0002: id 02 (in 6)
  interface 3, 25 bytes: usage page 000c, usage 0001: id 01 (in 5)
  interface 4, 21 bytes: usage page ffc1, usage 0001: no id (in 64)
Raw descriptors (hex):
  interface 0: [id hidden]
Feature reports (read only):
  none declared by a vendor-defined collection
```

### What the device can do in its official software

- [ ] DPI
- [ ] Polling rate
- [x] Lighting
- [x] Button remapping
- [x] Macros
- [x] Onboard profiles
- [ ] Battery level
- [ ] Something else (say what below)

### Anything else it can do

_No response_

### Official software it uses

_No response_

### Operating system

Windows

### Testing

- [x] I can run read-only diagnostic commands and share their output.
- [x] Optionally, if I am comfortable with it, I can make a USB capture of the official software with the [capture guide](https://github.com/hex-dump/perihub/blob/main/docs/captures.md)
"""
APEX_LABELS = ("device request", "mouse", "brand: SteelSeries", "needs captures")  # what the first triage put on it


class KeyboardTakenForAMouse(unittest.TestCase):
    """Issue #111: the SteelSeries Apex Pro (1038:1610) was labelled a mouse and "needs captures"."""

    seen = json.loads((ROOT / "tools/upstream-seen.json").read_text())

    def issue(self, body=APEX_PRO, labels=("device request",)):
        return {"number": 111, "labels": [{"name": n} for n in labels], "body": body}

    def test_the_id_is_listed_by_openrgb_in_the_real_baseline(self):
        self.assertIn("OpenRGB", triage.listed_by("1038:1610", self.seen))

    def test_the_request_is_ready_to_add_and_not_in_need_of_captures(self):
        result = triage.triage(self.issue(), "0.9.6", DEVICES, upstream=self.seen)
        self.assertIn("ready to add", result["labels"])
        self.assertNotIn("needs captures", result["labels"])
        self.assertIn("OpenRGB", result["comment"])
        self.assertIn("documents its protocol", result["comment"])
        self.assertIn("no USB captures are needed", result["comment"])
        self.assertNotIn("no public documentation", result["comment"])
        # OpenRGB is for lights: it is never added without a person (MECHANICAL stays as it is).
        self.assertNotIn("auto_add", result)

    def test_the_form_s_keyboard_beats_the_report_s_mouse_for_an_id_the_catalogue_lacks(self):
        result = triage.triage(self.issue(), "0.9.6", DEVICES, upstream=self.seen)
        self.assertEqual(result["labels"], ["brand: SteelSeries", "keyboard", "ready to add"])
        self.assertNotIn("remove_labels", result)

    def test_the_mouse_label_the_first_triage_put_on_it_is_taken_off(self):
        result = triage.triage(self.issue(labels=APEX_LABELS), "0.9.6", DEVICES, upstream=self.seen)
        self.assertEqual(result["labels"], ["keyboard", "ready to add"])
        self.assertEqual(result["remove_labels"], ["mouse"])

    def test_with_the_real_catalogue_too(self):
        catalogue = json.loads((ROOT / "site/devices.json").read_text())["devices"]
        if any("1038:1610" in d["usb_ids"] for d in catalogue):
            self.skipTest("1038:1610 is in the catalogue now")
        result = triage.triage(self.issue(labels=APEX_LABELS), "0.9.6", catalogue, catalogue, upstream=self.seen)
        self.assertEqual(result["labels"], ["keyboard", "ready to add"])
        self.assertEqual(result["remove_labels"], ["mouse"])

    def test_a_pasted_report_has_no_form_and_keeps_the_app_s_kind(self):
        saw = APEX_PRO.split("```text\n")[1].split("\n```")[0]
        result = triage.triage({"labels": [], "body": saw}, "0.9.6", DEVICES, upstream=self.seen)
        self.assertEqual(result["labels"][:2], ["device request", "mouse"])
        self.assertIn("ready to add", result["labels"])

    def test_the_dropdown_the_app_filled_in_agrees_with_the_report_so_nothing_changes(self):
        mouse_form = APEX_PRO.replace("### Device type\n\nKeyboard", "### Device type\n\nMouse")
        result = triage.triage(self.issue(mouse_form), "0.9.6", DEVICES, upstream=self.seen)
        self.assertEqual(result["labels"], ["brand: SteelSeries", "mouse", "ready to add"])

    def test_a_device_the_catalogue_knows_is_labelled_from_the_catalogue_whatever_the_form_says(self):
        known = [{"family": "steelseries", "brand": "SteelSeries", "name": "Apex Pro", "kind": "keyboard", "level": "experimental", "usb_ids": ["1038:1610"]}]
        for form in ("Mouse", "Keyboard", "Receiver"):
            with self.subTest(form=form):
                body = APEX_PRO.replace("### Device type\n\nKeyboard", f"### Device type\n\n{form}")
                result = triage.triage(self.issue(body, labels=APEX_LABELS), "0.9.6", known, upstream=self.seen)
                self.assertIn("keyboard", result["labels"])
                self.assertEqual(result.get("remove_labels"), ["mouse"])


class Catalogue(unittest.TestCase):
    """The data the answers come from: every project's ids in the files are well formed."""

    def test_upstream_seen_has_the_projects_the_answers_name(self):
        seen = json.loads((triage.ROOT / "tools/upstream-seen.json").read_text())
        for project in triage.PROJECT_VENDORS:
            self.assertIn(project, seen)
            self.assertIn(project, triage.UPSTREAM_URLS)

    def test_a_wireless_variant_openrazer_lists_is_ready_to_add_with_the_real_baseline(self):
        # Issue #85: 1532:00c1 (Viper V3 Pro, dongle) is a subclass of the wired class in OpenRazer and was
        # missing from the baseline, so the request was answered "needs captures".
        seen = json.loads((triage.ROOT / "tools/upstream-seen.json").read_text())
        self.assertIn("openrazer", triage.listed_by("1532:00c1", seen))
        result = triage.triage(req("1532:00c1"), "0.9.0", REQ_DEVICES, upstream=seen)
        self.assertEqual(result["labels"][-1], "ready to add")
        self.assertNotIn("needs captures", result["labels"])

    def test_a_vendor_mapped_to_a_project_has_a_driver_in_the_catalogue(self):
        devices = json.loads((triage.ROOT / "site/devices.json").read_text())["devices"]
        for vendors in triage.PROJECT_VENDORS.values():
            for vendor in vendors:
                self.assertIsNotNone(triage.driver_brand(vendor, devices), vendor)



TRIED_CLEAN = """
What I tried:
  worked: The name and USB ID PeriHub shows are right
  worked: PeriHub finds it again after I unplug it and plug it back in
  worked: The DPI PeriHub shows matches the mouse
  worked: The polling rate PeriHub shows is right"""


class TestMyDevice(unittest.TestCase):
    """A report made with Test my device: its checklist ("What I tried:") stands for the form's box of the other
    brands, which the procedure does not tick (issue #97)."""

    def test_a_clean_checklist_confirms_without_the_box(self):
        result = run(device_issue(ASUS + TRIED_CLEAN, "0b05:195c", ticked=()))
        self.assertEqual(result["confirm"], {"usb": "0b05:195c"})

    def test_a_check_that_did_not_work_confirms_nothing(self):
        failed = TRIED_CLEAN + "\n  did not work: The lighting PeriHub shows matches the mouse"
        self.assertNotIn("confirm", run(device_issue(ASUS + failed, "0b05:195c", ticked=())))

    def test_only_the_name_is_not_enough(self):
        name_only = "\nWhat I tried:\n  worked: The name and USB ID PeriHub shows are right"
        self.assertNotIn("confirm", run(device_issue(ASUS + name_only, "0b05:195c", ticked=())))

    def test_without_a_checklist_the_box_is_still_needed(self):
        self.assertNotIn("confirm", run(device_issue(ASUS, "0b05:195c", ticked=())))


def saw_request(usb, saw):
    body = (
        answer("Device model", "X")
        + answer("USB ID (vendor:product)", usb)
        + answer("What PeriHub saw", "```text\n" + saw + "\n```")
    )
    return {"number": 3, "labels": [{"name": "device request"}], "body": body}


PLAIN_MOUSE = """PeriHub 0.9.5 on windows x86_64
Device: Logitech USB Optical Mouse
Kind: Mouse
USB ID: 046d:c077 (USB)
HID collections:
  interface 0, usage page 0001, usage 0002"""


class StandardInterfacesOnly(unittest.TestCase):
    """A device whose collections are all standard (mouse, keyboard, media keys) has no channel to set it through."""

    devices = [{"family": "logitech-onboard", "brand": "Logitech", "name": "G403", "kind": "mouse", "level": "supported", "usb_ids": ["046d:c08f"]},
               {"family": "razer", "brand": "Razer", "name": "Viper", "kind": "mouse", "level": "supported", "usb_ids": ["1532:0078"]}]

    def test_a_logitech_id_libratbag_lists_without_hidpp_is_not_ready_to_add(self):
        # Issue #95. Not "needs captures" either: an id a project lists never is. A person checks which device it is.
        result = triage.triage(saw_request("046d:c077", PLAIN_MOUSE), "0.9.6", self.devices, upstream={"libratbag": ["046d:c077"]})
        self.assertEqual(result["labels"][-1], "needs maintainer")
        self.assertNotIn("needs captures", result["labels"])
        self.assertIn("libratbag", result["comment"])
        self.assertIn("only standard interfaces", result["comment"])
        self.assertIn("no USB captures are needed", result["comment"])
        self.assertNotIn("can be added directly", result["comment"])
        self.assertNotIn(triage.CAPTURE_GUIDE, result["comment"])

    def test_with_hidpp_it_is(self):
        saw = PLAIN_MOUSE + "\n  interface 1, usage page ff00, usage 0001"
        result = triage.triage(saw_request("046d:c077", saw), "0.9.6", self.devices, upstream={"libratbag": ["046d:c077"]})
        self.assertEqual(result["labels"][-1], "ready to add")

    def test_a_razer_mouse_is_set_through_its_standard_interface(self):
        saw = PLAIN_MOUSE.replace("046d:c077", "1532:00c1")
        result = triage.triage(saw_request("1532:00c1", saw), "0.9.6", self.devices, upstream={"openrazer": ["1532:00c1"]})
        self.assertEqual(result["labels"][-1], "ready to add")

    def test_nobody_lists_it_and_no_vendor_collection(self):
        saw = PLAIN_MOUSE.replace("046d:c077", "c0f4:03f5")
        result = triage.triage(saw_request("c0f4:03f5", saw), "0.9.6", self.devices, upstream={})
        self.assertEqual(result["labels"][-1], "needs captures")
        self.assertIn("no vendor-specific one", result["comment"])

    def test_nothing_seen_says_the_descriptors_were_missing(self):
        result = triage.triage(saw_request("c0f4:03f5", ""), "0.9.6", self.devices, upstream={})
        self.assertIn("no HID descriptors to compare", result["comment"])
        self.assertIn("Request support", result["comment"])
        self.assertNotIn("maintainer will look", result["comment"])
        self.assertEqual(result["labels"][-1], "needs captures")

class ReportInAnotherField(unittest.TestCase):
    """Issue #117: the Test report field left as the placeholder, the app's report pasted in "Messages and errors"
    after a sentence of the tester's, without a code block."""

    REPORT = (
        "PeriHub 0.9.7 on windows x86_64\n"
        "Device: EVision RGB keyboard 320f 5000 (keyboard), USB ID 320f:5000, protocol from OpenRGB, OpenEVision (GPL-2.0, facts only)\n"
        "Firmware: not read\n"
        "Changes: off (read-only)\n"
        "Transcript (every request and answer, for a replay test):\n"
        '{\n  "format": "perihub-transcript",\n  "events": [\n    {"op":"write","data":"04 0d 00 05 08","len":64}\n  ]\n}\n'
        "Writes this session: 0 sent, 0 refused"
    )
    PLACEHOLDER = "Paste the report here (PeriHub copied it to the clipboard)."

    def issue(self, notices, report=PLACEHOLDER, version="0.9.7"):
        body = (
            answer("Device type", "Keyboard") + answer("Brand", "Other") + answer("Device", "EVision RGB keyboard 320f 5000")
            + answer("USB ID (vendor:product)", "320f:5000") + answer("PeriHub version", version)
            + answer("Test report", f"```text\n{report}\n```")
            + boxes(FORM, "matches", "What PeriHub shows correctly", [*triage.SHOWS_MOUSE])
            + answer("Messages and errors", notices) + answer("What is wrong or missing", None)
        )
        return {"number": 117, "labels": [{"name": "test report"}], "body": body}

    def test_the_report_is_found_after_free_text(self):
        issue = self.issue("it's a reddragon K580 RBG\n" + self.REPORT)
        lines = triage.app_report(triage.fields(issue["body"]))
        self.assertEqual(lines[0], "PeriHub 0.9.7 on windows x86_64")
        self.assertEqual(lines[-1], "Writes this session: 0 sent, 0 refused")
        self.assertNotIn("it's a reddragon K580 RBG", lines)

    def test_it_stops_at_the_writes_line(self):
        issue = self.issue(self.REPORT + "\nthanks, it works")
        lines = triage.app_report(triage.fields(issue["body"]))
        self.assertEqual(lines[-1], "Writes this session: 0 sent, 0 refused")

    def test_an_empty_report_field_is_no_report_and_the_one_in_another_field_is_found(self):
        # PeriHub no longer fills the field in when the report is too long for a link: it is left empty.
        for empty in ("", None, "_No response_"):
            body = answer("Test report", empty) + answer("Messages and errors", self.REPORT)
            lines = triage.app_report(triage.fields(body))
            self.assertEqual(lines[0], "PeriHub 0.9.7 on windows x86_64", empty)
        body = answer("Test report", None) + answer("Messages and errors", "nothing to add")
        self.assertIsNone(triage.app_report(triage.fields(body)))

    def test_the_placeholder_alone_is_no_report(self):
        self.assertIsNone(triage.app_report(triage.fields(self.issue("nothing to add")["body"])))

    def test_nothing_is_confirmed_and_the_labels_are_right(self):
        result = run(self.issue("it's a reddragon K580 RBG\n" + self.REPORT))
        self.assertNotIn("confirm", result)
        self.assertEqual(result["labels"], ["brand: Other", "keyboard"])
        self.assertIsNone(result["comment"])

    def test_an_older_version_in_the_wrong_field_is_still_asked_to_update(self):
        result = run(self.issue(self.REPORT.replace("0.9.7", "0.9.5"), version="0.9.5"), latest="0.9.7")
        self.assertIn("0.9.5", result["comment"])
        self.assertNotIn("confirm", result)


class HeadsetRequests(unittest.TestCase):
    """PeriHub only supports mice, keyboards, and receivers: headset and audio requests close as not planned."""

    def test_telephony_headset_from_perihub_is_closed_even_without_headset_in_the_name(self):
        saw = (
            "PeriHub 0.9.7 on windows x86_64\n"
            "Device: Logitech PRO X\n"
            "Kind: Headset\n"
            "USB ID: 046d:0aaa (USB)\n"
            "HID collections:\n"
            "  interface 3, usage page 000b, usage 0005\n"
            "  interface 3, usage page 000c, usage 0001\n"
            "  interface 3, usage page ff43, usage 0202"
        )
        body = (
            answer("Device model", "Logitech PRO X")
            + answer("Brand", "Logitech")
            + answer("USB ID (vendor:product)", "046d:0aaa")
            + answer("What PeriHub saw", "```text\n" + saw + "\n```")
        )
        issue = {
            "number": 136,
            "title": "Device request: Logitech PRO X",
            "labels": [{"name": "device request"}, {"name": "needs captures"}],
            "body": body,
        }
        result = triage.triage(issue, "0.9.7", [], upstream={"OpenRGB": ["046d:0aaa"]})
        self.assertEqual(result["close"], "not_planned")
        self.assertEqual(result["labels"], ["brand: Logitech", "wontfix"])
        self.assertEqual(result["remove_labels"], ["needs captures"])
        self.assertIn("mice, keyboards, and their receivers", result["comment"])

    def test_named_headset_drops_mistaken_mouse_kind_label(self):
        body = (
            answer("Device model", "HyperX Cloud II Headset")
            + answer("Device type", "Mouse")
            + answer("Brand", "Other")
            + answer("USB ID (vendor:product)", "0951:16a4")
        )
        issue = {"number": 200, "title": "Device request: HyperX Cloud II Headset", "labels": [{"name": "device request"}], "body": body}
        result = triage.triage(issue, "0.9.7", [])
        self.assertEqual(result["close"], "not_planned")
        self.assertEqual(result["labels"], ["brand: Other", "wontfix"])

    def test_a_mouse_or_keyboard_is_not_mistaken_for_a_headset(self):
        result = triage.triage(saw_request("046d:c077", PLAIN_MOUSE), "0.9.7", StandardInterfacesOnly.devices, upstream={})
        self.assertNotIn("close", result)
        self.assertNotIn("wontfix", result["labels"])


# Issue #80 as it came in (PeriHub 0.9.4: collections, no report descriptors yet).
SURMEN_V400 = (
    answer("Device model", "Evision RGB Keyboard") + answer("Device type", "Keyboard") + answer("Brand", "Other")
    + answer("USB ID (vendor:product)", "320f:505b")
    + answer("What PeriHub saw", """```text
PeriHub 0.9.4 on linux x86_64
Device: Evision RGB Keyboard
USB ID: 320f:505b (USB)
HID collections:
  interface 0, usage page 0001, usage 0006
  interface 1, usage page 0001, usage 0002
  interface 1, usage page 0001, usage 0006
  interface 1, usage page 0001, usage 0080
  interface 1, usage page 000c, usage 0001
  interface 1, usage page ff1c, usage 0092
```""")
)
# What the answers that said a listed or known device was not in the list wrote (issues #80, #111, #123).
NOT_IN_THE_LIST = ("no public documentation", "no protocol to build on", "gives PeriHub no protocol", "None of the projects")
# A vendor collection no family of platforms.py has, for requests with descriptors.
OTHER_VENDOR_SAW = """PeriHub 0.9.7 on linux x86_64
Device: Something
Kind: Mouse
USB ID: {usb} (USB)
HID collections:
  interface 0, usage page 0001, usage 0002
  interface 1, usage page ff77, usage 0001"""
STANDARD_ONLY_SAW = """PeriHub 0.9.7 on linux x86_64
Device: Something
Kind: Mouse
USB ID: {usb} (USB)
HID collections:
  interface 0, usage page 0001, usage 0002"""


def request(body, number=7):
    return {"number": number, "labels": [{"name": "device request"}], "body": body}


def real_catalogue():
    return json.loads((triage.ROOT / "site/devices.json").read_text())["devices"]


def real_upstream():
    return json.loads((triage.ROOT / "tools/upstream-seen.json").read_text())


class EveryKnownIdIsKnown(unittest.TestCase):
    """The answers that said a device "is not in PeriHub" when it was: every id of the catalogue, in every form a
    request writes it, gets "already recognizes" (or "already added", when only main has it), never a next step."""

    @classmethod
    def setUpClass(cls):
        cls.devices = real_catalogue()
        cls.upstream = real_upstream()

    def every_id(self):
        for d in self.devices:
            for usb, how in triage.catalogue_ids(d).items():
                yield d, usb, how

    def assert_known(self, result, usb, d, *, released=True):
        comment = result["comment"] or ""
        self.assertIn("already recognizes" if released else "already added", comment, usb)
        self.assertIn(d["name"], comment, usb)
        self.assertNotIn(f"`{usb}` is not in", comment, usb)
        for phrase in NOT_IN_THE_LIST:
            self.assertNotIn(phrase, comment, usb)
        self.assertFalse({"ready to add", "needs captures", "needs maintainer"} & set(result["labels"]), usb)
        self.assertNotIn("auto_add", result, usb)

    def test_the_catalogue_has_ids_of_every_kind(self):
        kinds = {how for _, _, how in self.every_id()}
        self.assertEqual(kinds, {"usb", "wireless", "bluetooth"})
        receivers = {usb for d, usb, _ in self.every_id() if d["kind"] == "receiver"}
        for usb in ("046d:c539", "046d:c53a", "046d:c547", "046d:c548", "046d:c52b", "046d:c545", "046d:c541"):
            self.assertIn(usb, receivers)

    def test_every_id_in_the_form_s_field_in_every_way_it_is_written(self):
        for d, usb, how in self.every_id():
            v, p = usb.split(":")
            for typed in (usb, usb.upper(), f"0x{v}:0x{p}", f" {v} : {p} ", f"VID_{v.upper()}&PID_{p.upper()}&MI_02",
                          f"vid {v} pid {p}", f"{v} {p}", f"`{usb}`", f"Product ID: 0x{p}\nVendor ID: 0x{v}  (Some Inc.)"):
                with self.subTest(usb=usb, typed=typed):
                    body = answer("Device model", "X") + answer("USB ID (vendor:product)", typed)
                    result = triage.triage(request(body), "0.9.7", self.devices, upstream=self.upstream)
                    self.assert_known(result, usb, d)

    def test_every_id_in_perihub_s_own_report_with_the_field_empty(self):
        for d, usb, how in self.every_id():
            with self.subTest(usb=usb):
                saw = STANDARD_ONLY_SAW.format(usb=usb)
                body = answer("Device model", "X") + answer("USB ID (vendor:product)", None) + answer("What PeriHub saw", f"```text\n{saw}\n```")
                result = triage.triage(request(body), "0.9.7", self.devices, upstream=self.upstream)
                self.assert_known(result, usb, d)

    def test_every_id_pasted_as_a_report(self):
        for d, usb, how in self.every_id():
            with self.subTest(usb=usb):
                result = triage.triage({"labels": [], "body": STANDARD_ONLY_SAW.format(usb=usb)}, "0.9.7", self.devices, upstream=self.upstream)
                self.assertIn("device request", result["labels"])
                self.assert_known(result, usb, d)

    def test_every_id_only_main_has_comes_with_the_next_release(self):
        for d, usb, how in self.every_id():
            released = [o for o in self.devices if o is not d]
            if triage.find([usb], released):
                continue  # another device of the release has the same id
            with self.subTest(usb=usb):
                body = answer("Device model", "X") + answer("USB ID (vendor:product)", usb)
                result = triage.triage(request(body), "0.9.7", released, self.devices, upstream=self.upstream)
                self.assert_known(result, usb, d, released=False)
                self.assertIn("next release", result["comment"])


class NoListedIdNeedsCaptures(unittest.TestCase):
    """An id OpenRGB, OpenRazer, libratbag or rivalcfg lists is documented: never "needs captures", never "no public
    documentation" or "no protocol to build on" (issue #80), whatever the request shows."""

    def test_every_id_of_the_baseline(self):
        devices, upstream = real_catalogue(), real_upstream()
        ids = sorted({usb for listed in upstream.values() for usb in listed})
        self.assertGreater(len(ids), 1400)
        for usb in ids:
            for saw in ("", STANDARD_ONLY_SAW.format(usb=usb), OTHER_VENDOR_SAW.format(usb=usb)):
                with self.subTest(usb=usb, saw=saw[-30:]):
                    body = answer("Device model", "X") + answer("USB ID (vendor:product)", usb)
                    if saw:
                        body += answer("What PeriHub saw", f"```text\n{saw}\n```")
                    result = triage.triage(request(body), "0.9.7", devices, upstream=upstream)
                    self.assertNotIn("needs captures", result["labels"])
                    for phrase in NOT_IN_THE_LIST:
                        self.assertNotIn(phrase, result["comment"] or "")
                    self.assertNotIn(triage.CAPTURE_GUIDE, result["comment"] or "")

    def test_and_with_nothing_of_the_catalogue(self):
        # As if the release knew none of them: the answer comes from the projects alone.
        upstream = real_upstream()
        for project, ids in upstream.items():
            for usb in ids:
                with self.subTest(usb=usb):
                    result = triage.triage(request(answer("USB ID (vendor:product)", usb)), "0.9.7", [], upstream=upstream)
                    self.assertNotIn("needs captures", result["labels"])
                    self.assertIn("ready to add", result["labels"])


class WrongAnswersOfTheBot(unittest.TestCase):
    """The requests the bot told "not in the list", answered again with what was known then."""

    @classmethod
    def setUpClass(cls):
        cls.devices = real_catalogue()
        cls.upstream = real_upstream()

    def without(self, *ids):
        return [d for d in self.devices if not any(triage.knows(usb, d) for usb in ids)]

    def test_80_the_surmen_v400_is_documented_by_openrgb_and_has_the_evision_collection(self):
        self.assertIn("320f:505b", self.upstream["OpenRGB"])
        then = self.without("320f:505b")
        result = triage.triage(request(SURMEN_V400, 80), "0.9.4", then, upstream=self.upstream)
        self.assertEqual(result["labels"][-1], "ready to add")
        self.assertIn("EVision keyboards", result["comment"])
        for phrase in NOT_IN_THE_LIST:
            self.assertNotIn(phrase, result["comment"])
        # And now that main and the release have it:
        now = triage.triage(request(SURMEN_V400, 80), "0.9.7", self.devices, upstream=self.upstream)
        self.assertIn("already recognizes this device", now["comment"])

    def test_80_without_any_collection_openrgb_alone_is_enough(self):
        body = answer("USB ID (vendor:product)", "320f:505b")
        result = triage.triage(request(body, 80), "0.9.4", self.without("320f:505b"), upstream=self.upstream)
        self.assertEqual(result["labels"][-1], "ready to add")
        self.assertIn("OpenRGB", result["comment"])
        self.assertIn("no USB captures are needed", result["comment"])

    def test_111_the_apex_pro_openrgb_lists(self):
        self.assertIn("1038:1610", self.upstream["OpenRGB"])
        result = triage.triage(request(APEX_PRO, 111), "0.9.6", self.without("1038:1610"), upstream=self.upstream)
        self.assertEqual(result["labels"][-1], "ready to add")
        self.assertIn("OpenRGB", result["comment"])
        for phrase in NOT_IN_THE_LIST:
            self.assertNotIn(phrase, result["comment"])

    def test_123_the_x68he_on_main_comes_with_the_next_release(self):
        issue = dict(saw_request("2e3c:c365", X68HE_SAW), number=123)
        result = triage.triage(issue, "0.9.7", self.without("2e3c:c365"), self.devices, upstream=self.upstream)
        self.assertIn("already added", result["comment"])
        self.assertIn("next release", result["comment"])
        self.assertNotIn("auto_add", result)
        # Before it was on main: the family of the W60, added without a person.
        before = triage.triage(issue, "0.9.7", self.without("2e3c:c365"), upstream=self.upstream)
        self.assertEqual(before["auto_add"], "2e3c:c365")
        self.assertEqual(before["labels"][-1], "ready to add")


# A request from a mouse behind its LIGHTSPEED receiver: PeriHub's report gives the receiver's id, the form the mouse's.
RECEIVER_SAW = """PeriHub 0.9.7 on windows x86_64
Device: USB Receiver
Kind: Mouse
USB ID: 046d:c539 (USB)
HID collections:
  interface 2, usage page ff00, usage 0001
  interface 2, usage page ff00, usage 0002"""


class BehindAReceiver(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.devices = real_catalogue()

    def run_body(self, body, devices=None, current=None):
        return triage.triage(request(body), "0.9.7", devices or self.devices, current, upstream=real_upstream())

    def test_the_id_a_logitech_mouse_has_behind_its_receiver(self):
        # 046d:4087 is the G903 HERO's wireless id; Linux shows it for the mouse's own HID device.
        result = self.run_body(answer("USB ID (vendor:product)", "046d:4087"))
        self.assertIn("already recognizes this device: **Logitech G903 HERO**", result["comment"])
        self.assertIn("the id it has behind its receiver", result["comment"])
        self.assertIn("mouse", result["labels"])

    def test_a_mouse_the_catalogue_has_only_by_its_wireless_id(self):
        # The G305 has no USB id of its own: only its LIGHTSPEED one, 046d:4074.
        result = self.run_body(answer("USB ID (vendor:product)", "046D:4074"))
        self.assertIn("**Logitech G305**", result["comment"])

    def test_the_g604_over_bluetooth(self):
        result = self.run_body(answer("USB ID (vendor:product)", "046d:b024"))
        self.assertIn("**Logitech G604**", result["comment"])
        self.assertIn("its Bluetooth id", result["comment"])

    def test_the_receiver_s_id_alone_is_the_receiver(self):
        body = answer("USB ID (vendor:product)", None) + answer("What PeriHub saw", f"```text\n{RECEIVER_SAW}\n```")
        result = self.run_body(body)
        self.assertIn("already recognizes this receiver: **Logitech LIGHTSPEED receiver**", result["comment"])
        self.assertNotIn("is not in", result["comment"])

    def test_the_mouse_in_the_form_beats_the_receiver_in_the_report(self):
        body = answer("USB ID (vendor:product)", "046d:c091") + answer("What PeriHub saw", f"```text\n{RECEIVER_SAW}\n```")
        result = self.run_body(body)
        self.assertIn("already recognizes this device: **Logitech G903 HERO**", result["comment"])

    def test_several_ids_in_the_field(self):
        result = self.run_body(answer("USB ID (vendor:product)", "receiver 046D:C539, mouse 046d:c08b"))
        self.assertIn("**Logitech G502 HERO**", result["comment"])

    def test_a_test_report_through_the_receiver_names_the_mouse(self):
        # MOUSE_RECEIVER: "USB ID: 046d:c539" and "wireless id 4070" (the G703): the labels are the mouse's.
        issue = mouse_issue(MOUSE_RECEIVER, usb="046d:c539", connection="LIGHTSPEED receiver")
        result = triage.triage(issue, "0.9.0", self.devices)
        self.assertIn("mouse", result["labels"])
        self.assertIn("brand: Logitech", result["labels"])
        self.assertNotIn("confirm", result)
        self.assertEqual(triage.report_ids(MOUSE_RECEIVER), ["046d:4070", "046d:c539"])


class FieldAndReport(unittest.TestCase):
    """The form's USB ID field and the id of PeriHub's own report: both are read, and the report wins when they differ."""

    def test_the_report_s_id_is_the_one_answered(self):
        saw = STANDARD_ONLY_SAW.format(usb="1532:0078")
        body = answer("USB ID (vendor:product)", "dead:beef") + answer("What PeriHub saw", f"```text\n{saw}\n```")
        result = triage.triage(request(body), "0.9.0", REQ_DEVICES, upstream=REQ_UPSTREAM)
        self.assertIn("already recognizes this device: **Razer Viper**", result["comment"])
        self.assertIn("The form's USB ID field says `dead:beef` (which is not in PeriHub's catalogue)", result["comment"])

    def test_a_known_id_in_the_field_is_named_when_the_report_s_is_not(self):
        saw = OTHER_VENDOR_SAW.format(usb="dead:beef")
        body = answer("USB ID (vendor:product)", "1532:0078") + answer("What PeriHub saw", f"```text\n{saw}\n```")
        result = triage.triage(request(body), "0.9.0", REQ_DEVICES, upstream=REQ_UPSTREAM)
        self.assertIn("`dead:beef`", result["comment"])
        self.assertIn("which PeriHub 0.9.0 recognizes as **Razer Viper**", result["comment"])
        # The known id of the field is never told it is not in PeriHub, whatever project lists it.
        self.assertNotIn("`1532:0078` is not in", result["comment"])
        listed = triage.triage(request(body), "0.9.0", REQ_DEVICES, upstream={"openrazer": ["1532:0078"]})
        self.assertNotIn("`1532:0078` is not in", listed["comment"])
        self.assertIn("which PeriHub 0.9.0 recognizes as **Razer Viper**", listed["comment"])

    def test_a_documented_id_in_the_field_is_answered_when_the_report_s_is_unknown(self):
        saw = OTHER_VENDOR_SAW.format(usb="dead:beef")
        body = answer("USB ID (vendor:product)", "1532:0099") + answer("What PeriHub saw", f"```text\n{saw}\n```")
        result = triage.triage(request(body), "0.9.0", REQ_DEVICES, upstream=REQ_UPSTREAM)
        self.assertEqual(result["labels"][-1], "ready to add")
        self.assertIn("PeriHub's own report says `dead:beef`", result["comment"])
        self.assertIn("`1532:0099`, the id the form's USB ID field gives", result["comment"])
        # Auto-add writes the id with the descriptors of the request: never another device's.
        self.assertNotIn("auto_add", result)

    def test_the_same_id_written_differently_is_no_divergence(self):
        saw = STANDARD_ONLY_SAW.format(usb="1532:0078")
        body = answer("USB ID (vendor:product)", "VID_1532&PID_0078") + answer("What PeriHub saw", f"```text\n{saw}\n```")
        result = triage.triage(request(body), "0.9.0", REQ_DEVICES, upstream=REQ_UPSTREAM)
        self.assertNotIn("form's USB ID field", result["comment"])

    def test_a_documented_id_among_several_is_the_one_answered(self):
        # The first id nobody lists, the second documented: "ready to add", not "needs captures".
        result = triage.triage(request(answer("USB ID (vendor:product)", "dead:beef / 1532:0099")), "0.9.0", REQ_DEVICES, upstream=REQ_UPSTREAM)
        self.assertEqual(result["labels"][-1], "ready to add")
        self.assertIn("1532:0099", result["comment"])

    def test_the_ids_are_read_in_every_form(self):
        for text, ids in (
            ("046D:C08B", ["046d:c08b"]),
            ("0x046d:0xc08b", ["046d:c08b"]),
            ("USB\\VID_046D&PID_C08B&MI_02\\7&1234", ["046d:c08b"]),
            ("Product ID: 0xc08b\nVendor ID: 0x046d  (Logitech Inc.)", ["046d:c08b"]),
            ("046d c08b", ["046d:c08b"]),
            ("046d:c539 and 046d:c08b", ["046d:c539", "046d:c08b"]),
            ("released 2026-11-26 15:04", []),
            ("_No response_", []),
        ):
            with self.subTest(text=text):
                self.assertEqual(triage.usb_ids(text), ids)


if __name__ == "__main__":
    unittest.main()

# The request of issue #123 (X68HE), as PeriHub 0.9.7 wrote it: the collection and reports of the Wraith W60's family.
X68HE_SAW = """PeriHub 0.9.7 on windows x86_64
Device: (Standardsystemgeräte) X68HE
Kind: Keyboard
USB ID: 2e3c:c365 (USB)
HID collections:
  interface 0, usage page 0001, usage 0006
  interface 1, usage page 0001, usage 0002
  interface 1, usage page 0001, usage 0006
  interface 1, usage page 0001, usage 0080
  interface 1, usage page 000c, usage 0001
  interface 2, usage page ff1b, usage 0091
Report descriptors (lengths in bytes, with the id byte):
  interface 0, 66 bytes: usage page 0001, usage 0006: no id (in 8, out 1)
  interface 1, 66 bytes: usage page 0001, usage 0002: id 02 (in 7)
  interface 1, 25 bytes: usage page 0001, usage 0006: id 05 (in 21)
  interface 1, 29 bytes: usage page 0001, usage 0080: id 04 (in 2)
  interface 1, 25 bytes: usage page 000c, usage 0001: id 03 (in 3)
  interface 2, 40 bytes: usage page ff1b, usage 0091: id 01 (in 64, out 64)
Raw descriptors (hex):
  interface 2: 061bff0991a10185011900293e150026ff007508953f81021900293e150026ff007508953f9102c0"""
# The request of issue #69 (0c45:8513, PeriHub 0.9.4): EVision's collection, no report descriptors yet.
SGK30_SAW = """PeriHub 0.9.4 on windows x86_64
Device: SONiX USB DEVICE
USB ID: 0c45:8513 (USB)
HID collections:
  interface 0, usage page 0001, usage 0006
  interface 1, usage page 0001, usage 0006
  interface 1, usage page 0001, usage 0080
  interface 1, usage page 000c, usage 0001
  interface 1, usage page ff1c, usage 0092"""


class Platforms(unittest.TestCase):
    """A request whose descriptors show the configuration collection of a family PeriHub has a driver for (#123)."""

    W60 = {"family": "wraith", "brand": "Wraith", "name": "W60", "kind": "keyboard", "level": "read_only", "usb_ids": ["2e3c:c368"]}

    def run_saw(self, usb, saw, number=123, devices=None):
        issue = dict(saw_request(usb, saw), number=number)
        return triage.triage(issue, "0.9.7", devices or [self.W60], upstream={})

    def test_the_x68he_is_added_automatically_read_only(self):
        result = self.run_saw("2e3c:c365", X68HE_SAW)
        self.assertEqual(result["labels"][-1], "ready to add")
        self.assertEqual(result["auto_add"], "2e3c:c365")
        self.assertEqual(result["platform"], "wraith")
        self.assertEqual(result["issue"], 123)
        c = result["comment"]
        self.assertIn("is added automatically, read-only", c)
        self.assertIn("usage page ff1b, usage 0091, report id 01 (in 64, out 64 bytes)", c)
        self.assertIn("On Linux it is not opened", c)
        self.assertNotIn("documentation", c)

    def test_a_different_report_is_no_match(self):
        saw = X68HE_SAW.replace("id 01 (in 64, out 64)", "id 01 (in 32, out 32)")
        result = self.run_saw("2e3c:c365", saw)
        self.assertEqual(result["labels"][-1], "needs captures")
        self.assertIn("usage page ff1b, usage 0091 on interface 2", result["comment"])
        self.assertNotIn("auto_add", result)

    def test_another_vendor_with_the_same_collection_is_not_recognized(self):
        result = self.run_saw("1234:c365", X68HE_SAW.replace("2e3c:c365", "1234:c365"))
        self.assertNotIn("auto_add", result)
        self.assertEqual(result["labels"][-1], "needs captures")

    def test_a_collection_without_its_report_descriptor_is_left_to_a_maintainer(self):
        result = self.run_saw("0c45:8513", SGK30_SAW, number=69)
        self.assertEqual(result["labels"][-1], "ready to add")
        self.assertIn("EVision keyboards", result["comment"])
        self.assertIn("no report descriptor", result["comment"])
        self.assertNotIn("auto_add", result)

    def test_an_id_a_device_has_already_is_not_added_twice(self):
        known = dict(self.W60, usb_ids=["2e3c:c368", "2e3c:c365"])
        result = triage.triage(dict(saw_request("2e3c:c365", X68HE_SAW), number=123), "0.9.7", [self.W60], current=[known], upstream={})
        self.assertNotIn("auto_add", result)

    def test_a_qmk_keyboard_is_found_by_its_collection(self):
        saw = "PeriHub 0.9.7 on linux x86_64\nDevice: Board\nUSB ID: feed:6060 (USB)\nHID collections:\n  interface 1, usage page ff60, usage 0061"
        result = self.run_saw("feed:6060", saw)
        self.assertIn("finds by itself", result["comment"])
        self.assertNotIn("auto_add", result)

    def test_the_workflow_reads_the_platform_from_the_json(self):
        self.assertTrue(triage.auto_addable("2e3c:c365", {}, [self.W60], None, X68HE_SAW))
        self.assertFalse(triage.auto_addable("2e3c:c365", {}, [self.W60], None, SGK30_SAW))
        self.assertFalse(triage.auto_addable("2e3c:c368", {}, [self.W60], None, X68HE_SAW.replace("c365", "c368")))
