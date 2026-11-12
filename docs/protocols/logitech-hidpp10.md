# Logitech HID++ 1.0 mice: G9, G9x, G500, G500s, G700, G700s

How PeriHub talks to the Logitech gaming mice of the HID++ 1.0 years, on their cable. Their own software, Logitech
Gaming Software, is no longer offered by Logitech (removed on 31 October 2026). **None has been tried with PeriHub
yet**: everything here comes from the sources below.

Sources:

- [libratbag](https://github.com/libratbag/libratbag) (MIT), commit `8235b5b`: `src/hidpp10.c`, `src/hidpp10.h`,
  `src/driver-hidpp10.c`, `src/hidpp-generic.c` and the `data/devices/logitech-g*.device` files of these mice.
- [Solaar](https://github.com/pwr-Solaar/Solaar) (GPL-2.0), commit `e7304c4`, `lib/logitech_receiver/descriptors.py`:
  only facts, no code: these mice speak HID++ 1.0 and are reached on **USB interface 1**.

## Models

| Mouse | USB ID | Profile layout | DPI codes (libratbag) | Profiles | PeriHub |
| --- | --- | --- | --- | --- | --- |
| G9 | `046d:c048` | G9 | list: `0x80` = 200 … `0x8f` = 3200, steps of 200 | 5 | read-only |
| G9x | `046d:c066` | G500 | range 200–5700 in steps of 50 | 5 | read-only |
| G9x (Call of Duty MW3 Edition) | `046d:c249` | G500 | range 0–5700 in steps of 23.53 | 5 | read-only |
| G500 | `046d:c068` | G500 | range 0–5700 in steps of 23.53 | 1 | read-only |
| G500s | `046d:c24e` | G500 | range 0–8200 in steps of 50 | 1 | DPI in use (RAM) |
| G700 (cable) | `046d:c06b` | G700 | range 0–5700 in steps of 23.53 | 5 | read-only |
| G700s (cable) | `046d:c07c` | G700 | range 0–8200 in steps of 50 | 5 | DPI in use (RAM) |

A range turns code `i` (from 1) into `min + step × i`, rounded to a multiple of 25; code 0 is not a DPI. Without a
table libratbag takes `code × 50`. The DPI in use is changed only where the two agree and match the mouse's
published range: the G500s and the G700s (`code × 50`, 50–8200).

Solaar codes the G9 and the original G9x differently (short register, `0x81`–`0x8f` = 100–1500): the sources disagree,
so their DPI is shown as libratbag reads it and never changed. The steps of 23.53 have not been checked either.

The G700 and G700s through their receiver (`046d:c531`) are left out. The **G400s** (`046d:c24c`) is not one of
these mice: the only tool known to configure it sends a vendor USB control request (`bmRequestType 0x40`,
`bRequest 2`, `wValue 0x8e`) after detaching the kernel driver, writes only and reads nothing back. PeriHub, which
talks through HID, cannot send it.

## Interface

HID++ reports on interface 1 (Solaar). The short report has number `0x10` and 7 bytes, the long one `0x11` and 20
bytes. On Windows they are two collections of usage page `0xFF00`, usage 1 (short) and 2 (long), each with a path
of its own: PeriHub opens both, sends a short request through the short one and reads the answers from both. On
Linux they are one hidraw node. Interface 1 may also carry the keys and macros the mouse's own buttons send: the
Linux udev rule opens interface 1 only, never a keyboard.

## Messages

A request: `10 00 <sub> <register> p0 p1 p2` (short) or `11 00 <sub> <register> <16 bytes>` (long). Device index
`00`: a mouse on its cable. Sub-ids: `80` set a short register, `81` get a short register, `82` set a long register,
`83` get a long register. A get of a long register is a short request answered by a long report. The answer starts
with the same device index, sub-id and register; an error is `10 00 8f <sub> <register> <code>`.

## Registers PeriHub reads

| Register | Request | Answer |
| --- | --- | --- |
| `0f` profile in use | `81 0f` | p0 type: `00` by index (p1 = index in the directory), `01` by address (p1 page, p2 offset), `ee` EEPROM, `ff` factory profile. An error: no profiles |
| `63` DPI in use | `83 63` (G9: `81 63`) | X code in bytes 4–5, Y code in bytes 6–7, little-endian (G9: one byte, byte 4) |
| `64` polling rate | `81 64` | p0 = milliseconds between reports: `1000 / p0` Hz |
| `a2` read memory | `83 a2 <page> <offset in 16-bit words> 00` | 16 bytes of the page from byte 4 |
| `f1` firmware | `81 f1 01`, `81 f1 02` | `01`: p1 major, p2 minor; `02`: build, p1–p2 big-endian. Some mice refuse it |

## Memory

Pages of 512 bytes: page 0 is RAM, page 1 the profile directory, pages 2–31 flash (profiles and macros). The last two
bytes of a page hold a CRC-16/CCITT (polynomial `0x1021`, seed `0xFFFF`) of the 510 before it, big-endian. libratbag
notes that a profile with a wrong CRC is used by the mouse all the same; PeriHub shows it with a note. A directory with
a wrong CRC leaves the mouse read-only.

The directory: entries of 3 bytes (page, offset, LED mask), as many as the mouse has profiles, ended by a page `ff`.
libratbag reads a profile from offset 0 of its page; PeriHub shows nothing of a profile at another offset.

### Profiles

G500 layout (G500, G500s, G9x), 503 bytes:

| Byte | What |
| --- | --- |
| 0–2 | Colour: red, green, blue (the G9x has a light; the G500 and G500s do not) |
| 3 | Unknown |
| 4–33 | Five DPI modes of 6 bytes: X code and Y code, **big-endian**, then 2 bytes of LED nibbles. Code 0: unused |
| 34 | Angle correction |
| 35 | Default DPI mode |
| 36–37 | Unknown |
| 38 | Polling rate, `1000 / value` Hz |
| 39–77 | 13 buttons of 3 bytes |
| 78– | Metadata: `LGS02`, the profile name in 23 UTF-16 units, the macro names |

G700 layout (G700, G700s), 499 bytes: five DPI modes of 4 bytes (X code, Y code, 2 bytes of LED nibbles) in 0–19,
default DPI mode at 20, unknown 21–23, polling rate at 24, unknown 25–34, 13 buttons in 35–73, metadata from 74.

G9 layout: colour 0–2, unknown 3, five DPI modes of 3 bytes (one code, 2 bytes of LED nibbles) in 4–18, default DPI
mode at 19, unknown 20–21, polling rate at 22, 10 buttons in 23–52, unknown 53–55, metadata from 56.

### Buttons

| First byte | What |
| --- | --- |
| `81` | Mouse button: bytes 1–2 little-endian, one bit set (bit 0 left, 1 right, 2 middle, 3 back, 4 forward) |
| `82` | Key: byte 1 modifiers (bit 0 Ctrl, 1 Shift, 2 Alt, 3 Win, 4–7 the right ones), byte 2 keyboard usage |
| `83` | Special, bytes 1–2 little-endian: `01` scroll left, `02` scroll right, `03` battery level, `04` DPI up, `05` DPI cycle up, `08` DPI down, `09` DPI cycle down, `10` profile up, `11` profile cycle up, `20` profile down, `21` profile cycle down |
| `84` | Consumer control (media key), bytes 1–2 **big-endian** |
| `8f` | Disabled |
| other | A macro: byte 0 its page, byte 1 its offset in words |

## What PeriHub sends

- Reads: the five registers above, and the memory reads of the directory and of the profile in use, once per
  connection (a page is 32 reads).
- One change, on the G500s and the G700s: **the DPI in use**, `11 00 82 63 <x lo> <x hi> <y lo> <y hi> 00…`, both
  codes the same (`code × 50` DPI). The mouse answers `10 00 82 63 …`. It is a register in RAM: the mouse forgets it
  when it is unplugged, when it switches profile, or when its own DPI buttons pick another stored level. PeriHub reads
  it back, and puts the DPI from before back after 15 seconds unless the change is kept.

Never sent, and not in the list the driver's commands go through:

- writing the profile register (`80 0f`): libratbag switches away from the factory profile inside a read; PeriHub
  shows the factory profile and leaves it;
- writing the polling rate (`80 64`): libratbag builds that request with the sub-id of a read (`81`), so nobody has
  shown a working write;
- the memory commands of register `a0` (`02` erase a page, `03` copy RAM to flash) and the "hot" payload that fills
  the RAM page before a copy (long messages `92`/`93`, reset through register `a1`): libratbag erases the profile directory before writing it again, and a write that stops
  half way would leave the mouse without profiles;
- the LED registers (`51`, `54`, `57`), the optical sensor settings (`61`) and the pairing registers of the receivers
  (`b2`, `b5`).
