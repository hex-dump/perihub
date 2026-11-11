# Logitech G600

How PeriHub talks to the Logitech G600 (`046d:c24a`), a wired MMO mouse with three onboard profiles. It does not
speak HID++: its settings travel in vendor feature reports. **No G600 has been tried with PeriHub yet**: everything
here comes from the sources below, and every change PeriHub makes is experimental until someone confirms it.

Sources:

- [libratbag](https://github.com/libratbag/libratbag) (MIT), commit `8235b5b`: `src/driver-logitech-g600.c`,
  © 2018 Thomas Hindoe Paaboel Andersen, and `data/devices/logitech-g600.device`.
- [OpenRGB](https://gitlab.com/CalcProgrammer1/OpenRGB) (GPL-2.0-or-later), commit `1efdbb9a`,
  `LogitechControllerDetect.cpp`: only a fact, no code: OpenRGB opens the G600 on **USB interface 1, usage page
  `0xFF80`**.
- `lsusb -v` dumps in [linuxhw/LsUSB](https://github.com/linuxhw/LsUSB): interface 0 is a boot mouse, interface 1 a
  keyboard interface (the twelve side buttons can send keys).

## Interface

The vendor collection, usage page `0xFF80`, is on interface 1, together with the keyboard collection of the mouse's
own side buttons. The Linux udev rule opens interface 1 only: it carries what the mouse's own buttons send, never what
is typed on a keyboard. PeriHub reads and writes feature reports only, and never reads the input reports.

## Feature reports

All have a report number. Reading a feature report sends only its number and changes nothing.

| Report | Length | What |
| --- | --- | --- |
| `f0` | 4 | Read: byte 1 holds the DPI slot in use in bits 1–2 (0–3) and the profile in use in bits 4–7 (0–2). Write `f0 (40 + 2·slot) 00 00`: switches the DPI slot in use. Write `f0 (80 + 16·profile) 00 00`: **switches profile** |
| `f1` | 8 | Lighting sent directly (OpenRGB): whether it stays is not known |
| `f3`, `f4`, `f5` | 154 | Profile 1, 2, 3. Writing one stores it in the mouse's memory |

PeriHub writes only `f3`–`f5` (a change that stays) and, after writing the profile in use, the DPI slot that was in
use (`f0 40…46`). It never switches profile and never sends `f1`.

## A profile (154 bytes, report number included)

| Byte | What |
| --- | --- |
| 0 | Report number (`f3`, `f4`, `f5`) |
| 1–3 | Colour: red, green, blue |
| 4 | Lighting effect: `0` steady, `1` breathing, `2` colour cycle |
| 5 | Effect duration in seconds (libratbag caps it at 15) |
| 6–10 | Unknown, kept |
| 11 | Polling rate: `1000 / (value + 1)` Hz, `0`–`7`: 1000, 500, 333, 250, 200, 166, 142, 125 |
| 12 | DPI while G-Shift is held, in steps of 50 (`0`: off) |
| 13 | Default DPI slot, 1–4 |
| 14–17 | DPI of the four slots, in steps of 50 (`04` = 200 … `a4` = 8200); `0` turns a slot off |
| 18–30 | Unknown, kept |
| 31–90 | 20 buttons, 3 bytes each: code, modifiers, key |
| 91–93 | Colour while G-Shift is held |
| 94–153 | The 20 buttons while G-Shift is held |

Button codes: `00` a key (modifiers in the second byte: bit 0 left Ctrl, 1 left Shift, 2 left Alt, 3 left Win, 4–7 the
right ones; the key as a keyboard usage in the third) or nothing when both are 0, `01`–`05` mouse buttons 1–5, `11`
DPI up, `12` DPI down, `13` DPI cycle, `14` profile cycle, `15` DPI shift, `17` G-Shift.

## How PeriHub changes it

There is no change that lasts only until the mouse is unplugged: **writing a profile stores it**. PeriHub treats it
like the Razer and Glorious mice: only **Save to memory**, after the No warranty box, with backup, read-back and the
15-second undo, and only on the profile in use.

- **The reference profile** is the one read when the mouse was connected, then the last one PeriHub wrote and read
  back. PeriHub reads the profile again just before writing and writes nothing if it differs from the reference in any
  byte: the mouse does not change its profiles by itself, so another program did.
- It changes only the DPI of the slots that are on, the polling rate, the lighting effect, colour and duration. Slots
  that are off stay off, and every other byte (buttons, G-Shift, unknown bytes) is written back as read. libratbag
  copies the main colour into the G-Shift colour on every write; PeriHub does not.
- It reads the profile back until it matches, three times at most, then puts back the DPI slot that was in use.
- DPI: 200–8200 in steps of 50.

## Not done yet

Buttons (shown as read), the G-Shift DPI and colour, the other two profiles, switching profile, and the lighting
report `f1`.
