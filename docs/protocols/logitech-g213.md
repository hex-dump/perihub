# Logitech G213 Prodigy

How PeriHub talks to the Logitech G213 Prodigy gaming keyboard (`046d:c336`). The keyboard speaks Logitech's HID++ 2.0 on a
vendor collection of its own; its lighting has five areas, a few effects built into the keyboard, and no per-key colours.
The code is `crates/perihub-core/src/peripheral/logitech_g213.rs`, written from this page alone. None of it has been
tried on a real keyboard yet, so the keyboard ships as experimental: the first test report for
[issue #96](https://github.com/hex-dump/perihub/issues/96) (from a test build, or from the release that has it) is meant
to show whether it matches.

## Sources

Facts only from the GPL ones (commands, bytes, ids), described here in our own words; no code was copied, nothing was
translated line by line, and the Rust was written with none of them open. The MIT ones are credited in
`THIRD-PARTY-NOTICES.md` and in the credits of the app.

- **OpenRGB** (GPL-2.0-or-later, commit 6b681e3), `Controllers/LogitechController` (`LogitechG213Controller`,
  `RGBController_LogitechG213` and the detector): the USB id, the interface and the usage page and usage of the
  collection that takes the lighting commands, the five areas, the report bytes of the fixed colour and of the
  effects, the speed unit, the wave directions, and that the keyboard's answer is read after each command.
- **g810-led** (GPL-3.0, commit efa3c35), `src/classes/Keyboard.cpp` and `Keyboard.h`: the same commands as sent to other
  Logitech keyboards of that family, with the G213 in its list; the numbers of the effect groups, the wave variants
  (horizontal, vertical, from the centre), that the G213 needs no "commit" command, and the byte that decides whether an
  effect is stored in the keyboard.
- **G213Colors** (MIT, commit 369daa2, by TimeWaster): the author captured what the official software sends and wrote
  the byte layouts of the fixed colour, the breathing and the colour cycle from those captures; that each command must
  be followed by reading the keyboard's 20-byte answer, "otherwise commands may not be completely executed"; that the
  keyboard does not keep what was sent, and starts with a colour cycle after it is plugged in; and the range of 32 to
  65535 milliseconds that a time is given in.
- **gdevd** (MIT or Apache-2.0, taken under MIT, commit 60fcfc3), `src/drivers/g213.rs` and `src/lib.rs`: the byte that
  holds the brightness of an effect (0 to 100, 100 by default), the wave layout, the same four wave directions as
  OpenRGB, and a command that switches the startup effect on or off.
- **The Linux kernel** (`drivers/hid/hid-logitech-hidpp.c`, GPL-2.0) and **Solaar** (GPL-2.0): how a HID++ 2.0 request
  and an error answer are laid out, and what the root feature (index 0) answers. Facts only, as for the other Logitech
  drivers of PeriHub.
- **Issue #96** (a Windows user, PeriHub 0.9.6): the USB id, `bcdDevice` 0x0900, and the HID collections and report
  descriptors below.

## The device

`046d:c336`, two USB interfaces. What the owner's report of PeriHub 0.9.6 on Windows shows:

| Interface | Usage page | Usage | Report | What it is |
|---|---|---|---|---|
| 0 | `0001` | `0006` | none (8 in, 1 out) | the typing keyboard (boot protocol) |
| 1 | `0001` | `0006` | id `01`, 20 in | a second keyboard collection, 20 keys at a time |
| 1 | `000c` | `0001` | id `02`, 2 in | media keys |
| 1 | `ff43` | `0602` | id `11`, 19 bytes in and out (20 with the id) | **HID++ long reports: the lighting commands** |
| 1 | `ff43` | `0604` | id `12`, 63 bytes in and out (64 with the id) | HID++ very long reports: not used |

OpenRGB opens interface 1, usage page `ff43`, usage `0602`. **PeriHub does the same, and only on Windows.** Windows gives each
collection of an interface its own path, so the vendor collection is opened alone and nothing typed can reach PeriHub.
On Linux one hidraw node carries every collection of an interface, so the node of interface 1 also carries what is
typed (and the media keys): PeriHub never opens it there, the udev rule does not list the keyboard, and
`opened_on_linux` is false for it (the same as the EVision and Wraith keyboards). A Linux version needs a different way to
reach the collection, such as a rule the user turns on; it is not in this build.

## The report

Every command is one HID++ 2.0 long report of 20 bytes, sent as an output report with the id `11` and answered by an input
report with the same id:

| Byte | What |
|---|---|
| 0 | `11` (report id: HID++ long) |
| 1 | `ff`: the device index of a device on its own USB connection (all four projects send it) |
| 2 | feature index: `00` is the root feature, every project sends the lighting at `0c` |
| 3 | function in the high four bits, a software id the sender picks in the low four (the answer repeats it) |
| 4 to 19 | parameters, zeros up to the end |

An error comes back as `11 ff ff`, the feature index and the function and software id that were asked, then an error code
(1 unknown, 2 invalid argument, 3 out of range, 4 hardware error, 5 Logitech internal error, 6 invalid feature index,
7 invalid function, 8 busy, 9 unsupported).

## What PeriHub reads

Only the root feature, which every HID++ 2.0 device has. Nothing else is read, because a feature other than the root
sits at an index that differs from device to device, and no source names the others for this keyboard.

| Request | Answer |
|---|---|
| `11 ff 00 1a 00 00 5a`: protocol version (function 1, any byte as the ping value, `5a` here) | `11 ff 00 1a MM mm 5a`: version MM.mm, the ping value repeated |
| `11 ff 00 0a hh ll`: where is the feature with id `hhll` (function 0) | `11 ff 00 0a II TT VV`: its index (0: not listed), its flags and its version |

PeriHub asks for the version, then for the index of `8070` (colour LED effects: where the lighting is expected) and of the
features `0001` feature set, `0003` device information, `0005` device name, `8040` brightness control, `8071` RGB effects and
`8081` per-key lighting, and puts every answer into the test report. All the sources send the lighting at index `0c`, and `0c`
is where Logitech puts `8070` on the other keyboards whose lighting is sent the same way, so:

- **`8070` listed at `0c`: the lighting may be changed** (experimental).
- Anything else, or no answer to the version: the page is read-only, with the reason.

## The lighting

Function 3 of the feature at `0c`, with the software id `a`: byte 3 is `3a`. OpenRGB and g810-led send `3c` for the effects
and `3a` for the fixed colours, G213Colors and gdevd send `3a` for everything: the software id is only an echo, so PeriHub
sends `3a`. Byte 4 is the area: `00` the whole keyboard, `01` to `05` one area (OpenRGB's names: left, middle, right, arrows
and home keys, numpad). Byte 5 is the effect. Colours are bytes red, green, blue. A time is a 16-bit big-endian number of
milliseconds for one cycle (OpenRGB's speed times 100, 1000 for fast and 20000 for slow).

| Effect | Byte 5 | The rest of the report (byte: value) |
|---|---|---|
| Off | `00` | all zeros (OpenRGB) |
| Fixed colour | `01` | 6 to 8 colour, 9 `02` (OpenRGB, G213Colors, gdevd; g810-led leaves it 0) |
| Breathing | `02` | 6 to 8 colour, 9 and 10 time, 11 `00`, 12 brightness |
| Colour cycle | `03` | 6 to 8 `ff ff ff` (G213Colors, gdevd; OpenRGB leaves them 0), 11 and 12 time, 13 brightness |
| Wave | `04` | 6 to 8 zeros, 11 time high byte, 12 time low byte, 13 direction, 14 brightness, 15 time high byte |

- **Brightness** is the byte that G213Colors sends as a constant `0x64` (100) in the breathing and the cycle, OpenRGB sends in the
  same two (and leaves it 0 in the wave), and g810-led sends in all three ("unused?"). Only gdevd names it, as a percentage
  from 0 to 100 with 100 as the default. It exists in the breathing, the cycle and the wave, not in the fixed colour. PeriHub
  offers 10 to 100 %, and nobody has checked yet that the keyboard does anything with a value other than 100.
- **Wave direction** (byte 13): `01` left to right, `06` right to left, `03` from the centre to the edges, `08` from the edges
  to the centre (OpenRGB and gdevd). g810-led adds `02`, a vertical wave, with no second source.
- The wave puts the time's high byte in both byte 11 and byte 15 and its low byte in byte 12: OpenRGB and g810-led do so,
  gdevd leaves byte 11 at 0 (the two agree for times under 256 ms, which PeriHub does not send).
- **Time**: OpenRGB offers 1000 to 20000 ms, G213Colors accepts 32 to 65535. PeriHub offers 1000 to 20000, in steps of 100.
- After every command the answer (20 bytes, the same feature and function) is read. G213Colors found the keyboard
  does not carry a command out completely if its answer is not read. PeriHub reads it for each command, within a second.
- No source sends a command to "commit": g810-led says the G213 is not transactional. Nothing PeriHub sends is stored:
  G213Colors and gdevd both say the keyboard forgets what was sent when it is unplugged or restarted (gdevd sends it again
  after a restart, a suspend and a re-plug), and that it starts with a colour cycle of its own.
- The keyboard cannot report what it shows: none of the sources reads a lighting state. PeriHub therefore cannot read back,
  check or undo a lighting change. It offers each as a separate setting that is only sent, with a warning on the page. To go
  back to the keyboard's own lighting, unplug it and plug it in again.

## Not sent

- **The startup effect** (whether the keyboard starts with its colour cycle). gdevd sends `11 ff 0c 5d 00 01 vv` with `vv`
  1 to turn the animation on and 2 to turn it off; g810-led sends `11 ff 0d 5a 00 01 vv` to all its keyboards of this family,
  including the G213. The two disagree on the feature index and the function's software id, and on the G810 family that
  g810-led shares the code with the lighting sits at `0d`, so gdevd's command is the likelier one for the G213. This setting
  is stored in the keyboard (the colours are not), and no source reads it, so PeriHub could neither check nor undo a
  change: it stays out until a test report shows which feature index the keyboard has for the lighting, and someone has
  tried the command on a real G213. (Every other PeriHub change that is stored in a device is read back and undone.)
- **The "stored" byte of g810-led's effect report** (byte 16: 0, or 1 for "user-stored lighting"): not sent, for the
  same reason.
- **Per-key colours, G-keys, macros and the game mode**: the G213 has none of the first two, g810-led's G213 code has no
  per-key commands, and no source documents the others.
- Nothing in the feature table is written: feature index and function pairs other than the list above are not in the
  driver's list of commands, and `Io` refuses them.

## What the page offers

The keyboard's page (read-only data and settings that are only sent) has, in this order: a colour for the whole keyboard; a
colour for each of the five areas; "Breathing, in this colour"; an effect (off, colour cycle, five waves); the time of one
cycle; the brightness. A colour for the whole keyboard or an area, and "Off", end any effect that moves: the time and the
brightness are then refused until an effect that moves is sent again, since they belong to it. Sending the time or the
brightness sends the effect that moved last again with the new value. Each setting is checked in PeriHub before anything is
sent (range and steps), by the core as well as by the page. In read-only mode, or on a connection that may not change
anything, none is sent.

## What the first test report is meant to answer

The test report carries every request and answer. It should show: that the keyboard answers the version
request on this collection, at which index it lists `8070` and the others, what it answers to each lighting command (an
error code would show in the answer), and, from the person who tried it, what each effect did and whether the brightness
and the time had any effect.
