# Logitech keyboards: lighting through HID++ 2.0 features

What PeriHub reads of the lighting of a Logitech HID++ 2.0 device it knows only by its features (see
`UnsupportedDevice` in `crates/perihub-core/src/hidpp/receiver.rs`). The first one is the **G915 X LS**
keyboard behind a LIGHTSPEED receiver c547 ([issue #30](https://github.com/hex-dump/perihub/issues/30)):
firmware MPK 26.00.0012, HID++ 4.2, with features 8071 v4, 8081 v0, 8040 v0, 8101 v0, 4523 v1, 4540 v1,
1b05 v1, 1b10 v0 and 8051 v0. The G515 LS TKL has the same set.

**Sources.** Facts only, from [Solaar](https://github.com/pwr-Solaar/Solaar) (GPL-2.0, commit e7304c4c,
`lib/logitech_receiver/`: `settings_templates.py`, `hidpp20.py`, `rgb_power.py`, `notifications.py`, the
G515 LS TKL dump in `docs/devices/`) and [libratbag](https://github.com/libratbag/libratbag) (MIT,
`src/hidpp20.c`, `src/hidpp20.h` for 0x8071). No code was copied: the Rust in
`crates/perihub-core/src/hidpp/lighting.rs` was written from this page. Several layouts come from one source
only, or from comments; they are marked **unconfirmed** and the test report carries every reply as it came,
so that a tester's report confirms or corrects them.

Functions are numbered as in the request (`function << 4 | software id` in byte 3). "Read" means the call
changes nothing on the device, not even in RAM. Calls not listed as read are never sent: `READ_ONLY_CALLS`
and `READ_ONLY_GETS` in `crates/perihub-core/src/hidpp/mod.rs` refuse them on a read-only device.

## 0x8040 brightness

| Function | Parameters | Reply | Kind |
|---|---|---|---|
| 0 info | none | bytes 0–1 maximum (big-endian); byte 2 low 4 bits: levels of the brightness key; byte 3 bit 0x04: the lights can be turned off apart from the brightness; bytes 4–5 minimum | read |
| 1 brightness | none | bytes 0–1 brightness now | read |
| 2 set brightness | bytes 0–1 | – | change (kept or not: unknown) |
| 3 on/off | none | byte 0 bit 0: on | read |
| 4 set on/off | byte 0: 1 on, 0 off | – | change |

Solaar steps the brightness by halves (maximum, half, quarter… and 0), like the key on the G515. The device
sends event 0 with the new brightness when the key is pressed, and event 1 when the lights go on or off.

## 0x8071 RGB effects

Functions 3, 5 and 7 read or change by their first parameter: **0 reads**, 1 changes.

| Function | Parameters | Reply | Kind |
|---|---|---|---|
| 0 info, device | `ff ff 00` | byte 2 number of zones; the bytes after it are read differently by Solaar (capabilities in 5–6) and libratbag (in 3–4 and 4–5): **unconfirmed** | read |
| 0 info, zone | `zone ff 00` | bytes 2–3 location (1 primary, 2 logo, 3 left side, 4 right side, 5 combined, 6–11 primary 1–6), byte 4 number of effects | read |
| 0 info, effect | `zone effect 00` | bytes 2–3 effect id, 4–5 what it takes (0x0001 colour, 0x0002 fade, 0x0004 period, 0x0010 direction), 6–7 default period in ms | read |
| 1 set effect | zone, effect **index** (not id), 10 bytes of settings, 1 trailing byte | – | change (whether the trailing 1 keeps it: unknown) |
| 2, 4, 6 | – | – | never sent |
| 3 startup/shutdown effect | `00` then the effect: `00 01` startup, `00 40` shutdown | bytes 0–2 repeat the request, byte 3 1 on / 2 off, bytes 4–6 and 7–9 two colours (**unconfirmed**) | read with 0; with 1 **written to the device's memory** |
| 5 who drives the lights | `00` | byte 1: 0 the device, 3 a program | read with 0; with 1 a program takes the lights over (below) |
| 7 light timers | `00` | bytes 3–4 seconds before the lights dim, 5–6 before they turn off (**unconfirmed**) | read with 0; with 1 probably kept in the device's memory |
| 8, 9 power mode | – | – | never sent: 9 plays the shutdown effect and takes the device offline |
| 14 | – | error 7 (no such function) on 8071 v0 and v4 | never sent |

There is **no function that reads the effect in use**: what the keys show now cannot be read back.

Effect ids as Solaar names them: 0 off, 1 fixed colour, 2 pulse, 3 cycle, 4 wave, 8 startup, 9 demo,
0x0a breathing, 0x0b ripple, 0x0e decomposition, 0x0f signature 1, 0x10 signature 2, 0x15 cycle, 0x16 wave,
0x17 ripple.

### Taking the lights over (not done by PeriHub)

To set effects or per-key colours, a program takes the lights over: 0x8101 function 6 with `05`, then 0x8071
function 5 with `01 03 04`. That **turns off the device's own power management**: the keyboard then only
tells the program when it is idle (0x8071 event 1, byte 0 = 0) or in use again, and the program must dim and
turn off the lights itself (Solaar: function 5 `01 03 02` when idle, function 8 `01 03 00` to sleep,
`01 01 00` to wake), and hand them back on exit (function 5 `01 00 00`, then 0x8101 function 6 `03`). A
program that stops without handing them back leaves the keyboard lit, on battery. PeriHub does none of this
yet: it is a decision for after the first test reports.

0x0020 (configuration change) function 1 with the value `00 00` hands the lights back on the G515, Solaar
notes. PeriHub never calls 0x0020.

## 0x8081 per-key lighting

| Function | Parameters | Reply | Kind |
|---|---|---|---|
| 0 keys | `00 page`, pages 0, 1, 2 | bytes 2–15: a bitmap, bit *i* (low bit first) over the pages one after the other = key id *i* has its own colour (page layout **unconfirmed**) | read |
| 1, 5, 6 set colours | up to 4 `key r g b`; a range; one colour and up to 13 keys | – | change (RAM, shown at function 7) |
| 7 end of frame | | – | change |

There is **no function that reads the colours back**. Key ids are keyboard usage codes; the G515 lists some
that are no keys (47 and 97 on ANSI, 99–103, 254).

## 0x4540 layout

Function 0, no parameters: byte 0 is the layout. Solaar reads 1 as ANSI; 2, 5, 8, 0x0b, 0x0d–0x0f, 0x16, 0x1d,
0x21, 0x24 as ISO; 3 and 7 as ISO QWERTZ; 4 as ISO AZERTY; 9 and 0x3e as JIS. Read. **But the German (QWERTZ)
G915 X LS of issue #30 answers 4**: PeriHub names only the codes seen on a real keyboard.

## What a G915 X LS answered (issue #30, 2026-11-15, Windows, through its c547 receiver)

| Call | Reply (trailing zeros left out) | Reading |
|---|---|---|
| 8040 0 | `00 64 05 03` | maximum 100, 5 levels, byte 3 = 0x03: no separate on/off (bit 0x04 clear) |
| 8040 1 | 50 with G HUB running, 100 after it was closed | follows the brightness key |
| 8071 0 `ff ff 00` | `ff 00 02 00 63 00 04 08` | 2 zones |
| 8071 0 `00 ff 00` | `00 00 00 01 06` | zone 0: location 1 (keys), 6 effects |
| 8071 0 `01 ff 00` | `01 00 00 02 04` | zone 1: location 2 (logo), 4 effects |
| 8071 0 `00 ee 00` | ids 0x00, 0x01, 0x0a, 0x15, 0x16, 0x17 | off, fixed colour, breathing, cycle, wave, ripple |
| 8071 0 `01 ee 00` | ids 0x00, 0x01, 0x0a, 0x15 | off, fixed colour, breathing, cycle |
| 8071 3 `00 00 01` | `00 00 01 02 ff 00 81 80 aa ff` | startup effect off (G HUB: unchecked), colours #ff0081 #80aaff |
| 8071 3 `00 00 40` | `00 00 40 01 ff 00 81 80 aa ff` | shutdown effect on |
| 8071 5 `00` | 3 with G HUB running, 0 after it was closed | who drives the lights |
| 8071 7 `00` | `00 00 00 00 3c 01 0e` | dims after 60 s (G HUB: 1 minute), off after 270 s: G HUB writes `01 00 00 00 3c 01 0e` for its "5 minutes" (a G HUB capture on a G915 X TKL, in logi-ghub-linux's notes) |
| 8081 0 pages 0–2 | page 0 all `ff`; page 1 `… 00 00 00 00 00 7a 00 00 f0 1f 00 00 04`; page 2 empty | 126 keys: 1–111, 153, 155–158, 180–188 (G1–G9), 210 (logo) |
| 4540 0 | `04` | German ISO |

The bytes after the effect id (said to be capabilities and default period) do not look like what Solaar and
libratbag describe (`c0 01 1e 0b` for breathing, `dc e1 00 1e` for wave): **their meaning is unknown**, and
PeriHub does not use them. With G HUB closed the keys went back to the keyboard's own effect (a fixed colour)
at the brightness set before.

## Not read: 0x8101, 0x4523, 0x8051, 0x1b05, 0x1b10

No public source describes their functions. 0x8101 (profile management, in place of 0x8100 on these
keyboards) is only known for function 6 above. 0x4523 is named "disable controls" (game mode), 0x1b05 "full
key customization", 0x1b10 "control list", 0x8051 "Logitech modifiers". PeriHub calls none of them.
