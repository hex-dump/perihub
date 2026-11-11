# Glorious mice (SinoWealth)

How PeriHub talks to the Glorious Model O/O-, Model D and Model O Eternal, wired mice built on a SinoWealth
controller. **No mouse has been tried with PeriHub yet**: everything here comes from the sources below, and every
change PeriHub makes is experimental until someone confirms it.

Sources:

- [libratbag](https://github.com/libratbag/libratbag) (MIT), commit `8235b5b`: `src/driver-sinowealth.c` and
  `src/driver-sinowealth.h`, © 2020 Marian Beermann, and the `data/devices/glorious-*.device` files.
- [OpenRGB](https://gitlab.com/CalcProgrammer1/OpenRGB) (GPL-2.0-or-later), commit `b9d903b`,
  `SinowealthControllerDetect.cpp` and `SinowealthController.cpp`: only facts were taken, no code. These are the usage
  page of the configuration collection of the Model O and Model D, the warning below about shared ids, and what
  OpenRGB found on Windows and after a write (see Transport).
- Configuration dumps posted in libratbag issues #1091, #1361, #1444 and #1587, which confirm the bit layout of
  bytes 10 and 11 (see below).

## Which mice

Only the three ids that belong to Glorious mice:

| USB id | Mouse | Firmware libratbag knows |
| --- | --- | --- |
| `258a:0036` | Glorious Model O/O- | `V103` |
| `258a:0033` | Glorious Model D | `V102` |
| `3794:a000` | Glorious Model O Eternal | `1009` |

The other ids libratbag lists for this driver (`258a:0027`, `0029`, `0051`, `1007`) are generic SinoWealth ids,
shared by mice of several brands, some of which speak a different protocol (OpenRGB has a separate driver for the
ZET Fury Pro and the Genesis Xenon 200 on `258a:1007`). OpenRGB turned off its SinoWealth keyboard support because
the same ids were reused by Redragon keyboards, which its commands bricked. PeriHub leaves those ids out. The
wireless Model O and Model D (`258a:2011`, `2012`, `2022`, `2023`) speak another protocol and are left out too.

A mouse whose firmware is not the one in the table is read for its firmware only: libratbag refuses it as well.

## Transport

- Everything goes through **feature reports with a report number**: 5 for short commands (6 bytes with the number),
  4 or 6 for the configuration block (520 bytes with the number).
- The configuration collection has the vendor usage page `0xFF00` (OpenRGB). libratbag says only the mouse's
  keyboard interface has these reports: on the SinoWealth mouse whose descriptor is in issue #1091 (a Marvo
  Scorpion G961), USB interface 1 is a keyboard interface, used for the keys the mouse's buttons can send. No source
  gives the interface number of the Glorious mice, so the Linux udev rule opens the whole mouse, as it does for the
  ASUS mice without one. It carries what the mouse does and the keys its own buttons send, never what is typed on a
  keyboard.
- **A read** sends the 6-byte request as feature report 5, then reads a feature report back: report 5 for short
  answers, 4 or 6 for the configuration. The answer's byte 1 repeats the command.
- **On Windows**, where each HID collection is a device of its own, OpenRGB expects three `0xFF00` collections on
  the Model O and Model D, and finds the command report (5) and the configuration report (4) on collections that
  may differ. It reads report 4 with a 520-byte buffer. PeriHub opens one collection for now, so on Windows the
  page may show only what that collection answers.
- **Right after a write**, OpenRGB found that reading the block back returns zeros from byte `0x2C` onwards; it
  waits 50 ms before the next reading.

## Commands

| Command (byte 1) | What | PeriHub |
| --- | --- | --- |
| `01` | Firmware version: bytes 2–5 of the answer, ASCII (`V103`) | read |
| `11` | Configuration of the first profile | read, and stored with report 4 or 6 |
| `02` | Active profile when byte 2 is 0, **switches profile** otherwise | not sent |
| `12`, `22`, `32` | Buttons of profile 1, 2, 3 | not sent |
| `21`, `31` | Configuration of profiles 2 and 3 | not sent: the Glorious mice have one |
| `30` | Macros | not sent |
| `1a`, `1b` | Debounce time; angle snapping and lift-off distance on long mice | not sent |
| `75` | **Puts the mouse in firmware update mode** (libratbag: replug it to leave) | never sent |

PeriHub's list holds exactly `05 01 00 00 00 00`, `05 11 00 00 00 00` (reads) and a configuration block on report 4
or 6 with command `11` (a change that stays). Everything else is refused before it leaves.

## The configuration block

Read: send `05 11 00 00 00 00`, then read feature report 4. If that fails or its length is wrong, send the request
again and read report 6: that mouse uses the long report. libratbag picks the report from the descriptor; PeriHub
uses the one that answered, and writes back on the same one.

The mouse answers with its configuration, **123 to 167 bytes** depending on the model, report number included
(libratbag refuses any other length). Offsets below count the report number as byte 0.

| Byte | What |
| --- | --- |
| 0 | Report number (4 or 6) |
| 1 | `11` |
| 2 | Unknown, kept |
| 3 | `00` when read. When writing: the length of the block minus 8 (OpenRGB always writes `7b` on the Model O and Model D, which by this rule is a 131-byte block) |
| 4–8 | Unknown, kept |
| 9 | Sensor: `06` PMW3360, `08` PMW3212, `0e` PMW3327, `0f` PMW3389 |
| 10 | Low 4 bits: polling rate, `1` 125 Hz, `2` 250 Hz, `3` 500 Hz, `4` 1000 Hz. Bit 7: X and Y DPI separate |
| 11 | Low 4 bits: how many DPI slots are on. High 4 bits: the slot in use, from 1, counting only those on |
| 12 | One bit per DPI slot, from bit 0: set means off |
| 13–28 | DPI of the 8 slots, one byte each in bytes 13–20; with separate X and Y, two bytes each (X, Y) |
| 29–52 | Colour of each DPI slot, 3 bytes each |
| 53 onwards | Lighting effects and their colours (RGB or RBG depending on the model; `ff` in byte 53 means no LEDs), then on longer blocks the lift-off distance and unknown data |

The **bit order** of bytes 10 and 11 follows C bit-fields on the compilers libratbag is built with: the first field
is in the low 4 bits. The dumps in the issues above agree. At 1000 Hz byte 10 reads `04`. In issue #1444, byte 11
`25` (5 slots on, slot 2 in use) goes with byte 12 `e0`, where bits 0–4 are clear: 5 slots on. Reversed, the count
would be 2.

DPI is stored in steps of 100: `(DPI / 100) − 1` on the PMW3360 and PMW3327, `DPI / 100` on the PMW3389 and PMW3212.
The highest DPI is 12000 (PMW3360), 10200 (PMW3327), 16000 (PMW3389) and 7200 (PMW3212); the lowest is 100.

## How PeriHub changes it

There is no command that changes a setting without storing it: **every change rewrites the block in the mouse's
memory**. PeriHub treats it like the Razer mice: only **Save to memory**, after the No warranty box, with the backup,
read-back and 15-second undo of every other brand.

- **The reference block** is the one read when the mouse was connected, then the last one PeriHub wrote and read
  back. A reading that differs from it anywhere but in the DPI of the slots that are on, the polling rate (low 4 bits
  of byte 10) and the slot in use (high 4 bits of byte 11, which the mouse's own DPI button changes) is never used:
  another program changed the mouse, or the reading is incomplete. The page then turns read-only.
- PeriHub reads the block again just before writing, checks it against the reference block and the page, and changes
  only the DPI of the slots that are on, and the polling rate. Which slots are on, the slot in use, the lighting, the
  buttons and the unknown bytes are written back exactly as read.
- The block it sends is 520 bytes, like libratbag's: the report number, `11`, byte 2 as read, the length minus 8, the
  rest of the block as read with the changes, then zeros.
- It then reads the block back up to five times, 100 ms apart, until every byte (byte 3 apart) is the one sent. If
  none matches, PeriHub puts the settings from before back the same way. Since an incomplete reading never matches
  the reference block, its zeros can never be written back.
- DPI is not changed when X and Y are separate, nor when the sensor byte is not one of the four above.
- Nothing is changed when the block's length is not 123–167 bytes, or when byte 11's count does not match the slots
  on in byte 12. **A 520-byte answer** is read and shown, but never written: it does not say how long the block
  really is, and byte 3 must say it. That is what OpenRGB's reading on Windows suggests: there the mice are read-only
  for now.

## Not done yet

Lighting (the colour order differs between models, and `ff` must never be overwritten), buttons, macros, debounce
time, angle snapping and lift-off distance.
