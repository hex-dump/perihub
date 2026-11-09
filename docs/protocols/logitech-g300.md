# Logitech G300: what PeriHub relies on

This page describes, in our own words, how the Logitech G300 (and G300s) stores its settings and
how they are read and changed. PeriHub's G300 code is written from this page only. No code from
other projects was copied or translated.

Sources studied (read only, for the protocol facts):

- [libratbag](https://github.com/libratbag/libratbag) (MIT), `driver-logitech-g300.c`: report
  numbers, profile layout, how a profile is written. Its issues #1202, #1216 and #1458 (G300
  settings that did not stick, wrong DPI).
- [ratslap](https://gitlab.com/krayon/ratslap) (GPL-2.0): USB captures of Logitech Gaming Software
  (LGS) editing a G300s, and the meaning of the DPI shift byte.
- [Issue #16](https://github.com/hex-dump/perihub/issues/16): the three profiles of a real G300,
  read with hidapitester and with PeriHub 0.6.1 on Windows, compared with screenshots of LGS.
- [Issue #21](https://github.com/hex-dump/perihub/issues/21): the first saves to that G300, with
  a test build of PeriHub on Windows.

Facts checked on the G300 of issue #16 are marked **(#16)**, those from its first saves **(#21)**.

## Transport

- USB id `046d:c246`. The G300 does not speak HID++.
- Everything goes through **HID feature reports** on **interface 1**. On Windows that interface
  has three collections; only the vendor one (usage page `0xFF80`) answers, the others refuse the
  reports **(#16)**.
- Reading is a GET_REPORT of a feature report; writing a SET_REPORT of the same report, with the
  report id in the first byte.

## Reports

| Id | Length | Meaning |
|---|---|---|
| `F0` | 4 | Active profile and DPI level (read); selects a profile or level (write) |
| `F1` | 2 | Unknown. LGS sends `F1 00`, which turns the light off for a moment (ratslap) |
| `F2` | 2 | Unknown. LGS sends `F2 4F` before editing (ratslap) |
| `F3` | 35 | Profile 1 |
| `F4` | 35 | Profile 2 |
| `F5` | 35 | Profile 3 |

The numbering of the profiles follows the report ids: the profile LGS draws at the top of its
wheel is `F3` (ratslap), and on the G300 of #16 `F3` was the profile LGS listed first.

## Report F0, active profile

Read: byte 1 holds the active profile in its high nibble (0 to 2) and the active DPI level in bits
1–3 (0 to 3). `F0 02 00 00` is profile 1, level 2 **(#16)**.

Write: `F0`, then `0x80 | profile << 4` selects a profile, `0x40 | level << 1` a DPI level
(libratbag; ratslap uses `0x80`, `0x90`, `0xA0` for the three profiles). PeriHub does not send
these.

## Profile reports F3–F5

| Byte | Meaning |
|---|---|
| 0 | report id |
| 1 | light: bit 0 red, bit 1 green, bit 2 blue, each fully on or off (8 colours, `0` is off). The other bits were 0 on every G300 seen |
| 2 | report rate: `00` 1000 Hz, `01` 125 Hz, `02` 250 Hz, `03` 500 Hz |
| 3–6 | the four DPI levels |
| 7 | DPI shift |
| 8–34 | nine buttons, 3 bytes each |

All of this matched LGS on the G300 of #16 **(#16)**.

### DPI levels (bytes 3–6)

- The low bits are the DPI divided by 250: `1` is 250 DPI, `10` (`0x0A`) is 2500 DPI. Both sources
  agree, and #16 confirms it: `02 84 06 0A` read as 500, 1000, 1500, 2500 in LGS **(#16)**.
- Bit `0x80` marks the default level, the one used after a profile change. Exactly one level has
  it.
- For a while (2021–2023) libratbag used steps of 50 instead of 250 and wrote wrong DPI values to
  G300s (its issues #1216 and #1458); it went back to 250. The four levels are always present:
  the G300 has no way to turn one off.

### DPI shift (byte 7)

From the ratslap captures: the low nibble is the shift DPI divided by 250, like a DPI level, and
bit `0x40` means "no shift DPI". LGS writes `0x40` when the shift is off. This matches #16:
profile 2, whose G9 is "DPI shift", has `0x04` (1000 DPI); profiles 1 and 3, which have no shift
button, have `0x40` **(#16)**. libratbag only says "DPI index for shift, but something else too",
which the captures explain.

### Buttons (bytes 8–34)

Nine entries of three bytes: function, modifiers, key. In order: left button, right button, wheel
button, then G4, G5, G6, G7, G8 and G9 as printed on the mouse **(#16)**.

| Function | Meaning |
|---|---|
| `00` | a key (modifiers and key below), or nothing when both are 0 |
| `01`–`09` | a mouse button: `01` left, `02` right, `03` middle, `04` back, `05` forward **(#16)**; `06`–`09` further buttons |
| `0A` | DPI up |
| `0B` | DPI down |
| `0C` | cycle DPI levels |
| `0D` | next profile |
| `0E` | DPI shift, while held |
| `0F` | back to the default DPI level |

Modifiers are a bit mask: bits 0–3 left Ctrl, Shift, Alt, Windows; bits 4–7 the same keys on the
right. The key is a USB keyboard usage (`04` is A, `2C` space…); a modifier alone has key 0
**(#16: G7 of profile 3 is Ctrl alone)**. For anything other than function `00`, modifiers and
key are 0.

LGS never lets the left and right buttons be changed (ratslap).

## Writing a profile

libratbag writes a profile with **one SET_REPORT of the whole 35-byte report**, and nothing else.

LGS sends more (ratslap captures), and ratslap repeats it before editing: `F0 42 39 00`,
`F2 4F`, `F0 00 00 00`, `F1 00`, then `F0 42 00 00` ("start edit"). What these do is not known;
ratslap's own notes say several of them are not needed. `F1 00` turns the light off.

Writes are slow: ratslap waits half a second after each profile it writes.

libratbag issue #1202 (2021) reports G300 settings that looked saved but were gone after
replugging the mouse; the cause was never found. In that report the button that did not stick
was set to "previous profile", a function the G300 does not have.

## What PeriHub does with it

- It reads `F3`, `F4`, `F5` and `F0`, and nothing else, unless the user turned on experimental
  editing for the G300.
- With experimental editing on, it saves a profile like libratbag: one SET_REPORT of the 35-byte
  report, and none of the other LGS commands above. It changes only the fields being edited and
  keeps every other bit as read: the unused bits of the light byte, the DPI value of a disabled
  shift, and button entries it does not change (key combinations, unknown functions).
- It accepts only the values listed on this page: the 8 colours, the 4 report rates, DPI from 250
  to 2500 in steps of 250, and the functions above with mouse buttons 1–5. Left and right buttons
  keep what they have.
- Before writing it saves the three profile reports as a backup, waits half a second after the
  write, then reads the profile back (a few times, in case the mouse is still busy) and compares it
  byte by byte. A save is undone unless the user keeps it, as for other untested mice.
- It never selects a profile or DPI level through `F0`.

Saving this way worked on a real G300 **(#21)**: the report rate of profile 3 read back changed
(`F5 02 03 …` became `F5 02 00 …`), and the tester saw the buttons and the light colour change. On
that mouse every colour with blue in it showed without the blue (blue was dark, cyan green,
magenta red), and LGS showed them the same way: its blue light does not work, the bytes were
right.

## Not known yet

- Whether the profile written this way stays after the mouse is unplugged (libratbag #1202), and
  whether a change to the active profile applies at once or only after switching profiles.
- What `F1`, `F2` and the `F0 42 …` commands of LGS do.
- What the high bits of the light byte and of the DPI bytes mean (they were 0 on every G300 seen).
