# SteelSeries Apex Pro, Apex 7, Apex 7 TKL and Apex 5 (the first-generation protocol)

Four SteelSeries keyboards that share one command set on their vendor HID collection: the **Apex Pro** (`1038:1610`,
issue #111), the **Apex 7** (`1038:1612`), the **Apex 7 TKL** (`1038:1618`) and the **Apex 5** (`1038:161c`). **Nobody has
tried them with PeriHub.** What PeriHub reads, and what it changes as an experimental feature, is listed under "What
PeriHub does". The Apex 3 and Apex 3 TKL are other keyboards with another protocol (the 8-zone one,
`crates/perihub-core/src/apex`), and are not described here.

## Sources, in PeriHub's own words

- [OpenRGB](https://gitlab.com/CalcProgrammer1/OpenRGB) (GPL-2.0-or-later, commit 6b681e3c, 19 November 2026), its SteelSeries Apex
  controller (`Controllers/SteelSeriesController`: `SteelSeriesApexController`, `SteelSeriesApexBaseController`,
  `SteelSeriesApexRegions.h`, `SteelSeriesControllerDetect.cpp`). OpenRGB says it is based on the public plugins of SignalRGB.
  GPL, so only facts were taken (ids, interface, report sizes, commands, bytes, answers) and written here; the Rust was
  written from this page alone, with neither project open. No code was copied.
- [apex-tux](https://github.com/not-jan/apex-tux) (Unlicense, commit d0911fe, 10 September 2026): the picture the OLED
  display of these keyboards takes, which it sends to them from Linux. Public domain, so no attribution is needed; it is
  credited anyway. Its code lists the Apex Pro, 7, 7 TKL, 5 and Pro TKL; the comment above the first four says "never tested",
  and its README says that it supports the Apex Pro, 5 and 7.
- The device request of issue #111 (an Apex Pro on Windows, with PeriHub 0.9.6): the USB interfaces and what each
  carries, which agrees with OpenRGB's choice of interface 1.
- A pull request of another project (steelseriesgg-rs #318, an Apex Pro of 2024, `1038:1640`) says the `3a` colour frame below was
  tried on that keyboard: a colour held without being sent again, and a sweep lit every key at its HID usage. That is another
  keyboard, so it only supports the frame, and PeriHub does not add that id.

Looked at and **not used**:

- OpenMouse's keyboard-protocol (AGPL-3.0) says in its own notes that its SteelSeries commands come from the device
  descriptions shipped with SteelSeries GG, "extracted locally". PeriHub does not use facts that come from reading the
  official software, even when someone else did it. Its notes list the legacy Apex Pro (`1610`, `1614`, `1640`) among the families it does not cover.
- apex-control (MIT) controls the wired Apex Pro TKL Gen 3 (`1642`) only and says the other Apex models use different commands.
- apex-analog-mapper (MIT) reads how far each key is pressed on the original Apex Pro TKL (`1614`) with a command PeriHub has no
  page for, and marks the full-size Apex Pro "untested". Not used.
- Nothing public documents the per-key **actuation point** (OmniPoint) of the original Apex Pro, nor its key remapping, macros
  or onboard profiles: they wait for a recording of SteelSeries GG made by a tester (docs/captures.md).

## The four models

| USB id | Name | Interface | Collection |
|---|---|---|---|
| `1038:1610` | Apex Pro | 1 | usage page `ffc0`, usage `0001` |
| `1038:1612` | Apex 7 | 1 | the same |
| `1038:1618` | Apex 7 TKL | 1 | the same |
| `1038:161c` | Apex 5 | 1 | the same |

OpenRGB detects all four by USB interface 1 alone and speaks the same commands to them; it says the same colour frame is sent
whatever the model, and the keyboard picks the keys it has and ignores the others.

The Apex Pro of issue #111 shows its collections like this: interface 1 is the vendor collection (usage page `ffc0`, no report
number, input 64 bytes, output 64 bytes, feature 642 bytes), interface 4 a second vendor collection (usage page `ffc1`, input 64
bytes), and interfaces 0, 2 and 3 carry a keyboard, a mouse and media keys. **PeriHub opens only interface 1.** The others carry
what is typed.

Left out on purpose, although OpenRGB lists them:

- **Apex Pro TKL** (`1614`) and the **Apex 9 TKL and Mini**. OpenRGB gives the Apex Pro TKL this protocol until firmware 1.19.7 and a newer one
  (an initialisation command `4b`, colours in `40`, back to the profile with `41`) from then on, by a test (major version above 1,
  or 1.19.7 and later) that puts every unit with the firmware 4.x seen in other projects in the newer kind, while another project
  says the `3a` frame works on a Gen 3 keyboard with firmware 4.16.8. PeriHub cannot tell which one a Pro TKL is, so it leaves it out.
  The Apex 9 are of the newer kinds in OpenRGB too, and so are the **2023 and Gen 3 models**, whose wireless versions use yet another
  command for their colours.
- **Apex Pro 3** (`1640`): OpenRGB gives it this protocol, but apex-tux gives it another picture format for the display.
- **Apex M750, Apex (OG), Apex 350**: other protocols.

## Transport

- Report number 0 everywhere. An **output report** is 64 bytes: the command, then its arguments, then zeros. The keyboard answers
  with an **input report** of 64 bytes.
- A **feature report** is 642 bytes (643 with the report number that OpenRGB sends, 0): the command, its data, then zeros. OpenRGB
  says that a feature report of any other size makes the keyboard stall the control transfer.
- OpenRGB reads every answer from the input endpoint, after an output report.
- Other programs that have the collection open (SteelSeries GG) receive the same answers: a request has to accept only the answer
  that carries its own command.

## Reads (output report, answer on the input endpoint)

| Request | Answer |
|---|---|
| `90 00` | the keyboard's firmware version as ASCII text with two dots (`1.2.3`). OpenRGB drops the first byte when it is `90` (an echo of the command) and removes the zeros, so the echo is optional |
| `90 01` | the same for the LED controller (the first generation has two versions, OpenRGB prints "KBD: x / LED: y") |
| `a3` | `a3 00 LL`: the brightness, `LL` from 0 to 10. A keyboard that does not have it answers with `ff` in the second byte, and OpenRGB then leaves the brightness alone |
| `ff` | the serial number in the first 19 bytes, whose first five digits are the product number (the region): **PeriHub does not read it**, because it tells units apart and OpenRGB's table of product numbers has the layout of only a few of them |

OpenRGB waits 2 ms for the firmware and 100 ms for the brightness. PeriHub waits longer.

## Changes

| Command | What it does | Kind |
|---|---|---|
| `23 LL` (output) | sets the brightness, 0 to 10, "the same setting the brightness keys on the keyboard step through". OpenRGB says it applies to the keyboard's own lighting and that colours sent with `3a` do not depend on it. Whether it is stored in the keyboard is not documented | change |
| `3a NN (KK RR GG BB) x NN` (feature) | **direct lighting**: `NN` keys, each a HID usage `KK` with its colour. The colours last until the keyboard is told to go back to its own lighting; nothing documents what happens to a key that is not in the frame, so PeriHub always sends all of them | change, not stored |
| `3b` (output) | **back to the keyboard's own lighting** (its profile). OpenRGB sends it when it leaves direct mode and when it closes | change |
| `61` + 640 bytes + `00` (feature) | **the OLED display**: a picture of 128 x 40 pixels, one bit each, row by row from the top, 16 bytes per row, the leftmost pixel in the highest bit | change, not stored |

apex-tux sends the display's feature report with `61` as its report number; the bytes on the wire are the same as with report
number 0 and `61` first, which is how PeriHub sends it (every report with report number 0, the one the descriptor declares).

The keys of the frame, as OpenRGB sends them, in order (HID usages): `04` to `1d` (A to Z), `1e` to `27` (1 to 0), `28` Enter, `29`
Esc, `2a` Backspace, `2b` Tab, `2c` Space, `2d` to `30` (minus, equals, the two brackets), `32` (the key next to Enter on ISO),
`33` to `38` (semicolon, quote, grave, comma, period, slash), `39` Caps Lock, `3a` to `45` (F1 to F12), `46` to `4e` (Print Screen,
Scroll Lock, Pause, Insert, Home, Page Up, Delete, End, Page Down), `4f` right, `50` left, `51` down, `52` up, `64` (the key next
to the left Shift on ISO), `e0` to `e7` (the eight modifiers), `f0` (the SteelSeries key), `31` (the backslash of ANSI keyboards),
`87` to `8b` (the Japanese keys), `53` to `63` (the number pad) and `fb` (media play/pause): **112 keys**, so the frame uses 451 bytes
of the 642.

## What PeriHub does

- **Reads** the two firmware versions and the brightness, with the commands above and nothing else. An answer that is not a
  version, or a brightness above 10, leaves the keyboard read-only with the reason. A keyboard that does not answer the brightness
  is shown without it. The firmware is read once, the brightness again every few seconds.
- **Brightness**, in tens of percent, as an experimental change: a backup of the settings first, the change, a read-back, and the
  15-second undo. Nothing is saved to the keyboard (no command that does is listed).
- **Colours**, one colour for the whole keyboard or one for each group of keys (letters, number row, F1 to F12, navigation keys and
  arrows, number pad, modifiers and the SteelSeries key, all the other keys), sent as one frame with all 112 keys. The keyboard
  cannot be asked what it shows, so PeriHub cannot read it back or undo it, and says so: unplugging the keyboard brings back its own
  lighting. A group never sent is dark.
- **Back to the keyboard's own lighting** with `3b`.
- **The display**: clear it, light every pixel, or a test pattern, as one 128 x 40 picture.

Never sent: any command of the other generations (`40`, `41`, `4b`, `f5`, the wireless `61` colour frame, which is the same
byte as the display's), and `89` (select a profile), which OpenRGB defines and never sends.
