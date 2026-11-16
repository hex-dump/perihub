# Mice on the YJX platform

Many wireless mice (MAMBASNAKE M4, Rajin Blaze Claws, K-Snake X11, EWEADN GS02 Pro, Noir M2-NEX…) use a
receiver that calls itself "YJX-CHIP USB Receiver", USB id `a8a5:2255`, and `a8a4:2255` on the cable. The same id
stands for different mice, configured from web pages on `yjx2012.com`. **None has been tried with PeriHub yet**, and
for now PeriHub only reads.

Sources, in PeriHub's own words:
- [claws-mouse-linux](https://github.com/naevui/claws-mouse-linux) (MIT, commit ed9e0e9), from a capture of the
  Rajin Claws' official app: the transport, the version and settings commands and the settings layout.
- [EWEADN-GS02-Pro-Control](https://github.com/mimuruu/EWEADN-GS02-Pro-Control) (MIT, commit f31692d): the same
  commands sent with report number `f0` on that mouse, and the battery command.
- [OpenMouse mouse-protocol](https://github.com/OpenMouse-Project/mouse-protocol) (AGPL-3.0, commit 5a4f643), its
  K-Snake driver, facts only: the battery request and where the settings hold the lift-off distance.

## Transport

- USB interface 2, a vendor collection (usage page `ff01`, usage `10`) with 64-byte input and output reports. The
  receiver's interfaces 0 and 1 are the mouse and a keyboard: PeriHub never opens them.
- Two sources send the reports **without a report number**: a request starts with `55`, then the command; the
  answer starts with `aa`, then the same command. EWEADN's mouse uses **report number `f0`** instead.
- So PeriHub first reads the collection's report descriptor (a request to the system, nothing reaches the mouse's
  configuration). Only when it declares no report number does PeriHub send anything. Otherwise it shows the mouse
  read-only, with the descriptor in its test report.
- The mouse also sends reports by itself (`aa fa ae …` when its DPI button is pressed): PeriHub takes only an
  answer whose second byte is the command it sent.
- Through the receiver a mouse that is off does not answer: PeriHub asks again later.

## Reads

| Request | Answer |
|---|---|
| `55 03`, zeros | the version: text from byte 8 up to a zero byte |
| `55 0e`, zeros | the settings, below |
| `55 30 a5 0b 2e 01 01 00 00 00`, zeros | battery in percent at byte 8, `01` at byte 9 while charging |

The settings answer: byte 9 the lighting mode, byte 10 the polling rate (`1`–`4`: 125, 250, 500, 1000 Hz), byte
11 the number of DPI levels (1–6), byte 12 the level in use (from 1), bytes 13–24 the six levels as 16-bit
little-endian DPI, byte 49 the lift-off distance. Out-of-range values make the page show what was read and change
nothing.

The MAMBASNAKE M4's vendor page, in the tester's screenshots, shows six levels from 200 to 24000 DPI, 125 to
1000 Hz, the scroll direction, a lighting mode list of its own, five buttons, macros and a factory reset.

## Left out for now

Writing the settings (`55 0f`, the whole block written back), buttons (`08`/`09`), lighting and macros. A
malformed settings write can stop the sensor until the mouse is turned off (claws-mouse-linux), and the bytes after
the command differ between models, so PeriHub will write only what a tester's own mouse shows in a capture of its
vendor page, and only the fields it knows, the rest written back exactly as read.
