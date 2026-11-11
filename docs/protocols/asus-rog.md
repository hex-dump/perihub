# ASUS ROG and TUF mice

How PeriHub talks to ASUS ROG and TUF gaming mice. **No mouse has been tried with PeriHub yet**: everything here
comes from the sources below, and every change PeriHub makes is experimental until someone confirms it.

Sources:

- [libratbag](https://github.com/libratbag/libratbag) (MIT): `src/asus.c`, `src/asus.h` and `src/driver-asus.c`,
  © 2021 Kyoken, and the `data/devices/asus-*.device` files. The device table in
  `crates/perihub-core/src/peripheral/asus.rs` is taken from those files (see `THIRD-PARTY-NOTICES.md`).
- [OpenRGB](https://gitlab.com/CalcProgrammer1/OpenRGB) (GPL-2.0-or-later), `AsusAuraMouseControllerDetect.cpp`
  and `AsusAuraMouseController.cpp`: only facts were taken, no code. These are the interface number and usage page
  of each mouse's configuration collection, and the 65-byte write (a report number 0, then 64 bytes).

## Transport

- The mouse's configuration collection has the vendor usage page `0xFF01`. It is on USB interface 0 on most mice,
  and on interface 2 on the Gladius II, Pugio, Strix Impact and TUF Gaming M3 Gen II (OpenRGB).
- PeriHub sends an **output report** of 64 bytes with no report number (through hidapi: `00` followed by the 64
  bytes) and reads the answer, a 64-byte **input report**. OpenRGB drops any answer left waiting before it sends.
  PeriHub does the same.
- Bytes 0–1 of a request are the command. Byte 0 is `12` for the commands that read, and `50` or `51` for the ones
  that change something.
- An answer that starts `ff aa` means the mouse is asleep, disconnected from its receiver, or in a state where it
  refuses the request (libratbag `ASUS_STATUS_ERROR`).
- Integers are little-endian.

## Commands

| Bytes 0–1 | What | Request | Answer |
|---|---|---|---|
| `12 00` | Profile and firmware | — | see below |
| `12 03` | LEDs | byte 2: `00`, or the LED number on mice with separate LEDs | see below |
| `12 04` | Settings | byte 2: `00`, or `02` for separate X and Y DPI | see below |
| `12 05` | Button bindings | byte 2: group (`00`, or `01` for the second group) | see below |
| `51 31` | Set one setting | byte 2: field, bytes 4–5: value | |
| `51 28` | Set one LED | byte 2: LED, 4: mode, 5: brightness, 6–8: red, green, blue | |
| `51 21` | Set one button | bytes 4–5: button code, `01`; 6–7: action code, action type | |
| `50 02` | Switch to profile | byte 2: profile (from 0) | |
| `50 03` | Save to the mouse's memory | — | |
| `12 12` | Identify a mouse behind the ROG Omni receiver | byte 2: `02` | |

`12 12` and `50 02` are not used: PeriHub does not open the Omni receiver, and never switches profiles. libratbag
switches to each profile in turn to read all of them, which is a change. PeriHub only reads the active one.
`51 21` is not used yet either.

### Profile and firmware (`12 00`)

- Byte 10: active profile, from 0. On mice with the `STRIX_PROFILE` quirk, byte 9.
- Byte 11: active DPI level, from 1 (0: none).
- Firmware: bytes 15.14.13 (primary), and 6.5.4 (secondary, the receiver of wireless mice).

### Settings (`12 04`, byte 2 = `00`)

The fields are numbered in order: the DPI levels first (two or four), then polling rate, button response and angle
snapping. `51 31` takes the field's number in byte 2. The polling rate is therefore field 2 on a mouse with two DPI
levels and field 4 on one with four.

| | 2 DPI levels | 4 DPI levels | 4 levels, `SETTINGS_V2` |
|---|---|---|---|
| DPI levels | bytes 4–7, 2 bytes each | bytes 4–11 | bytes 4–11 |
| Polling rate | bytes 8–9 | bytes 12–13 | byte 12, low 3 bits |
| Button response | bytes 10–11 | bytes 14–15 | byte 14 |
| Angle snapping | bytes 12–13 | bytes 16–17 | byte 16 |

- A DPI level is stored as `(dpi − 50) / 50`, so 0 is 50 DPI. Mice with the `DOUBLE_DPI` quirk store half the DPI:
  the value is doubled after that.
- Polling rate: index into 125, 250, 500, 1000 Hz.
- Button response (debounce): index into 4, 8, 12 … 32 ms.
- Angle snapping: 0 off, 1 on.
- With byte 2 = `02` (mice with `SEPARATE_XY_DPI`), bytes 4–19 hold four pairs of X and Y, 2 bytes each, stored the
  same way. libratbag sets only one value per level on these mice, and it is not known whether that sets X, Y or
  both, so **PeriHub does not change their DPI**.

### LEDs (`12 03`)

- Most mice: one answer holds every LED, 5 bytes each from byte 4: mode, brightness, red, green, blue.
- `SEPARATE_LEDS`: asked one LED at a time (byte 2 = LED). Its 5 bytes are at byte 4.
- `LED_V2`: asked one LED at a time. Mode at byte 4, brightness at 5 (0–100 %), colour at 6–8.
- Brightness: 0–4 on most mice. libratbag sends it unscaled on `RAW_BRIGHTNESS` mice, whose range is not known, so
  **PeriHub keeps their brightness as it is**.
- Modes, by number: 0 static, 1 breathing, 2 colour cycle, 3 rainbow wave, 4 reactive, 5 custom, 6 battery. The
  Pugio numbers them differently: 3 is its colour cycle. PeriHub offers only static, breathing and colour cycle.

### Button bindings (`12 05`)

Seventeen pairs from byte 4: action code and action type (0 key, 1 mouse button; code `ff` is disabled). Which
physical button each pair belongs to is the mouse's button mapping. By default it is left, right, middle, back,
forward, DPI, wheel up, wheel down; some mice list their own.

Button and action codes: `f0` left, `f1` right, `f2` middle, `e4`/`e1` back, `e5`/`e2` forward, `e6` DPI cycle,
`e7` DPI target, `e8` wheel up, `e9` wheel down, `ea`–`ef` side buttons A–F, `d0`–`d3` joystick, `d7`/`d8`/`da`/`db`
joystick axes. Key codes follow the USB HID keyboard usages for letters, digits and F-keys.

## Saving

libratbag sends the `51` commands, then `50 03` to store them. That suggests the `51` commands alone last only until
the mouse is turned off, but nobody has confirmed it. PeriHub therefore treats **every** change as one that may stay:
it saves a backup first, reads the settings back after the change, and puts the backup back after 15 seconds unless
the user keeps the change. **Save to the mouse's memory** (`50 03`) comes only after the No warranty box.

## What PeriHub leaves out

- The ROG Omni receiver (`0b05:1ace`) and the mice that use it (Strix Impact III Wireless): it is shared by several
  devices, uses report number 3, and may carry a keyboard's typing.
- Switching profiles, button bindings, DPI on mice with separate X and Y DPI, and brightness on `RAW_BRIGHTNESS` mice.
- Battery: libratbag does not read it.
