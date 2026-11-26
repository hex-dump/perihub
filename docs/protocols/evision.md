# EVision keyboards (320f and 0c45 ids, vendor collection `ff1c`)

Keyboards on an EVision controller, sold under many brands (Redragon, Glorious, Ajazz, Tecware, Cosmic Byte, Surmen…).
Their lighting, key assignments and macros are configured through one vendor HID collection, usage page `ff1c`, on
USB interface 1. **On Linux PeriHub does not open them** (see "Linux" below); on Windows the collection is a device
of its own. PeriHub reads every listed id, and on two ids it changes, as an experimental feature, everything the
sources document: the lighting header, the colours per key and the key table.

## Sources, in PeriHub's own words

Both are GPL-2.0, so only facts were taken (commands, byte layouts, ids, safety notes) and written here; the Rust
was written from this page alone, with neither project open. No code was copied.

- [OpenRGB](https://gitlab.com/CalcProgrammer1/OpenRGB) (GPL-2.0-or-later, commit 91714d1, 18 November 2026), its
  EVision controllers (`Controllers/EVisionKeyboardController`, and the `EVisionV2KeyboardController` folder next to
  it): the packet format, the commands, the lighting header, the id lists and the interface and usage page. Its
  older controller ("V1") never reads anything back; its newer one ("V2") does. Looked at again at commit 6b681e3
  (20 November 2026) for the colours per key of the older controller: the packet size, the 126 LEDs and their map
  (`RGBController_EVisionKeyboard.cpp`), and the header parameters it names but never sends (polling rate, Surmount's
  colour).
- [OpenEVision](https://github.com/iefedev/OpenEVision) (GPL-2.0-only, commit 1cca304, 3 November 2026), a Windows app
  for **`320f:505b`**, tried on a full-size ISO keyboard with 126 LEDs. It uses OpenRGB's command set and says what it
  checked on that keyboard (`docs/PROTOCOL.md`): the reads below answer, the layout of the configuration memory, the
  key table, and which regions it never writes. How it reads and writes the colours per key and the key table, what it
  lets a key be set to, and which key each LED is on its keyboard, are in its `app/Device` folder
  (`KeyboardService.cs`, `HidKeyboard.cs`, `KeyActions.cs`, `DeviceProfile.cs`).

Looked at and **not used**:

- ak820-rs (MIT), phantom-tkl-driver (GPL-3.0) and OpenKey (MIT) take their facts for `320f:505b` from an analysis
  or decompilation of the keyboard makers' own Windows software. PeriHub does not use facts that come from reading
  or decompiling the official software, even when someone else did it.
- DotJazz (MIT) lists `320f:505b` as an Ajazz AK820 whose lighting it has **not** tried (the keyboard is "Direct",
  everything else "not captured"), with effect codes that differ from OpenRGB's, and vendors a layout file from the
  maker's software. Nothing was taken from it.
- No project, and no maker's publication, documents macros or onboard profiles of these keyboards.

The Surmen V400 (issue #80) has no public documentation. `320f:505b` is one id shared by keyboards of several brands
and layouts (the OpenEVision keyboard is a full-size ISO, the AJAZZ AK820 and the Cosmic Byte Phantom TKL report it
too): the layout and the number of LEDs are not the same on all of them, and the DotJazz and ak820-rs projects show
that other firmware on that id uses other usage pages. PeriHub opens only the collection with usage page `ff1c`.

## Transport

- USB interface 1, usage page `ff1c` (usage `92` on the Surmen V400, from its device request), report number `04`.
  Output and input reports are 64 bytes **including** the report number: 63 bytes after it.
- A request is sent as an output report, and the keyboard answers with an input report of the same size.
  Every program that has the collection open receives every answer: a request must flush what is waiting first and
  accept only the answer whose command and offset match its own.

| Byte | Request | Answer |
|---|---|---|
| 0 | `04` | `04` |
| 1–2 | checksum: the sum of bytes 3 to 63, 16 bits, low byte first | the same checksum as the request (not one of the answer's own bytes) |
| 3 | command | the command |
| 4 | number of data bytes (at most 56) | number of data bytes |
| 5–6 | offset, low byte first | the offset |
| 7 | `00` | status: `00` is success, any other value is an error |
| 8– | data, zeros after it | data |

`04 01 00 01` and `04 02 00 02` (rest zeros) are complete "begin" and "end" requests: with nothing after the command,
the checksum is the command itself.

## Commands

| Command | Meaning | Source | Kind |
|---|---|---|---|
| `01` | begin a configuration change | OpenRGB V2; OpenEVision | change |
| `02` | end a configuration change | OpenRGB V2; OpenEVision | change |
| `03` | read the capabilities: 7 bytes, `aa 55`, then 3 bytes not described, then the size of the key map (byte 5), then the macro memory in blocks of 128 bytes (byte 6) | OpenRGB V2 only | read |
| `05` | read the configuration memory at an offset | both; OpenEVision on `320f:505b` | read |
| `06` | write the configuration memory at an offset | both; OpenEVision on `320f:505b` | **change, stays in flash** |
| `10` | read the custom (per-key) colours, at an offset, up to 56 bytes a request | OpenEVision on `320f:505b`; named but not used by OpenRGB V1 | read |
| `11` | write the custom colours at an offset: 54 bytes a request in OpenRGB V1, up to 56 in OpenEVision | both | change, stays in flash |
| `0a` / `0b` | read / write the custom colours on the V2 controller (512 bytes per colour set, 10 sets) | OpenRGB V2 | read / change |
| `12` / `13` | send live colours (RAM only, dropped after a short idle time, so re-sent every 150 ms) / end live mode | OpenRGB V2; OpenEVision | change, not stored |

A read of more than 56 bytes is several requests, each with the offset moved on by the bytes that came back.

## Where the settings are

There are two layouts of the configuration memory, told apart by the controller, not by anything the keyboard says:

**Layout 1** (OpenRGB's older controller, `320f:505b` checked by OpenEVision, and `320f:5000` read on a keyboard, see
"Seen on a keyboard" below): the lighting header is at offset 0,
8 bytes, and covers offsets `00`–`bf` (the rest of that range is not described). The key table follows at `c0`.

**Layout 2** (OpenRGB's newer controller): offset 0 is one byte, the profile in use (0 to 2; OpenRGB treats a larger
value as 0). Each profile has 64 bytes, starting at `01`, `41` and `81`; the lighting header is the first 8 bytes of
the profile, followed by parameters for a logo light and an edge light.

The lighting header (8 bytes, both layouts):

| Byte | Meaning |
|---|---|
| 0 | mode, a number from the list below |
| 1 | brightness, 0 (off) to 4 |
| 2 | speed, 0 (fastest) to 5 (slowest); 3 is the normal setting |
| 3 | direction, a number: OpenRGB V1 writes its own direction numbers, 0 left and 1 right for the waves and the colour wheel, 2 up and 3 down for the vertical rainbow; OpenEVision writes 0 to 3 ("Direction 1" to "4") and says their meaning depends on the mode and is not documented |
| 4 | random colour: 0 or 1 when written on layout 1 (OpenRGB V1 and OpenEVision); OpenRGB's V2 writes 255 |
| 5–7 | colour, R G B (used by the modes that have one) |

Modes: 1 wave (short), 2 wave (long), 3 colour wheel, 4 spectrum cycle, 5 breathing, 6 static, 7 reactive,
8 reactive ripple, 9 reactive line, 10 starlight (fast), 11 blooming, 12 vertical rainbow wave, 13 hurricane,
14 accumulate, 15 starlight (slow), 16 visor, 17 surmount, 18 circular rainbow wave, 20 custom (per-key colours).
OpenRGB's older controller offers all of them except 15 and 17; OpenEVision offers all of them on `320f:505b`, 15 and
17 included, and writes them like the others. "Surmount" takes its colour from a separate parameter instead of the RGB
bytes: OpenRGB names it at offset `11` with the values 1 to 4 (red, yellow, green, blue) for the older controller,
but never sends it there (its older controller does not offer Surmount); its newer controller writes 0 to 3 (red,
yellow, green, cyan) at offset `11` of the profile. OpenEVision does not write it. OpenRGB also names offset `0f` the
polling rate (0 to 3: 125, 250, 500, 1000 Hz) and never sends it; OpenEVision found that it does not match the USB
descriptor (see below). Which modes use the colour, the
speed or the direction differs: spectrum cycle, circular rainbow wave and blooming take no colour; visor, static and circular rainbow wave have no speed.

Every OpenRGB mode of both controllers is flagged "saved automatically": a header written with command `06` stays
in the keyboard's memory. OpenEVision confirms it on `320f:505b` ("saved to the keyboard, so they keep running
without the app").

**Key table** (layout 1, offset `c0`, 128 entries of 3 bytes, OpenEVision): `02 02 <usage>` a key (its HID usage;
usage 0 disables the key), `02 01 <bit>` a modifier (its bit in the modifier byte: `01` left Ctrl, `02` left Shift,
`04` left Alt, `08` left Win, `10` to `80` the right ones), `03 <lo> <hi>` a consumer (media) usage, `05 …` the Fn
key. The entries are in the order the keyboard's own live colours use; which entry is which physical key is only
known from the keyboard's factory table (OpenEVision keeps a backup of it, taken the first time it reads the keyboard,
before any write, and names each entry by what it held then), so a key cannot be named from the table alone once it
is changed. Two more copies of the table (at `2c0` and `4c0`) have an unknown purpose. An entry of any other shape is not
described by any source: the test report prints the bytes of those, grouped, to tell what they are.

How OpenEVision changes the table: one write of 3 bytes per changed entry, command `06` at `c0 + 3 × entry`, all of
them between one begin and one end, in the order of the entries; entries that do not change are not sent. "Writing
an entry remaps that key; remaps are stored in flash and survive unplugging." What it lets an entry be set to: a key
(usages `04` to `65` of the keyboard page, without `31`, which its ISO keyboard has not), one of the eight modifier
bits, one of twelve media usages (`e9` volume up, `ea` volume down, `e2` mute, `cd` play/pause, `b5` next, `b6`
previous, `b7` stop, `183` media player, `192` calculator, `196` browser, `18a` e-mail, `194` my computer), or
disabled (`02 02 00`). The Fn key is never changed ("Fn never reaches the PC … and is not remappable"), and nothing
writes the copies.

**Custom colours** (command `10` to read, `11` to write): 3 bytes per LED, LED `row * 21 + column` (126 LEDs, 378
bytes), the same matrix in OpenRGB's older controller and on the keyboard OpenEVision tried. OpenEVision reads them in
requests of up to 56 bytes from offset 0 (and `0x200` bytes for a backup); a write "shows immediately but takes about
22 ms per packet (flash)". OpenRGB's older controller writes all 378 bytes from offset 0 in seven requests of 54
bytes (18 LEDs each), with no begin and end; OpenEVision writes them in requests of up to 56 bytes between a begin and
an end, and to show them, writes the mode byte (offset 0) with 20 in the same bracket.

Which key each LED is, from OpenRGB's map of the older controller (a full-size ANSI board) and OpenEVision's map of
its full-size ISO `320f:505b`. The two agree; the three positions only one of them has are marked. Columns count
from 0; a column not listed has no key in either map.

| Row (LEDs) | Keys by column |
|---|---|
| 0 (0–20) | 0 Esc, 1–12 F1–F12, 14 Print Screen, 15 Scroll Lock, 16 Pause |
| 1 (21–41) | 0 \`, 1–10 the digits 1 to 0, 11 -, 12 =, 13 Backspace, 14 Insert, 15 Home, 16 Page Up, 17 Num Lock, 18 keypad /, 19 keypad *, 20 keypad - |
| 2 (42–62) | 0 Tab, 1–10 Q to P, 11 [, 12 ], 13 the ANSI backslash (OpenRGB only), 14 Delete, 15 End, 16 Page Down, 17–19 keypad 7 8 9, 20 keypad + |
| 3 (63–83) | 0 Caps Lock, 1–9 A to L, 10 ;, 11 ', 12 the ISO key left of Enter (OpenEVision only), 13 Enter, 17–19 keypad 4 5 6 |
| 4 (84–104) | 0 left Shift, 1 the ISO key right of it (OpenEVision only), 2–11 Z to /, 13 right Shift, 15 Up, 17–19 keypad 1 2 3, 20 keypad Enter |
| 5 (105–125) | 0 left Ctrl, 1 left Win, 2 left Alt, 3 Space, 4 right Alt, 5 Fn, 6 Menu, 8 right Ctrl, 14 Left, 15 Down, 16 Right, 18 keypad 0, 19 keypad . |

Neither map has keys a keyboard adds to a full-size board (a K580's five macro keys and its media keys): those LEDs,
if the keyboard has them, are among the positions without a name.

Other facts OpenEVision found on `320f:505b`: offset `0f`, which OpenRGB calls the polling rate, reads 0 although
the USB descriptor says 1 ms (so it is not trusted); the memory from `700` on changes on its own; the firmware
version is the USB device version (`bcdDevice`), not something the HID commands return (for example `0104`).

## What the sources say about danger

- OpenRGB has no warning about these keyboards. It sends no "begin"/"end" with its older controller, and sends every
  lighting write, the colours per key included, straight to the flash (`automatic save`).
- OpenEVision's README warns that it **writes to the keyboard's flash**, that the protocol is undocumented, that "a
  wrong write could leave the keyboard misconfigured or, in the worst case, unusable", and that it uses the keyboard at
  your own risk. It therefore backs up the keyboard's original configuration first and refuses any write outside the
  lighting header, the key table, and the custom colours (and the live colours): it never writes offsets above
  `ff` except the key table, never the table copies, never offset `0f` or `700` and above, and no custom colour past
  the 126 LEDs. Flash writes of colours take about 22 ms per packet.
- Nothing in either source reads or writes the firmware, a bootloader or a factory reset, and PeriHub never sends
  any of them. No command in the table above is one.

## Which keyboards

From OpenRGB's detection lists, interface 1 and usage page `ff1c` for each:

- Older controller (layout 1), vendor `320f`: `5000`, `502a`, `505b`, `5064`, `5078`, `5084`; vendor `0c45` (Sonix, whose
  controller EVision rebranded): `5004`, `5104`, `5204`, `652f`, `7672`, `7698`, `8520`. OpenRGB names none of them by
  product. Only `320f:505b` (by OpenEVision) and `320f:5000` (see below) have been seen answering the reads.
- Newer controller (layout 2): `320f:5041` (OpenRGB's source calls it Bygg CSB-ICL01) and `3299:0012` (the Endorfy
  Omnis, another maker's id).

## What PeriHub does

- Reads, on every listed id and with no write command in its list (read-only): the lighting header (layout 1 at `00`;
  on `320f:5041` the profile in use at `00` and then the profile's header, after the capabilities read `03`, which
  must answer `aa 55`), and on layout 1 the key table, once, for the test report. A header that is out of range
  (mode not in the list, brightness over 4, speed over 5) makes the page read-only with the reason.
- On `320f:505b` and `320f:5000` only, at every reading: the header, the key table (seven reads from `c0`) and the
  colours per key (seven reads of command `10` from 0, 378 bytes). A keyboard that does not answer the colours, or
  refuses them, is not asked for them again while it stays connected, and the page says why.
- On those two ids, as an **experimental** feature, PeriHub changes:
  - the **lighting**: mode, brightness, colour, speed, and the direction (0 to 3) and random colour flag (0 or 1) when
    they read in those ranges (otherwise they are written back as read). Every mode of the older controller's list;
    Custom (20) once the colours per key were read; Starlight (slow) and Surmount only on `320f:505b`, where
    OpenEVision sets them. All 8 bytes of the header go to offset 0;
  - the **colours per key**, LED by LED or all at once: only the 54-byte packets that hold a changed LED, at offsets
    0, 54 … 324, so no LED is split over two requests and nothing is written past the 126th LED;
  - the **key table**: an entry that holds a key, a modifier or a media usage can be set to any of what OpenEVision
    offers, plus the ANSI backslash (`31`), F13 to F24 (`68` to `73`: standard keyboard-page usages, encoded `02 02 NN` like the others; OpenEVision does not offer them, PeriHub's own choice) and the international keys (`87` to `8b`, `90`, `91`) that the K580's
    own table holds: one write of 3 bytes at the entry's offset per changed entry. Each entry is named by what it
    held the first time PeriHub read the keyboard, before any change: PeriHub keeps that table in the keyboard's
    backup folder (`first-reading.bin`). The Fn entries, the entries no source describes (`02 00 00`, `d0 a2 NN` on
    the K580) and the copies at `2c0` and `4c0` are never written. If the table read later does not fit the one
    kept (an entry that is not a key, a modifier or a media usage has other bytes than then, as on another keyboard
    with the same id), remapping is not offered and the page says which entry differs.

  The colours go first, then the key entries, then the header, so that a keyboard switched to Custom shows its new
  colours; all of it between one begin and one end, and the end is sent also after a refused write. The command list
  holds only these writes: 8 bytes at offset 0, 3 bytes at each entry's own offset, 54 bytes at each packet's own
  offset. Every change stays in the keyboard's flash, so it goes through PeriHub's No warranty box, a backup of the
  settings (header, colours, table), the whole reading again after the change (a change that does not read back is
  undone at once) and the 15-second undo. Modes that are not offered are shown when the keyboard is in them and can
  be put back.
- The test report lists the changes sent with the keyboard's answer to each, and the entries that differ from the
  table PeriHub first read.
- Never sent: the live colours (`12`/`13`, RAM only: the colours per key are written as they are kept, with no
  preview), offsets `0f` and `11`, any other offset, and anything on the other ids.

## Seen on a keyboard

A Redragon K580 RGB, `320f:5000`, on Windows with PeriHub 0.9.7 (issue #117, read only): it answers the reads as
documented. Header `14 04 00 00 00 …`: mode 20 (custom, per key), brightness 4, colour 0 (so the page shows #000000).
The key table is 128 entries read in seven requests from `c0` (the answers are in
`crates/perihub-core/tests/fixtures/transcripts/evision-k580.json`). Entries seen, by shape:

- 94 keys (`02 02 <usage>`), 7 modifiers (`02 01 <bit>`), 3 media (`03 <lo> <hi>`: b6, cd, b5), 3 Fn (`05 02 02`,
  `05 1b 00`, `05 1d 00`: the Fn key is not always `05 00 00`).
- 16 entries `02 00 00`, one at the end of every group of 8 entries, and 5 entries `d0 a2 01` to `d0 a2 05`, the
  seventh of five consecutive groups. **No source documents either shape.** They are only an observation: the first
  looks like an empty position at the end of each group, the second like five numbered entries, and the K580 has five macro keys, but nobody has checked this, so PeriHub does not name them.
- The firmware version is not in the answers; OpenEVision says it is the USB device version (`bcdDevice`), which is
  not a command, so PeriHub does not read it from the keyboard.

Why `320f:5000` may be changed: OpenRGB's older controller lists it next to `505b` and sends both the
same writes (the header and the colours per key), and this keyboard answered as the documented layout 1, its key
table in the documented forms. The other ids of OpenRGB's
older controller are listed the same way, but no report has shown one yet, so those stay read only.

## Left out: waiting for captures

What the official software changes and no source documents, so PeriHub does not invent it (captures of the official
software from a tester's keyboard, see `docs/captures.md`):

- **Macros and the macro keys** (the K580's G1 to G5): no source describes the macro memory (only its size is in the
  newer controller's capabilities read) nor what the `d0 a2 01` to `d0 a2 05` entries of the K580's key table do.
- **Onboard profiles** on layout 1: no source.
- **Fn layers** (what a key does with Fn held): no source; the Fn entries themselves are never written.
- **Polling rate** (offset `0f`): OpenRGB names its values but never sends it, and OpenEVision found that it reads 0
  while the keyboard polls at 1 ms.
- **Surmount's colour** (offset `11`): no source writes it on layout 1, and the two controllers give it other values.
- **Which LED is which key on the K580** and on the other keyboards with these ids: the colours per key are changed by
  position, with the names of OpenRGB's and OpenEVision's maps, which are not checked on them.
- The entries no source describes (`02 00 00` at the end of each group of 8, `d0 a2 NN`) and the table copies.
- Anything the Surmen or Redragon software offers beyond this, and every change on the other ids, which wait for a
  test report each.

## Linux

Interface 1 holds, besides the vendor collection, a keyboard (`0001/0006`), a mouse (`0001/0002`), a system-control
(`0001/0080`) and a consumer-control (`000c/0001`) collection, as the Surmen V400's device request shows. On Linux there
is one hidraw node per **interface**, not per collection, so opening the node for the vendor collection also gives
access to what is typed. A udev rule that opened it would let any program running as the user read the typing, so, as
for most Razer keyboards, **the udev rule does not open these keyboards, and PeriHub does not open them on Linux**.
**Send a report** lists them with their interfaces. On Windows the system keeps typed keys from programs and the
vendor collection has a path of its own.
