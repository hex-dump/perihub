# Pulsar mice on their cable (Sonix protocol)

How PeriHub talks to the Pulsar Xlite, X2, X2H, X2A, Xlite v4, X3 and X3 LHD on their USB cable. **No mouse has
been tried with PeriHub yet**: everything here comes from the sources below, and every change PeriHub makes is
experimental until someone confirms it.

Sources:

- [pulsar-mouse-linux](https://github.com/packerlschupfer/pulsar-mouse-linux) (MIT, © 2026 Packerlschupfer), commit
  `037cefe`: `src/pulsar_mouse/drivers/x2a.py`, the drivers of the other models and their tests, built from USB
  captures of Pulsar's own app. It is a recent project: PeriHub marks everything from it as to be verified.
- [Pulsar-X3-Web-Config](https://github.com/santoshpanna/Pulsar-X3-Web-Config) (MIT, © 2026 Santosh Panna), commit
  `17a8186`, `PROTOCOL.md`: only for what each USB interface carries.

## Which mice

| USB id | Mouse | Profiles | DPI | Tried by the source |
| --- | --- | --- | --- | --- |
| `3710:1401` | Xlite Wired | 5 | 100–26000, steps of 50 | no: the X2A's protocol |
| `3710:1402` | X2 Wired | 5 | 100–26000, steps of 100 | no: the X2A's protocol |
| `3710:1403` | X2H Wired Medium | 5 | 100–26000, steps of 100 | "fully supported" in its README |
| `3710:1404` | X2A Medium Wired | 5 | 100–26000, steps of 100 | yes, the author's own mouse |
| `3710:3401` | Xlite v4 (cable) | 6 | 100–32000, steps of 10 | partly: lift-off and profiles |
| `3710:3409` | X3 (cable) | 6 | 100–32000, steps of 10 | yes |
| `3710:3410` | X3 LHD (cable) | 6 | 100–32000, steps of 10 | yes |

The CrazyLight mice speak another protocol: on their cable they are in [nordic-mice.md](nordic-mice.md). The wireless
dongles, the Feinmann mice, the first X2H (on `3554`) and the X2 Wireless on Areson ids are left out.

## Interfaces

What the second source found on the 1K dongle of these mice, the same family: interface 0 is the mouse; interface 1
carries a keyboard, consumer and system control collections (what a button bound to a key or a macro types) and
two collections of unrequested reports (DPI and profile changes); interfaces 2 and 3 have the same descriptor,
usage page `FFFF`, usage 1, with 64-byte input, output and feature reports without a report id, and only interface 3
answers. Its web app looks for that `FFFF` collection on the cabled mice too.

So the Linux udev rule opens **interface 3 only**: never interface 1, which carries keys. PeriHub finds the mouse by
that interface number, not by the usage page, since interface 2 would match it as well.

## Transport

USB interface 3, **feature report 0**, 64 bytes. The Linux udev rule opens interface 3 only.

A request: byte 0 `00`, byte 1 category, byte 2 register (with bit 7 set to read it), byte 3 sub-register, bytes 4–5
`00`, byte 6 the profile (`00` for the mouse's global settings, else 1 to the number of profiles), the data from byte 7,
and in bytes 62–63 the sum of bytes 0–61 as a 16-bit little-endian number.

After sending it, the host reads feature report 0 until byte 0 says `01` (answered); `02` means the mouse is still on
it, `00` that it did not take it. The source polls every 20 ms; a write is answered about 28 ms after it was sent.
PeriHub waits for `01` after every request, reads and writes alike, as the source advises, reading every 30 ms, five
times.

An answer repeats the request's category and register, bit 7 included, and the profile byte when the request named
a profile (profile 0 answers carry data there). The sub-register can differ: the DPI stages, asked with `15`, are
answered with `21`. PeriHub takes the answer to a read only when those bytes match its request, so an answer left
in the mouse's buffer for another request, by PeriHub or by another program, is never read as this one. Answers to
writes were seen to repeat the request only through the dongle (the source checks nothing but byte 0 on the cable):
PeriHub waits for a matching answer to a write too, but takes one still at `01` after the last poll, since the
reading after every change checks what the mouse stored.

Right after its cable shows up, an X3 left requests at `00` for up to about a second (pulsar-mouse-linux issue 21),
and they were answered only when sent again. Until the first answer of a connection, PeriHub sends a read the mouse
did not take (`00` on the last poll) again, 100 ms later, six times at most. A request the mouse took (`02`) is not
sent again, and **a write is never sent twice**.

A mouse that does not answer one of these reads the first time PeriHub reads it gets a read-only page with the
test report, so a model nobody tried still shows what it answered.

## Registers

| Category, register, sub | Profile | What | Answer |
| --- | --- | --- | --- |
| `01 07 04` | 0 | Firmware | bytes 6–9, shown highest first |
| `02 06 01` | 0 | Profile in use (read) | byte 6, from 1 (see below) |
| `01 09 02` | 0 | Polling rate | byte 7: `1` 125, `2` 250, `4` 500, `8` 1000 Hz |
| `04 03 03` | 0 | Debounce time | byte 7, in ms, 0–20 |
| `07 04 02` | 0 | Angle snapping | byte 7, 0 or 1 |
| `07 03 02` | 0 | Ripple control | byte 7, 0 or 1 |
| `07 05 02` | 0 | Motion sync | byte 7, 0 or 1 |
| `05 04 15` | profile | DPI stages (read) | byte 7 the stage in use (from 1), byte 8 how many, then 5 bytes a stage: its number, X and Y DPI (16-bit little-endian) |
| `05 04 21` | profile | DPI stages (write) | the same layout from byte 7 |

A write sends the register without bit 7, with the value in byte 7 (the DPI stages: the stage in use, how many, and
each stage's number, X, Y).

The profile in use: pulsar-mouse-linux reads it on the X3 and the X2A, while the second source says the X3 behind
its dongle does not answer it. When it is not answered, PeriHub leaves the DPI stages out (it never guesses a
profile) and still shows and changes the global settings.

The polling rate codes above are the cabled ones. The 8K dongle adds `10` 2000, `20` 4000 and `40` 8000 Hz; a cabled
mouse that reads one of them, or any other unknown value, is shown read-only.

Never sent: `02 06 01` as a write (switches profile), `02 05 01` (resets the profile to its defaults **and every
global setting**, measured on an X2A), `04 01 06` (buttons), the lighting, lift-off and power registers.

## How PeriHub changes them

Every write is stored by the mouse, so PeriHub treats it like the Razer mice: only **Save to memory**, after the No
warranty box, with backup, read-back and the 15-second undo.

- Global settings: polling rate, debounce time, angle snapping, ripple control and motion sync.
- The profile in use: the DPI of each stage. How many stages there are and which is in use are written back as read,
  from the reading made right before the change, which is refused if the profile in use changed since the page was
  read. When a stage's X and Y differ, the DPI is shown and not changed.
- Every value is checked before the first write, so a refused value leaves the mouse untouched.

## Not done yet

Lift-off distance (its encoding differs between the X2A and the X3), lighting, buttons, power settings, the other
profiles, and the wireless dongles.
