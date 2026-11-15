# Logitech keyboards: lighting through HID++ 2.0 features

What PeriHub reads of the lighting of a Logitech HID++ 2.0 device it knows only by its features (see
`UnsupportedDevice` in `crates/perihub-core/src/hidpp/receiver.rs`), and what it sends when it drives a
keyboard's lights in host mode (experimental, see below). The first one is the **G915 X LS**
keyboard behind a LIGHTSPEED receiver c547 ([issue #30](https://github.com/hex-dump/perihub/issues/30)):
firmware MPK 26.00.0012, HID++ 4.2, with features 8071 v4, 8081 v0, 8040 v0, 8101 v0, 4523 v1, 4540 v1,
1b05 v1, 1b10 v0 and 8051 v0. The G515 LS TKL has the same set.

**Sources.** Facts only. Logitech's own specifications of the features: "x8071 – RGB Effects" version 4,
"x8081 – Per-Key Lighting" version 0, "x8040 – Brightness Control" version 1 and the HID++ 2.0 draft of
2012-06-04 for the error codes, from Logitech's public HID++ folder on Google Drive, read from the copy kept in
[logirgb](https://github.com/mevdschee/logirgb) (MIT, commit ac77dbb, `research/drive-download.zip`).
[Solaar](https://github.com/pwr-Solaar/Solaar) (GPL-2.0, commit e7304c4c, `lib/logitech_receiver/`:
`settings_templates.py`, `hidpp20.py`, `rgb_power.py`, `notifications.py`, `special_keys.py`, and the G HUB
captures and hardware tests in its pull request 3149). [libratbag](https://github.com/libratbag/libratbag) (MIT,
`src/hidpp20.c`, `src/hidpp20.h`). [logi-ghub-linux](https://github.com/theo-antonescu/logi-ghub-linux) (GPL-2.0,
commit 021f4a2): its notes of a USB capture of G HUB on a G915 X (`docs/g915x-profile-management.md`) and its
tests on a G915 X LS TKL. No code was copied: the Rust in `crates/perihub-core/src/hidpp/lighting.rs` and
`crates/perihub-core/src/hidpp/host.rs` was written from this page. No decompiled Logitech software was used;
OpenLogi was left out because it says it was checked against G HUB by static analysis. What comes from one source
only is marked **unconfirmed**, and the test report carries every reply as it came, so that a tester's report
confirms or corrects it.

Functions are numbered as in the request (`function << 4 | software id` in byte 3). "Read" means the call
changes nothing on the device, not even in RAM. Calls not listed as read are never sent: `READ_ONLY_CALLS`
and `READ_ONLY_GETS` in `crates/perihub-core/src/hidpp/mod.rs` refuse them on a read-only device.

## 0x8040 brightness

| Function | Parameters | Reply | Kind |
|---|---|---|---|
| 0 info | none | bytes 0–1 maximum (big-endian); byte 2 low 4 bits: levels of the brightness key; byte 3 bit 0x04: the lights can be turned off apart from the brightness; bytes 4–5 minimum | read |
| 1 brightness | none | bytes 0–1 brightness now | read |
| 2 set brightness | bytes 0–1, no persistence byte | – | change: kept after a power cycle unless capability bit 0x10 ("transient") is set; the G915 X LS has it clear |
| 3 on/off | none | byte 0 bit 0: on | read |
| 4 set on/off | byte 0: 1 on, 0 off | – | change |

Solaar steps the brightness by halves (maximum, half, quarter… and 0), like the key on the G515. The device
sends event 0 with the new brightness when the key is pressed, and event 1 when the lights go on or off.

## 0x8071 RGB effects

Functions 3, 5 and 7 read or change by their first parameter: **0 reads**, 1 changes.

| Function | Parameters | Reply | Kind |
|---|---|---|---|
| 0 info, device | `ff ff 00` | byte 2 number of zones, 3–4 what the settings can keep (nvCapabilities), 5–6 extCapabilities, 7 multi-zone effects (the specification; libratbag reads 5–6 from the wrong byte) | read |
| 0 info, zone | `zone ff 00` | bytes 2–3 location (1 primary, 2 logo, 3 left side, 4 right side, 5 combined, 6–11 primary 1–6), byte 4 number of effects | read |
| 0 info, effect | `zone effect 00` | bytes 2–3 effect id, 4–5 capabilities, 6–7 the effect's period unit in ms (the specification asks colour cycles and waves to use a multiple of it) | read |
| 1 set effect | zone, effect **index** (not id), 10 bytes of settings, then byte 12: bits 0–1 persistence (0x01 RAM, 0x02 the device's memory, 0x03 both), bits 2–3 power mode (0 full, 1 power save) | – | change; refused (error 5) unless a program has the lights (function 5). PeriHub sends 0x01 only |
| 2, 4, 6 | – | – | never sent |
| 3 startup/shutdown effect | `00` then the effect: `00 01` startup, `00 40` shutdown | bytes 0–2 repeat the request, byte 3 1 on / 2 off, bytes 4–6 and 7–9 two colours | read with 0; with 1 **written to the device's memory** |
| 5 who drives the lights | `00` | byte 1: 0 the device, 3 a program | read with 0; with 1 a program takes the lights over (below) |
| 7 light timers | `00` | bytes 1–2 flags, 3–4 seconds without use before the lights dim, 5–6 seconds from dimming to off; 0xffff never | read with 0; with 1 kept in the device's memory, never sent |
| 8 power mode | `01`, then 1 full, 2 power save, 3 off | – | change; refused unless a program has the power modes (function 5, bit 0x02) |
| 9 shutdown | – | – | never sent: it plays the shutdown effect and takes the device offline (the G915 X LS lists no such function: extCapabilities 0x0004) |
| 14 | – | error 7 (no such function) on 8071 v0 and v4 | never sent |

There is **no function that reads the effect in use**: what the keys show now cannot be read back.

Effect ids as Solaar names them: 0 off, 1 fixed colour, 2 pulse, 3 cycle, 4 wave, 8 startup, 9 demo,
0x0a breathing, 0x0b ripple, 0x0e decomposition, 0x0f signature 1, 0x10 signature 2, 0x15 cycle, 0x16 wave,
0x17 ripple.

**The ten effect bytes of function 1** (the specification's table; offsets from the first of them, which is byte 2
of the request). Periods are milliseconds, big-endian. Intensity: 0 the default (100), else 1–100.

| Id | Effect | Bytes |
|---|---|---|
| 0x00 | off | – |
| 0x01 | fixed colour | 0–2 colour, 3: 0 default, 1 ramp, 2 at once (PeriHub sends 2) |
| 0x0a | breathing | 0–2 colour, 3–4 period, 5 waveform (0 default, 1 sine … 6 exponential), 6 intensity |
| 0x15 | colour cycle | 1 saturation, 6–7 period, 8 intensity |
| 0x16 | colour wave | 1 saturation, 6–7 period, 8 intensity, 9 direction: 0 default, 1 horizontal, 2 vertical, 3 centre out, 4 inward, 5 outward, 6 reverse horizontal, 7 reverse vertical, 8 centre in |
| 0x17 | ripple | 0–2 colour, 3 saturation, 4: 1 skips the start animation, 6–7 period |

The capabilities of an effect (function 0) tell the wave's directions: 0x0020 horizontal, 0x0040 vertical, 0x0080
centre out, 0x0100 inward, 0x0200 outward, 0x0400 reverse horizontal, 0x0800 reverse vertical, 0x1000 centre in;
0x4000 sync, 0x8000 period. The G915 X LS wave has 0xdce1: no inward and outward, as Solaar also notes for the
G515.

### Host mode: what PeriHub does when it drives the lights

To set effects and per-key colours a program takes the lights over, as G HUB does with its "on-board memory
mode" off. `crates/perihub-core/src/hidpp/host.rs` does it on a keyboard that lists 0x8071 and 0x8101, only
when the user asks, never while G HUB or Solaar runs, and only when function 5 says the keyboard drives its
own lights (0):

1. 0x8101 function 6 `05`: host mode (G HUB, Solaar and logi-ghub-linux send it; it ends at a power cycle).
2. 0x8071 function 5 `01 03 06`. Byte 1 hands over the zones (0x01) and the power modes (0x02); byte 2 asks for
   the notices of user activity (0x02) and of no activity (0x04). Solaar sends `04`, G HUB `05` and `03`
   (captures on a G515 and a G915 X), logi-ghub-linux `06`: with `04` alone, it notes, a G915 X puts its own
   lighting back over the host's after a minute (**unconfirmed**, one source).
3. The effects (function 1, persistence 0x01), then the per-key colours (0x8081). The first per-key frame after
   the take-over is sent twice, as Solaar does: it can come before the keyboard has left its own mode.

The keyboard then sends 0x8071 **event 1** (software id 0) with byte 0 = 0 when nobody has used it for the time in
function 7 (60 s on the G915 X LS), and 1 when it is used again, each several times in a row. PeriHub dims the
colours to a quarter (intensity 25 for the effects that have one), and turns the lights off itself with
function 8 `01 03 00` after the second timer of function 7 (270 s): testers of the G515 report that the second
notice never comes while a program drives the lights. On use it sends function 8 `01 01 00` (tried three times,
100 ms apart), function 5 again and the look at full strength. Every 5 seconds it reads function 5: 0 means the
keyboard went back to its own lights (it restarted), and PeriHub takes them again.

To give the lights back: function 5 `01 00 00`, then 0x8101 function 6 `03`. PeriHub does it when the user asks,
when G HUB or Solaar starts, when a command fails, when the device's thread ends, and before it quits or installs
an update. A keyboard that stops answering has its own lights when it comes back, after a power cycle or a deep
sleep; PeriHub shows its look again when it connects, for the rest of the session.

A reply with error 8 (busy) is asked again after 30, 60 and 90 ms, as Solaar does; a per-key frame that failed
half-way gets no end of frame. Calls that keep something in the keyboard's memory are refused even in host mode
(`is_host_call` in `crates/perihub-core/src/hidpp/mod.rs`): functions 3 and 7 with 1, function 1 with another
persistence than 0x01, 0x8081 function 7 with anything but 0, 0x8101 functions other than 0, 6, 8 and 12.

0x0020 (configuration change) function 1 with the value `00 00` hands the lights back on the G515, Solaar
notes. PeriHub never calls 0x0020.

## 0x8081 per-key lighting

| Function | Parameters | Reply | Kind |
|---|---|---|---|
| 0 keys | `00 page`, pages 0, 1, 2 | bytes 2–15: a bitmap, bit *i* (low bit first) over the pages one after the other = key id *i* has its own colour (confirmed by the G915 X LS below) | read |
| 1 set colours | up to 4 `key r g b`; a key id 0x00 or 0xff ends the list | – | change (RAM, shown at function 7) |
| 2, 3, 4 | 5 colours from a key on; the same with 5-bit and 4-bit differences | – | never sent |
| 5 set ranges | up to 3 `first last r g b` | – | change (RAM) |
| 6 one colour | `r g b`, then up to 13 key ids | – | change (RAM); what PeriHub sends |
| 7 end of frame | byte 0 persistence (0 RAM), 1–2 frame, 3–4 frames until the next change: everyone sends zeros | – | change; PeriHub sends only `00…` |

There is **no function that reads the colours back**. Key ids (not in the specification; Solaar, logi-ghub-linux and
g915x-rgb agree): the keyboard usage code minus 3 for 0x04–0x65 (A = 1 … Z = 26, 1 = 27 … 0 = 36, F1–F12 = 55–66,
the numpad 80–96), the usage minus 0x78 for the modifiers (left Ctrl 104 … right Windows 111), 153 the brightness
key, 155–158 the media keys, 180 on the G keys, 210 the logo. The G515 lists some that are no keys (47 and 97 on
ANSI, 99–103, 254). The keyboard shows the per-key colours only while the keys' zone has a fixed colour (Solaar,
logi-ghub-linux), and a key never written shows white: PeriHub always sends every key.

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
| 8071 7 `00` | `00 00 00 00 3c 01 0e` | dims after 60 s (G HUB: 1 minute), off 270 s later: G HUB writes `01 00 00 00 3c 01 0e` for its "5 minutes" (a G HUB capture on a G915 X TKL, in logi-ghub-linux's notes) |
| 8081 0 pages 0–2 | page 0 all `ff`; page 1 `… 00 00 00 00 00 7a 00 00 f0 1f 00 00 04`; page 2 empty | 126 keys: 1–111, 153, 155–158, 180–188 (G1–G9), 210 (logo) |
| 4540 0 | `04` | German ISO |

The bytes after the effect id are the capabilities and the period unit of the specification: `c0 01 1e 0b` for
breathing is colour, sync and period, 7691 ms; `dc e1 00 1e` for the wave is six directions, 30 ms. A G915 X LS TKL
in logi-ghub-linux reports the same. With G HUB closed the keys went back to the keyboard's own effect (a fixed colour)
at the brightness set before.

## 0x8101, 0x4523, 0x8051, 0x1b05, 0x1b10: only function 0

No public Logitech document describes them. 0x8101 (profile management, in place of 0x8100 on these keyboards):
function 6 with `00` reads the mode (reply byte 0: 1 the keyboard's own profiles, 0 host mode; 1–2 the profile
file in use), `05` sets host mode, `03` gives the keyboard its profiles back (G HUB also sends `0b` and `0f`).
Function 8 opens a profile file and function 12 reads 16 bytes of it (logi-ghub-linux, logirgb); PeriHub allows
them as reads but does not call them yet. 0x4523 is named "disable controls" (game mode), 0x1b05 "full key
customization", 0x1b10 "control list", 0x8051 "Logitech modifiers". PeriHub reads function 0 of each, and 0x8101
function 6 with `00`, for the test report; it changes none of them.
