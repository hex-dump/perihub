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
- A command that changes something (a `Change` or `Save` of the driver's list, a HID++ call
  outside the read calls, a Corsair packet other than "get property") **fails the test** when the
  transcript is `"access": "read-only"`, even though the replay opens the device the way the app
  does, allowed to change things.
- What the code read must match the transcript's `expect`: every value given there must be the
  same in what the code read (the `PeripheralInfo`, `UnsupportedDevice` or `CorsairInfo` the app
  gets, as JSON). An object may leave fields out; a list is compared whole.
- A HID++ device info answer (`0x0003` function 0) whose unit id is not `xx` fails the test.

## Turning a test report into a replay test

Test reports of devices of other brands (the drivers in `peripheral/`) end with a transcript,
from `{` to `}`, after "Transcript (every request and answer, for a replay test)". It holds the
first reading after PeriHub connected, and the latest one when it was different, with a
`{"op":"mark","text":"latest reading"}` in between. One longer than about 40,000 characters (an
issue takes 65,536) keeps the first reading only, and is cut with a mark if it is still too long.

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
| `family` | Which code reads it: `peripheral` (a driver of `peripheral::models()`, found by the USB ids), `hidpp-receiver` (a HID++ 2.0 device in receiver slot `slot`, read as for its test report), `corsair-bragi` (a model of `corsair::models`) |
| `access` | `read-only` or `read-write`. PeriHub writes `read-write` when the transcript has a command that changes something |
| `events` | In order. `write` (output report) and `feature_set` are requests; each `read` (input report) after a request is its answer; `feature_get` is a feature report read (`report_id` asked, `data` with the number first); `descriptor` is the report descriptor; `mark` is a note |
| `collection` | `main`, the default, or `short`: the second HID++ collection Windows gives a receiver or a HID++ 1.0 mouse |
| `data`, `len` | Bytes in hex, report number first (0 for a device without numbers). `len`: the zeros at the end are left out of `data` |

In a replay a request is answered by its first copy in the transcript not used yet, then by the
last copy again, so a reading repeated every few seconds needs no more copies. Answers queue up
on their own collection, as the device sends them.

## Recording

`Io` records every call a driver makes through it (`Recording` in `replay/record.rs`): it sits
between the driver and the device, writes down what went through and what came back, and sends
nothing of its own. A call that failed is not kept. Recording costs no request: a report that
holds a transcript asked the device exactly what the page asked it.

Not recorded yet: Logitech HID++ mice and receivers, Corsair devices, the G300 and the Apex 3 TKL,
which do not go through `Io`. Their replays are written by hand from reports
(`logitech-g303-receiver.json`) or from the protocol page (`corsair-katar-pro.json`, until a
Corsair device sends a report), and say so in `notes`.
