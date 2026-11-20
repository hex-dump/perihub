# EVision keyboards (320f and 0c45 ids, vendor collection `ff1c`)

Keyboards on an EVision controller, sold under many brands (Redragon, Glorious, Ajazz, Tecware, Cosmic Byte, Surmen…).
Their lighting, key assignments and macros are configured through one vendor HID collection, usage page `ff1c`, on
USB interface 1. **On Linux PeriHub does not open them** (see "Linux" below); on Windows the collection is a device
of its own. For now PeriHub reads, and on one id it can change the lighting, as an experimental feature.

## Sources, in PeriHub's own words

Both are GPL-2.0, so only facts were taken (commands, byte layouts, ids, safety notes) and written here; the Rust
was written from this page alone, with neither project open. No code was copied.

- [OpenRGB](https://gitlab.com/CalcProgrammer1/OpenRGB) (GPL-2.0-or-later, commit 91714d1, 18 November 2026), its
  EVision controllers (`Controllers/EVisionKeyboardController`, and the `EVisionV2KeyboardController` folder next to
  it): the packet format, the commands, the lighting header, the id lists and the interface and usage page. Its
  older controller ("V1") never reads anything back; its newer one ("V2") does.
- [OpenEVision](https://github.com/iefedev/OpenEVision) (GPL-2.0-only, commit 1cca304, 3 November 2026), a Windows app
  for **`320f:505b`**, tried on a full-size ISO keyboard with 126 LEDs. It uses OpenRGB's command set and says what it
  checked on that keyboard (`docs/PROTOCOL.md`): the reads below answer, the layout of the configuration memory, the
  key table, and which regions it never writes.

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
| `10` | read the custom (per-key) colours | OpenEVision on `320f:505b`; named but not used by OpenRGB V1 | read |
| `11` | write the custom colours (up to 54 bytes at a time in OpenRGB V1) | both | change, stays in flash |
| `0a` / `0b` | read / write the custom colours on the V2 controller (512 bytes per colour set, 10 sets) | OpenRGB V2 | read / change |
| `12` / `13` | send live colours (RAM only, dropped after a short idle time, so re-sent every 150 ms) / end live mode | OpenRGB V2; OpenEVision | change, not stored |

A read of more than 56 bytes is several requests, each with the offset moved on by the bytes that came back.

## Where the settings are

There are two layouts of the configuration memory, told apart by the controller, not by anything the keyboard says:

**Layout 1** (OpenRGB's older controller, and `320f:505b`, checked by OpenEVision): the lighting header is at offset 0,
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
| 3 | direction, a number: OpenRGB uses it as left/right or up/down depending on the mode; OpenEVision says its meaning depends on the mode and is not documented |
| 4 | random colour: 0 or 1 when written on layout 1 (OpenRGB V1 and OpenEVision); OpenRGB's V2 writes 255 |
| 5–7 | colour, R G B (used by the modes that have one) |

Modes: 1 wave (short), 2 wave (long), 3 colour wheel, 4 spectrum cycle, 5 breathing, 6 static, 7 reactive,
8 reactive ripple, 9 reactive line, 10 starlight (fast), 11 blooming, 12 vertical rainbow wave, 13 hurricane,
14 accumulate, 15 starlight (slow), 16 visor, 17 surmount, 18 circular rainbow wave, 20 custom (per-key colours).
OpenRGB's older controller offers all of them except 15 and 17. "Surmount" takes its colour from a separate
parameter (offset `11`, 1 to 4 on the older controller) instead of the RGB bytes. Which modes use the colour, the
speed or the direction differs: spectrum cycle, circular rainbow wave and blooming take no colour; visor, static and circular rainbow wave have no speed.

Every OpenRGB mode of both controllers is flagged "saved automatically": a header written with command `06` stays
in the keyboard's memory. OpenEVision confirms it on `320f:505b` ("saved to the keyboard, so they keep running
without the app").

**Key table** (layout 1, offset `c0`, 128 entries of 3 bytes, OpenEVision): `02 02 <usage>` a key (its HID usage;
usage 0 disables the key), `02 01 <bit>` a modifier (its bit in the modifier byte), `03 <lo> <hi>` a consumer
(media) usage, `05 …` the Fn key. The entries are in the order the keyboard's own live colours use; which entry is
which physical key is only known from the keyboard's factory table (OpenEVision keeps a backup of it), so PeriHub
cannot name a key from the table alone. Two more copies of the table (at `2c0` and `4c0`) have an unknown purpose.

**Custom colours** (command `10`, OpenEVision): 3 bytes per LED, `row * 21 + column` on the keyboard it tried (126
LEDs, 378 bytes).

Other facts OpenEVision found on `320f:505b`: offset `0f`, which OpenRGB calls the polling rate, reads 0 although
the USB descriptor says 1 ms (so it is not trusted); the memory from `700` on changes on its own; the firmware
version is the USB device version (`bcdDevice`), not something the HID commands return (for example `0104`).

## What the sources say about danger

- OpenRGB has no warning about these keyboards. It sends no "begin"/"end" with its older controller, and sends every
  lighting write straight to the flash (`automatic save`).
- OpenEVision's README warns that it **writes to the keyboard's flash**, that the protocol is undocumented, that "a
  wrong write could leave the keyboard misconfigured or, in the worst case, unusable", and that it uses the keyboard at
  your own risk. It therefore backs up the keyboard's original configuration first and refuses any write outside the
  lighting header, the key table, and the custom colours (and the live colours): it never writes offsets above
  `ff` except the key table, never the table copies, never offset `0f` or `700` and above. Flash writes of colours
  take about 22 ms per packet.
- Nothing in either source reads or writes the firmware, a bootloader or a factory reset, and PeriHub never sends
  any of them. No command in the table above is one.

## Which keyboards

From OpenRGB's detection lists, interface 1 and usage page `ff1c` for each:

- Older controller (layout 1), vendor `320f`: `5000`, `502a`, `505b`, `5064`, `5078`, `5084`; vendor `0c45` (Sonix, whose
  controller EVision rebranded): `5004`, `5104`, `5204`, `652f`, `7672`, `7698`, `8520`. OpenRGB names none of them by
  product. Only `320f:505b` has been seen answering the reads, by OpenEVision.
- Newer controller (layout 2): `320f:5041` (OpenRGB's source calls it Bygg CSB-ICL01) and `3299:0012` (the Endorfy
  Omnis, another maker's id).

## What PeriHub does

- Reads, on every listed id and with no write command in its list (read-only): the lighting header (layout 1 at `00`;
  on `320f:5041` the profile in use at `00` and then the profile's header, after the capabilities read `03`, which
  must answer `aa 55`), and on layout 1 the key table, once, for the test report. A header that is out of range
  (mode not in the list, brightness over 4, speed over 5) makes the page read-only with the reason.
- On `320f:505b` only, changes the lighting as an **experimental** feature: mode, brightness, colour and speed. It
  reads the header, writes all 8 bytes at offset 0 (changed values only; direction and the random flag are written
  back as read) between a begin and an end, and reads the header back: a header that does not read back is undone.
  Every change stays in the keyboard's flash, so it goes through PeriHub's No warranty box, backup, read-back
  and 15-second undo. Modes 15, 17 and 20 are shown when the keyboard is in them but not offered.
- Never sent: the custom colours, the live colours, the key table, any other offset. They wait for a tester's report
  and, for the key table, for captures that say which entry is which key.

## Left out: waiting for captures

Key remapping and macros: the key table's format is known, but only for `320f:505b` and without which entry is which
key; the macro memory's format is described by no source (only its size is in the capabilities read). Onboard
profiles on layout 1: no source. Fn layers, polling rate and anything the Surmen software offers beyond lighting:
captures of the official software from a tester's keyboard, see `docs/captures.md`.

## Linux

Interface 1 holds, besides the vendor collection, a keyboard (`0001/0006`), a mouse (`0001/0002`), a system-control
(`0001/0080`) and a consumer-control (`000c/0001`) collection, as the Surmen V400's device request shows. On Linux there
is one hidraw node per **interface**, not per collection, so opening the node for the vendor collection also gives
access to what is typed. A udev rule that opened it would let any program running as the user read the typing, so, as
for most Razer keyboards, **the udev rule does not open these keyboards, and PeriHub does not open them on Linux**.
**Send a report** lists them with their interfaces. On Windows the system keeps typed keys from programs and the
vendor collection has a path of its own.
