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
Memory: model 1, profile format 4, macro format 1, 16 sectors of 255 bytes, 5 factory profiles
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
        self.assertIn("WRITABLE_PROFILE_FORMATS: &[u8] = &[2, 3];", source)
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
        self.none(mouse_issue(MOUSE.replace("profile format 2", "profile format 4")))
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
        self.assertEqual(run(other)["labels"], ["brand: ASUS", "keyboard"])

    def test_a_report_from_an_older_version_is_asked_to_try_the_latest(self):
        result = run(device_issue(ASUS.replace("0.9.0", "0.8.4"), "0b05:195c", version="0.8.4"))
        self.assertIn("0.8.4", result["comment"])
        self.assertNotIn("confirm", result)

    def test_a_request_for_a_device_only_on_main_is_left_to_the_maintainer(self):
        released = [d for d in DEVICES if d["family"] != "asus"]
        body = answer("Device model", "ROG Keris") + answer("USB ID (vendor:product)", "0b05:195c")
        issue = {"number": 1, "labels": [{"name": "device request"}], "body": body}
        result = triage.triage(issue, LATEST, released, DEVICES, upstream={"libratbag": ["0b05:195c"]})
        self.assertIsNone(result["comment"])
        self.assertNotIn("ready to add", result["labels"])

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
    """A new device is on a test/* branch until a tester confirms it with that branch's test build."""

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
        self.assertIn("A maintainer will look for documentation", result["comment"])
        self.assertIn("optional", result["comment"])
        self.assertIn("never required", result["comment"])
        self.assertIn(triage.CAPTURE_GUIDE, result["comment"])
        self.assertEqual(result["labels"], ["needs captures"])
        self.assertNotIn("OpenRGB", result["comment"])

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

    def test_a_logitech_device_with_hidpp_collections_is_ready_to_add_without_any_project(self):
        result = self.run_body(HIDPP)
        self.assertEqual(result["labels"][-1], "ready to add")
        self.assertIn("HID++", result["comment"])
        self.assertIn("already supports", result["comment"])
        self.assertNotIn("auto_add", result)

    def test_without_hidpp_collections_the_same_logitech_id_needs_captures(self):
        result = self.run_body(G604.replace("046d:b024", "046d:b999"))
        self.assertEqual(result["labels"][-1], "needs captures")

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
                self.assertIn("will be posted on this issue", result["comment"])

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


if __name__ == "__main__":
    unittest.main()