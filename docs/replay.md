# Replay tests: a user's report as a test

A **transcript** is every request PeriHub sent to a device and every answer, in full, as a JSON
file. A **replay test** plays it back to the code that reads the device, with no device connected,
so a driver can be checked in CI against what a real device answered. The code is in
`crates/perihub-core/src/replay/`; the transcripts are in
`crates/perihub-core/tests/fixtures/transcripts/`, and `cargo test` replays every one of them.

## What a replay checks

- Every request the code sends must be in the transcript (same collection, same bytes; `xx` in
  the transcript matches any byte). A request it does not have **fails the test**: the code changed
  what it asks, or the transcript is missing a step.
- A command that changes something **fails the test** when the transcript is
  `"access": "read-only"`, even though the replay opens the device the way the app does, allowed to
  change things. What counts as a change:
  - a driver of `peripheral/`: a `Change` or `Save` of its command list;
  - HID++: any feature call outside the read calls of `hidpp/mod.rs` (`READ_ONLY_CALLS`), a root
    feature function other than 0 and 1, a receiver register write (sub ids `0x80` and `0x82`;
    `0x81` and `0x83` read). The feature at each index is learnt from the transcript's own answers
    to the root feature, on a cable (device index `0xFF`) as behind a receiver; a call to an index
    the transcript never looked up is not a request the code may send, and fails the test too;
  - Corsair: any packet other than "get property" (`corsair::allowed` without lighting).

  When PeriHub records a transcript it uses the same rules to write `access`, and a request it
  cannot tell counts as a change: such a transcript is `read-write`.
- What the code read must match the transcript's `expect`: every value given there must be the
  same in what the code read (the `PeripheralInfo`, `MouseInfo`, `UnsupportedDevice` or
  `CorsairInfo` the app gets, as JSON). An object may leave fields out; a list is compared whole.
- A HID++ device info answer (`0x0003` function 0) whose unit id is not `xx` fails the test.

## Turning a test report into a replay test

Test reports end with a transcript, from `{` to `}`, after "Transcript (every request and answer,
for a replay test)": those of devices of other brands (the drivers in `peripheral/`), of Logitech
mice with onboard profiles, of Logitech devices behind a receiver that PeriHub reads by their
features, and of Corsair devices. A transcript longer than about 40,000 characters (an issue takes
65,536) is cut with a mark; a cut transcript cannot replay the whole reading.

- Devices of other brands: the first reading after PeriHub connected, and the latest one when it
  was different, with a `{"op":"mark","text":"latest reading"}` in between (one too long keeps the
  first reading only).
- Logitech mice (`hidpp-mouse`): what `MouseTask::open` asked, from the HID++ version on; the
  profiles opened later on the page are not in it.
- Logitech devices behind a receiver (`hidpp-receiver`): the reading of the receiver test report
  (`UnsupportedDevice::read_recorded`), with the receiver's USB ids and the slot.
- Corsair devices (`corsair-bragi`): what `CorsairTask::open` asked; behind a receiver, the
  channel is in `slot` (the receiver's own requests before it are in the transcript too, and a
  replay leaves them out).

1. Copy the transcript from the issue into
   `crates/perihub-core/tests/fixtures/transcripts/<brand>-<model>.json`.
2. Add `"source": "issue #N"` and, in `"notes"`, anything a reader should know (firmware,
   system, what the user said was wrong).
3. Add `"expect"` with what the report shows and the user confirmed: the firmware, the settings,
   the `details`, `"problem": null`. Run `cargo test -p perihub-core replay`: a wrong value is
   printed with its path and what was read.
4. Check that nothing in the file tells the unit apart. PeriHub writes the bytes a driver names in
   `Driver::private_answers` as `xx`, and nothing else from the system goes in, but a device may
   put a serial number where nobody expected one: replace such bytes with `xx` (in an answer it
   reads as 0).

When a driver changes, a replay that now fails says which request is new or which value is
different. Change the transcript only when the device really answers differently, never to make a
test pass.

Older reports (before transcripts) have "Requests and answers", cut to 64 lines, the requests to
their first bytes and some answers to 16 bytes. A transcript can still be rebuilt from them where
the protocol page says what the rest of the bytes are (`wraith-w60.json`): say so in `"notes"`.

## The format

```json
{
  "format": "perihub-transcript",
  "version": 1,
  "device": { "vendor_id": "2e3c", "product_id": "c368", "name": "Wraith W60" },
  "family": "peripheral",
  "access": "read-only",
  "recorded_by": "PeriHub 0.9.3 on windows x86_64",
  "source": "issue #35",
  "notes": ["..."],
  "expect": { "firmware": "V3.22.19" },
  "events": [
    {"op":"descriptor","data":"06 1b ff 09 91 a1 01 ..."},
    {"op":"write","data":"01 0d","len":64},
    {"op":"read","data":"01 0d 00 00 00 32 57 36 ...","len":64}
  ]
}
```

| Field | Meaning |
|---|---|
| `version` | 1. A newer version is refused, not guessed at. |
| `family` | Which code reads it: `peripheral` (a driver of `peripheral::models()`, found by the USB ids), `hidpp-mouse` (`MouseTask::open` for the model of `hidpp::models` with that product id, with its cable, or in receiver slot `slot`), `hidpp-receiver` (a HID++ 2.0 device in receiver slot `slot`, read as for its test report; the USB ids are the receiver's), `corsair-bragi` (a model of `corsair::models`, on receiver channel `slot` if there is one) |
| `slot` | HID++: the receiver slot. Corsair: the receiver channel. Left out for a device on its cable |
| `access` | `read-only` or `read-write`. PeriHub writes `read-write` when the transcript has a command that changes something |
| `events` | In order. `write` (output report) and `feature_set` are requests; each `read` (input report) after a request is its answer; `feature_get` is a feature report read (`report_id` asked, `data` with the number first); `descriptor` is the report descriptor; `mark` is a note |
| `collection` | `main`, the default, or `short`: the second HID++ collection Windows gives a receiver or a HID++ 1.0 mouse |
| `recorded_by` | The PeriHub version and system, or "PeriHub's tests, from a simulated device" |
| `data`, `len` | Bytes in hex, report number first (0 for a device without numbers). `len`: the zeros at the end are left out of `data` |

In a replay a request is answered by its first copy in the transcript not used yet, then by the
last copy again, so a reading repeated every few seconds needs no more copies. Answers queue up
on their own collection, as the device sends them.

## Recording

Every recording sits between the code and the device, writes down what went through and what came
back, and sends nothing of its own (`Recording` in `replay/record.rs`). A call that failed is not
kept. Recording costs no request: a report that holds a transcript asked the device exactly what the
page asked it.

- `Io` records every call a driver of `peripheral/` makes through it.
- `HidppDevice` records every report it writes and reads, on the long collection and on the short
  one (Windows), from the HID++ version request on. A mouse stops recording once it is open, a
  device read by its features once its reading is done.
- `BragiDevice` records every Corsair packet the same way, until `CorsairTask::open` is done.

What tells one unit apart is written as `xx` before the transcript leaves the core: the bytes a
driver of `peripheral/` names in `Driver::private_answers`; for HID++, the unit id in every device
info answer (`0x0003` function 0, bytes 1 to 4) and a serial number (`0x0003` function 2), should
a call ever ask for one. The Corsair code asks for no serial number.

Not recorded yet: the G300 and the Apex 3 TKL, which talk to their devices through code of their
own, outside `Io`, `HidppDevice` and `BragiDevice`.

## Transcripts recorded from a simulated device

Before a real device sends a report, a transcript can still be recorded through PeriHub's real code
from a simulated device built on the facts we have: then a change in what the code asks shows up in
CI. Such a fixture says so in `source` and `notes`, and a test writes it again:

- `logitech-pro-x-2-dex.json`: the PRO X 2 DEX of issue #32 through its receiver, from the values
  of its two test reports (`pro_x_2_dex()` in `mouse.rs`); `notes` lists what the reports did not
  say and was made up.
- `corsair-katar-pro.json`: a KATAR PRO laid out from the protocol page, with example values
  (`SimCorsair` in `replay/tests.rs`), until a Corsair device sends a report.

`cargo test -p perihub-core` fails when the code records something else than the fixture holds.
If the change is meant, write the fixtures again and read the difference before committing it:

```sh
PERIHUB_WRITE_TRANSCRIPTS=1 cargo test -p perihub-core --lib -- the_replay_of_the_pro_x_2_dex the_katar_pro_replay
```

This keeps `source`, `notes` and `expect` as they were written by hand and replaces the rest.
`logitech-g303-receiver.json` and `wraith-w60.json` were rebuilt by hand from older reports that
had no full bytes, and say so in `notes`.
