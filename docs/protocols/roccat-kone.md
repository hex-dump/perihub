# Roccat Kone Pure, Kone XTD and Kone EMP

How PeriHub reads three wired Roccat mice. **No mouse has been tried with PeriHub yet**, and PeriHub only reads them:
the sources disagree on details that a write would depend on (see the end).

Sources:

- [libratbag](https://github.com/libratbag/libratbag) (MIT), commit `8235b5b`: `src/driver-roccat.c` (Kone XTD) and
  `src/driver-roccat-kone-pure.c` (Kone Pure), © 2015 Red Hat, Inc., `src/driver-roccat-kone-emp.c` (Kone EMP),
  © 2021 Alexandre Laurent, and `data/devices/roccat-kone-*.device`.
- The Linux kernel (GPL-2.0), `drivers/hid/hid-roccat-konepure.c` and `hid-roccat-koneplus.c`: only a fact, no code:
  the configuration of these mice goes through their interface whose protocol is "mouse".
- `lsusb -v` dumps in [linuxhw/LsUSB](https://github.com/linuxhw/LsUSB): on `2dbe`, `2dc2`, `2e22` and `2e24`,
  interface 0 is the boot mouse, with a long report descriptor (232–273 bytes), and interface 1 a keyboard interface
  for the keys the mouse's own buttons send.

## Which mice

| USB id | Mouse | Settings report | DPI stored as |
| --- | --- | --- | --- |
| `1e7d:2dbe`, `1e7d:2dc2` | Kone Pure | 31 bytes | DPI / 50 |
| `1e7d:2e22` | Kone XTD | 43 bytes, with a checksum | DPI / 50 |
| `1e7d:2e24` | Kone EMP | 41 bytes, with a checksum | (DPI − 100) / 100 |

libratbag also lists `1e7d:2dcb` as a Kone Pure, but its USB interfaces are laid out differently (three, a short
mouse descriptor), and no source says where its configuration is: PeriHub leaves it out.

## Interface

Interface 0, the boot mouse interface. The Linux udev rule opens it only: it carries the mouse's movements and
clicks. Interface 1, with the keys the buttons can send, stays closed.

## Feature reports

All have a report number.

| Report | Length | What |
| --- | --- | --- |
| `04` | 3 | Write `04 profile 80`: the next read of report `06` gives that profile's settings (`04 profile 90`: report `07`, its buttons). Read: byte 1 is `01` when the mouse is ready, `03` while it is busy (wait 100 ms), `02` on an error |
| `05` | 3 | Read: byte 2 is the profile in use, 0–4 |
| `06` | see above | A profile's settings |
| `07` | 59 / 77 / 71 | A profile's buttons, with a checksum |
| `08` | | Macros |

The checksum is the sum of every byte but the last two, as a 16-bit little-endian number in the last two bytes.

PeriHub sends only `04 profile 80` (it chooses what the next read returns, and changes no setting), waits for the mouse
to be ready, and reads `05` and `06`.

## Settings (report `06`, report number included)

Kone Pure and Kone XTD:

| Byte | What |
| --- | --- |
| 0 | `06` |
| 1 | Report length on the XTD |
| 2 | Profile, 0–4 |
| 6 | One bit per DPI slot: set means on |
| 7–11 | X DPI of the 5 slots |
| 12 | DPI slot in use, 0–4 |
| 13–17 | Y DPI of the 5 slots |
| 19 | Polling rate: `0`–`3` = 125, 250, 500, 1000 Hz |
| last 2 | Checksum (XTD) |

Kone EMP:

| Byte | What |
| --- | --- |
| 0 | `06` |
| 1 | `29` |
| 2 | Profile, 0–4 |
| 6 | One bit per DPI slot: set means on |
| 7–11 | DPI of the 5 slots |
| 12–16 | The same, for Y |
| 17 | DPI slot in use |
| 18 | Polling rate: `0`–`3` = 125, 250, 500, 1000 Hz |
| 19–38 | Lighting |
| 39–40 | Checksum |

PeriHub shows the DPI of the slots that are on (X), the slot in use, the polling rate and the profile in use. An
answer for another profile, a wrong checksum where there is one, or values outside these ranges leave the page with
what could be read and a note.

## Why only reading

libratbag writes the Kone Pure's settings without a checksum, with a comment saying it has none, while the XTD's and
the EMP's have one; it also writes the buttons report whenever a profile changed. Until someone with one of these
mice sends a test report, PeriHub writes nothing.
