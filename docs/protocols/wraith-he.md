# Wraith W60 (AULA WIN60 HE platform)

The Wraith W60 (`2e3c:c368`) is a magnetic (Hall effect) keyboard configured from `wraith.software`. `2e3c` is the
vendor id of Artery, the maker of its microcontroller, not of Wraith. It has the configuration collection of the
AULA WIN60 HE (`2e3c:c365`): USB interface 2, usage page `ff1b`, usage `91`. For now PeriHub only reads.

Source, in PeriHub's own words: [AulaControl](https://github.com/sgtflixy/AulaControl) (Apache-2.0, commit
9a526ee), which documents the WIN60 HE and confirms the lighting and per-key actuation on a real keyboard.

**Confirmed on a W60** (firmware V3.22.19, Windows, PeriHub 0.8.4, report in issue #35): the report descriptor
(`06 1b ff 09 91 a1 01 85 01` and an input and an output report of 63 bytes, number `01`), the device-info reads
and the two lighting reads below answer as documented for the WIN60 HE. Writing has not been tried.

## Transport

- Output report number `01` with 63 bytes; the answer is input report `01` with 63 bytes.
- A request: command, second byte, `00`, an index, then its values. The answer repeats the command and has `00 00`
  after it, then the index, a length and the values (checked on the W60: the lighting request's `01` is not repeated).

## Reads

| Request | Answer |
|---|---|
| `0d 00 00 00` | two answers: the model and the build date. Byte 4 of the answer is the index: `00` is the model, `01` the build date. The text follows a length byte (always `32` on the W60, even when the text is shorter: the rest is zeros) |
| `07 01` | the key lighting, 12 values (below) |
| `08 01` | the same for the side lighting |

On the W60 (firmware V3.22.19): the model is `W669,34,KB,ML,AP61HEARGB,V3.22.19` and the date `Aug  6 2026,13:31:21`.
`W669` is the platform's name, the same as on the WIN60 HE (`W669,34,KB,SI,SI2825KZHEARGB,V3.17.07`). The version is
the last field of the model. Which of the two answers comes first depends on the keyboard's last session: PeriHub
tells them apart by the index, not by the text.

A lighting answer is `<command> 00 00 00 0c` and then 12 values (the answer's second byte is `00`, not the `01` of the
request; the length is `0c`, where AulaControl's write frame has `0e`):

| Value | Meaning |
|---|---|
| 0 | mode (a number, see below) |
| 1 | speed, 0 to 4 |
| 2 | brightness, 0 to 4 |
| 3–5 | colour, R G B |
| 6–8 | second colour, R G B |
| 9 | direction (a number; what it means per effect is not known) |
| 10 | full colour: 0 fixed colour, 1 the effect cycles through all colours (AulaControl, on a WIN60 HE) |
| 11 | not documented: PeriHub shows it as a number |

As read on the W60: the key lighting `10 04 04 ff ff ff 00 ff ff 00 00 00` is mode 16, speed 4, brightness 4, colour
white, second colour cyan, direction 0, full colour off, last value 0. The side lighting
`05 04 03 ff 00 00 00 ff 00 00 01 01` is mode 5, speed 4, brightness 3, colour red, second colour green, direction 0,
full colour on, last value 1. (On a WIN60 HE, AulaControl's capture of the side lighting has the last value 0 with
full colour on.)

Modes: AulaControl lists the WIN60 HE's effect numbers (Static 0, Breath 1, Wave 2, Neon 3, Radar 4, Reactive 6,
Aurora 7, Ripple 8, Twinkle 9, Custom 10, Cross 11, SpeedRespond 12, AutoRipple 14, Striation 15, Fireworks 16) and
says 5 and 13 have no button. The W60 reports 5 for its side lighting, a number AulaControl says no button sends, so the
W60's list may differ. PeriHub shows the number until a tester says what the W60's page calls modes 16 and 5.

## Key travel

While the vendor's travel test runs, the collection sends one report per key movement with how far the key is
pressed (`21 00 00 00 05 01 03 02 <travel>`, in AulaControl's captures). PeriHub never asks for it and skips
such reports. But any program that can open the collection could, so **on Linux PeriHub does not open this
keyboard yet**: it will take a rule the user turns on, as for QMK keyboards. On Windows, programs can open the
collection without any rule.

## Left out for now

Changing the lighting (`07`/`08` with values): AulaControl's write frame is `<command> 00 00 00 0e` and the 12 values
and was tried on a WIN60 HE for `07` (never for `08`), but its length byte differs from what the W60 answers (`0c`)
and the last value is 1 on the W60's side lighting, where AulaControl always sends 0. PeriHub waits for captures of
the W60's own page before it writes.

Per-key colours (`09`), actuation and rapid trigger (`21`), SOCD (`24`), key remapping and polling rate.
AulaControl decodes some of them only partly: they need captures of `wraith.software` from a tester's own keyboard
(the guide for them is on issue #35).
