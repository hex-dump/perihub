# Keyboards with QMK firmware: VIA and Vial

How PeriHub talks to keyboards whose QMK firmware speaks the VIA protocol, and to keyboards with Vial, a fork of
QMK. **No keyboard has been tried with PeriHub yet**: everything here comes from the sources below, and every change
PeriHub makes is experimental until someone confirms it.

Sources, read for facts only. No code was copied, and none was translated: QMK and Vial are GPL.

- [QMK](https://github.com/qmk/qmk_firmware) (GPL-2.0-or-later), commit `d194c07`: `quantum/via.h`, `quantum/via.c`,
  `tmk_core/protocol/usb_descriptor.h` and `.c`, `quantum/backlight/backlight.c`, `quantum/rgb_matrix/rgb_matrix.c`,
  and the history of `quantum/via.h` (commits `bc6f8dc8b`, `7f805cc77`, `cc291cff7`).
- [vial-qmk](https://github.com/vial-kb/vial-qmk) (GPL-2.0), commit `dd43959`: `quantum/vial.h`, `quantum/vial.c`,
  `quantum/via.h`, `quantum/via.c`, `quantum/vialrgb.h`, `quantum/vialrgb.c`, `quantum/vialrgb_effects.inc`,
  `builddefs/build_vial.mk`, `tmk_core/protocol/usb_descriptor.h`.
- [vial-gui](https://github.com/vial-kb/vial-gui) (GPL-2.0): `src/main/python/util.py`, for how Vial finds its
  keyboards.

## Which keyboards

QMK keyboards have no common USB id: every keyboard has its own. What they share is the **raw HID interface**:

- A QMK keyboard built with raw HID has a USB interface of its own for it, with its own two endpoints (in and out),
  never shared with the keyboard's typing. Its report descriptor holds one application collection, **usage page
  `0xFF60`, usage `0x61`**, with a 32-byte input report and a 32-byte output report, and no report id. QMK keeps it
  at interface 1, but only when raw HID is built in: without it, the next interface (a mouse or shared one, which
  can carry keys) takes number 1. So PeriHub never trusts the number alone, it looks at the descriptor.
- **Vial keyboards** say so in their USB serial number, which Vial's build always sets to `vial:f64c2b3c`. Vial's own
  program finds them that way, then checks for the `0xFF60`/`0x61` collection. PeriHub does the same, and reads them
  without being asked.
- A serial number with `vibl:d4f8159c` is Vial's **bootloader**: PeriHub never opens it.
- Any other `0xFF60`/`0x61` collection may be a VIA keyboard, or a keyboard whose firmware answers raw HID with code of
  its own, where any byte may do anything. VIA's own programs know VIA keyboards from a list of definitions that
  PeriHub cannot ship (GPL-3). So PeriHub **only lists them**, and talks to one only after the user turns it on for
  that keyboard.

### On Linux

Opening the raw HID interface needs a udev rule, and a rule must never open an interface that carries typing.

- The report descriptor of every hidraw node, `/sys/class/hidraw/hidrawN/device/report_descriptor`, can be read by
  every user without opening the device. PeriHub reads it there.
- The optional rule PeriHub installs for a keyboard, on request and through the system's password window, names the
  keyboard's vendor id, product id and the number of the interface whose descriptor PeriHub saw: exactly one
  collection, `0xFF60`/`0x61`, and no keyboard, keypad, consumer or system usage. Nothing else of the keyboard is
  opened.
- Vial's own instructions give a rule that opens every interface of a Vial keyboard to the user: if it is installed,
  PeriHub can read the keyboard without its own rule.

## Transport

- Every request is a **32-byte output report** without a report id (on the wire, after the report number 0 that the
  system expects). Every answer is a 32-byte input report.
- The keyboard answers each request with the same 32 bytes, some of them filled in. **Byte 0 of the answer is the
  command**, or `ff` if the firmware does not handle it: then nothing else in the answer is data.
- A keyboard's own code may handle any command before QMK does, and may also give a known command another meaning.
  PeriHub sends only the commands below.

## VIA commands

Byte 0 is the command.

| Command | What | PeriHub |
| --- | --- | --- |
| `01` | Protocol version: bytes 1–2, big-endian | read |
| `02 nn` | A keyboard value: `01` uptime, `02` layout options, **`03` the state of the key matrix** (which keys are held: a keylogger's tool), `04` firmware version, `06` keycodes version | **not sent** (none is needed) |
| `03 nn` | Sets a keyboard value: layout options, or device indication (flashes the lights) | not sent |
| `04`, `05` | Reads / **writes** one key of the keymap. A write goes to the memory at once | not sent |
| `06` | **Resets the keymap** to the firmware's | never sent |
| `07`, `08`, `09` | Lighting and other values: set, get, save (see below) | see below |
| `0a` | **Resets the memory** (when the firmware allows it) | never sent |
| `0b` | **Jumps to the bootloader** (Vial, when unlocked) | never sent |
| `0c`, `0d` | Number of macros, size of the macro buffer | read |
| `0e`, `0f`, `10` | Reads / writes / **resets** the macros | not sent |
| `11` | Number of layers: byte 1 | read |
| `12`, `13` | Reads / writes the keymap in blocks | not sent |
| `14`, `15` | Reads / writes a rotary encoder's keys | not sent |
| `fe` | Vial commands (Vial only, see below) | see below |

### Protocol versions

| Version | Firmware | Lighting format |
| --- | --- | --- |
| `0009` | Vial (all current Vial firmware) | old format, no channel byte |
| `000b` and older | QMK before February 2023 | **`000b` is both**: the channel byte came in November 2022 (QMK `bc6f8dc8b`, "VIA V3") without a new version number |
| `000c` | QMK from February 2023 (`7f805cc77`) | channel format |
| `000d` | QMK from April 2026 (`cc291cff7`): adds keyboard value `06`, nothing else changes | channel format |

PeriHub changes the lighting of a VIA keyboard only with version `000c` or `000d`. Any other version is read for the
version, layers and macros only.

## Lighting, channel format (VIA `000c` and `000d`)

Requests: `08 ch id` gets a value, `07 ch id data…` sets it, `09 ch` saves the channel's settings to the keyboard's
memory. The answer to `08` repeats the request with the value from byte 3. A channel the keyboard does not have
answers `ff` in byte 0.

| Channel | What | Values |
| --- | --- | --- |
| `00` | the keyboard's own values | **never sent**: their meaning is the keyboard's |
| `01` | backlight (one colour) | `01` brightness, `02` breathing on/off |
| `02` | RGB light (underglow) | `01` brightness, `02` effect, `03` effect speed, `04` colour (hue, saturation) |
| `03` | RGB matrix (per key) | `01` brightness, `02` effect, `03` effect speed, `04` colour (hue, saturation) |
| `04` | audio | not sent |
| `05` | LED matrix (one colour per key) | `01` brightness, `02` effect, `03` effect speed |

What QMK does with them:

- **Set** changes the lighting without storing it (QMK's functions with no memory write), with one exception:
  turning the backlight's breathing on or off writes the memory at once. **Save** writes the channel's settings to
  memory.
- **Brightness** travels on a 0–255 scale. The keyboard keeps it on its own scale (0 to its maximum, 255 or less), so
  a value set and read back may differ by a step or two when the maximum is below 255: set stores `value × max / 255`
  (rounded down, or `scale8`), get returns `stored × 255 / max` (rounded down). The backlight's levels are few, so
  its brightness hardly ever reads back as set.
- **Effect** `0` means off. Any other number is the firmware's own numbering of the effects it was built with:
  the same number is a different effect on another keyboard, so PeriHub shows the number and offers only "off" and
  the effect it saw in use.
- **Speed** and **colour** (hue, saturation, 0–255 each) read back exactly as set.
- With the RGB matrix turned off, setting the colour or brightness does nothing (QMK returns at once): the effect must
  be turned on first, and nothing but the effect is changed while it stays off.

What PeriHub does:

- **Backlight**: not read yet, never changed (breathing writes the memory; brightness does not read back).
- **RGB light, RGB matrix, LED matrix**: effect off / on (the effect it saw), speed, colour; brightness in steps of
  10 %, read as the nearest step, so that a value that comes back a step or two off still reads as what was set.
- Changes are sent effect first, then speed, colour, brightness. **Apply** sends them without saving; **Save to
  memory** sends `09 ch` after them, after the No warranty box the first time. Both are read back, and undone after
  15 seconds unless kept.

## Vial

Vial's VIA layer is protocol `0009`, with the **old lighting format**: `07`, `08` and `09` carry the value id in
byte 1 and no channel byte, and every lighting module the firmware has gets each request (so the same bytes mean
something else than in the channel format). PeriHub uses only VialRGB there.

Vial commands start with `fe`, the command in byte 1. The answer starts at byte 0.

| Command | What | PeriHub |
| --- | --- | --- |
| `fe 00` | Keyboard id: bytes 0–3 Vial protocol version (little-endian, `6` in current firmware), bytes 4–11 the keyboard's unique id, **byte 12 = 1 when VialRGB is built in** (so that other programs need not read the definition) | read |
| `fe 01`, `fe 02` | Size and pages of the keyboard's definition (an xz-compressed JSON: matrix, layout, lighting) | not sent yet |
| `fe 03`, `fe 04` | Encoder keys | not sent |
| `fe 05` | Unlock status | not sent |
| `fe 06`, `fe 07`, `fe 08` | **Starts / polls the physical unlock, locks** | never sent |
| `fe 09`–`fe 0c` | QMK settings: query, get, set, **reset** | not sent |
| `fe 0d` | Tap dance, combos, key overrides | not sent |

The physical unlock (holding keys the definition names) guards what could do harm: the bootloader jump, the
`QK_BOOT` keycode, writing macros and reading the key matrix. PeriHub never needs it.

### VialRGB

Only when byte 12 of the keyboard id is 1. Requests are 32 bytes; byte 0 is `08` (get), `07` (set) or `09` (save),
byte 1 the VialRGB value, arguments from byte 2.

| Request | Answer from byte 2 |
| --- | --- |
| `08 40` info | VialRGB protocol version (2 bytes, little-endian, `1`), then the highest brightness the keyboard takes |
| `08 41` mode | effect id (2 bytes, little-endian), speed, hue, saturation, brightness (on the keyboard's own scale) |
| `08 42 a b` supported effects | the ids above `a b` (little-endian) the firmware has, 2 bytes each, little-endian, the rest filled with `ff ff`; ask again from the last id until `ff ff` |
| `07 41 a b s h t v` | sets effect, speed, hue, saturation, brightness at once, without storing them |
| `09` | stores the RGB matrix settings in memory (every lighting module the firmware has stores its own) |

- Effect ids are **the same on every Vial keyboard**, so they have names: 0 off, 1 direct (colours sent by a program;
  PeriHub never sets it), 2 solid colour, 3 alphas and mods, 4 gradient up/down, 5 gradient left/right, 6 breathing,
  7 band saturation, 8 band brightness, 9 band pinwheel saturation, 10 band pinwheel brightness, 11 band spiral
  saturation, 12 band spiral brightness, 13 cycle all, 14 cycle left/right, 15 cycle up/down, 16 rainbow moving
  chevron, 17 cycle out/in, 18 cycle out/in dual, 19 cycle pinwheel, 20 cycle spiral, 21 dual beacon, 22 rainbow
  beacon, 23 rainbow pinwheels, 24 raindrops, 25 jellybean raindrops, 26 hue breathing, 27 hue pendulum, 28 hue wave,
  29 typing heatmap, 30 digital rain, 31 solid reactive simple, 32 solid reactive, 33 solid reactive wide, 34 solid
  reactive multiwide, 35 solid reactive cross, 36 solid reactive multicross, 37 solid reactive nexus, 38 solid
  reactive multinexus, 39 splash, 40 multisplash, 41 solid splash, 42 solid multisplash, 43 pixel rain, 44 pixel
  fractal. An id the firmware does not have turns the lighting into QMK's effect 0: PeriHub offers only the supported
  ones.
- Brightness is on the keyboard's own scale (0 to the highest from `08 40`), and the keyboard caps it there: it reads
  back exactly as set. PeriHub shows it in steps of 5 %.
- As above, with the effect off the colour and brightness are not applied.

## Not done yet

Keymaps (they need the keyboard's layout: Vial's definition, compressed, or VIA's definitions, which PeriHub cannot
ship), macros, encoders, layout options, the backlight, lighting on QMK before `000c`, and Vial's other lighting
modules.
