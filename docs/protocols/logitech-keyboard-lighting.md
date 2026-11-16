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
mode" off. `crates/perihub-core/src/hidpp/host.rs` does it on a keyboard that lists 0x8071 and 0x8101, once the
user asked for it (see below for when it does it again by itself), never while G HUB or Solaar runs, and only
when function 5 says the keyboard drives its own lights (0):

1. 0x8101 function 6 `05`: host mode (G HUB, Solaar and logi-ghub-linux send it; it ends at a power cycle).
2. 0x8071 function 5 `01 03 06`. Byte 1 hands over the zones (0x01) and the power modes (0x02); byte 2 asks for
   the notices of effect sync (0x01), user activity (0x02) and no activity (0x04). Solaar sends `04`,
   logi-ghub-linux `06`: with `04` alone, it notes, a G915 X puts its own lighting back over the host's after a
   minute (**unconfirmed**, one source). G HUB re-arms the one notice it waits for: `01 03 05` while the keyboard
   is in use (effect sync and no activity) and `01 03 03` once it is idle (effect sync and activity), within 9 ms
   of each notice (USB captures by the tester of issue #30 on the G915 X LS, and ksanislo's on a G515). PeriHub
   keeps `06`: with it the G915 X LS of issue #30 dimmed after a minute, turned off later and lit again at the
   first key, and PeriHub has no use for the effect sync events (event 0) that `05` adds.
3. Function 8 `01 01 00`: lights on (tried three times, 100 ms apart). Until the take-over the keyboard dims its
   lights and turns them off by itself when nobody types; once the power modes are the program's, it stays as it
   was. Without it the keys stayed dark at a first take-over in issue #30 (the user had been clicking with the
   mouse; a second take-over, after typing, was right). G HUB sends no function 8 in the captures, which never
   reach the off timer.
4. The effect of the keys' zone (function 1, persistence 0x01), the per-key colours (0x8081), then the effects of
   the other zones, the logo last. The first per-key frame after the take-over is sent twice, as Solaar does: it
   can come before the keyboard has left its own mode. (G HUB never sets the logo zone on its own in the
   captures: it sends one effect on all zones, `ff 06 …`, the keyboard's multi-zone effect 0x18 with two colours,
   a 3000 ms period and intensity 100, or 50 while idle.)

**The logo under colours per key.** While a per-key frame is shown (the keys' zone on a fixed colour, then 0x8081
functions 6 and 7), the G915 X LS shows the logo with the frame's colour for the logo's key (210), whatever the
logo zone's effect, and a key left out of the frame keeps the colour it had. In issue #30 the logo's breathing
never showed and its colour cycle stayed red, first with the logo's key in the frame, then with it left out and
the logo's effect sent after the frame; with the keys on a wave (no frame) both showed at once. That the frame
hides the logo zone, rather than the order of the calls, is PeriHub's reading of these three tests
(**unconfirmed** by another source). So with colours per key PeriHub gives the logo's key the logo zone's look
itself: its colour when fixed, black when off, and for breathing and colour cycle the colour of the moment,
sent every 100 ms when it changed (0x8081 function 6 with the one key, then function 7 `00…`, in RAM; a quarter of
the colour while idle, nothing while the lights are off). Breathing goes from the colour to dark and back as a
cosine over the period; the colour cycle goes once round the hue wheel per period, rounded to the keyboard's unit
as for the zone's own effect. The logo zone's own effect is still sent, last.

A new look is sent the same way, without the take-over. Leaving a wave "to the centre" (direction 8) is the
exception: the G915 X LS of issue #30 kept that wave, its period too, through every later effect of the zone
until the lights were given back and taken again, so PeriHub does just that (function 5 `01 00 00`, 0x8101 `03`,
150 ms, then steps 1–4). That this direction alone does it is **unconfirmed**: the test report counts the looks
shown and these resets.

The keyboard then sends 0x8071 **event 1** (software id 0) with byte 0 = 0 when nobody has used it for the time in
function 7 (60 s on the G915 X LS), and 1 when it is used again, each several times in a row. PeriHub dims the
colours to a quarter (intensity 25 for the effects that have one), and turns the lights off itself with
function 8 `01 03 00` after the second timer of function 7 (270 s): testers of the G515 report that the second
notice never comes while a program drives the lights. On use it sends function 8 `01 01 00` (tried three times,
100 ms apart), function 5 again and the look at full strength. Every 5 seconds it reads function 5: 0 means the
keyboard went back to its own lights (it restarted), and PeriHub takes them again.

To give the lights back: function 5 `01 00 00`, then 0x8101 function 6 `03`. PeriHub does it when the user asks,
when G HUB or Solaar starts, and before it quits or installs an update. (G HUB, when its on-board memory mode is
turned on, sends function 5 `01 00 00` and 0x8101 function 6 `0f`, which answers `03 00 01`: issue #30
captures.)

When a call fails while PeriHub drives the lights:

- no answer in time (1 s): the next tick paints the look again. The keyboard can be slow: in the captures of
  issue #30 it took 1 s to answer G HUB's first call and 1.4 s while another G HUB client was busy with it. After
  three in a row, as when the keyboard is gone (the receiver says it is asleep or out of reach), the device's
  thread ends **without** giving the lights back;
- refused (such as error 5 once the keyboard left host mode on its own): PeriHub takes the lights again, and
  gives them back only if that fails too.

Before, any failed call gave the lights back, and a keyboard that went away for a moment came back still in host
mode (function 5 reads 3): PeriHub then refused to take it again, as if another program drove it, and its page
showed **Drive the lights** with the editor back to white (issue #30, "after a while"). Now, when the keyboard
answers again, PeriHub takes the lights again and shows the same look even if function 5 reads 3, unless G HUB or
Solaar runs; when the user asks, 3 is still refused. A keyboard that was turned off or slept deeply has its own
lights when it comes back, and PeriHub shows its look again too, for the rest of the session.

The last look shown is saved in PeriHub's settings, with the user's wish that PeriHub drives the lights: set by
"Drive the lights", cleared only by "Give the lights back" (on the page: "Stop driving them" while PeriHub waits)
and by the tray's "Turn off experimental lighting". Giving the lights back on its own (G HUB or Solaar started, a
failed take-over) or when quitting or updating leaves the wish as it is. While it is set, PeriHub takes the lights
with the saved look every time the keyboard connects, at every start too, with the checks of a manual take-over:
G HUB and Solaar not running, and function 5 reading 0 (or 3 when PeriHub itself had them before the keyboard went
away, this session). The "No warranty" box needs no new answer: the wish only exists once it was accepted. When a
check fails the page says why ("Not now: …") and PeriHub checks again every 5 seconds, so it takes the lights
again once G HUB or Solaar quit; after a take-over that failed it waits until the keyboard connects again. The
test report counts, over the whole session, the take-overs (and
those on a reconnection), the give-backs and the last reason, the calls not answered in time, the notices, the
looks shown and the wave resets: the counts of one take-over alone started again from 0 at each one.

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
| 6 one colour | `r g b`, then up to 13 key ids | – | change (RAM); what PeriHub sends; a key not listed keeps its colour (issue #30, below) |
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
customization", 0x1b10 "control list", 0x8051 "Logitech modifiers". PeriHub reads function 0 of 0x8101, 0x1b05,
0x1b10 and 0x8051, and 0x8101 function 6 with `00`, for the test report; it changes none of them. It no longer
calls 0x4523 function 0: that is the call G HUB writes the game mode's disabled keys with (below), so asking it
with no parameters may have sent "no key disabled" (the G915 X LS answered `00` to it in the reports of issue #30).

## Key assignments and game mode: what G HUB sends (issue #30 captures)

USB captures by the tester of issue #30 (2026-11-16, Windows, USBPcap, G HUB) on the G915 X LS behind its c547
receiver, one change each: F5 assigned to B, Insert assigned to B, and the A key disabled in game mode. Facts read
from the bytes only; **PeriHub sends none of this**. The requests go to device 1 on the receiver's interface 2
with software id 0x0b. The feature indexes are this keyboard's, from the order of its feature table in the test
report: 0x0c is 0x4523, 0x0f is 0x8101, 0x10 is 0x1b05.

**Key assignment: a 13-byte file written through 0x8101.** Each assignment is three calls, answered within
1 s, identical in both captures but for the key byte and the checksum:

| # | Call | Parameters | Reply |
|---|---|---|---|
| 1 | 0x8101 function 2 (short report) | `00 0d 00`: 0x000d, the length of what follows | zeros |
| 2 | 0x8101 function 3 | the 13 bytes `00 01 KK 01 00 80 00 00 00 80 02 00 05` | `00 01`, then zeros |
| 3 | 0x8101 function 9 | `1b 05 01 02 00 00 0d`, then a CRC-32 of the 13 bytes, big-endian | zeros |

- `KK` is the key being assigned: 0x0b for F5, 0x23 for Insert. It is neither the HID usage (0x3e, 0x49) nor
  the 0x8081 key id (59, 70); that it is the key's index in the 0x1b10 control list is **unconfirmed**.
- The last four bytes, `80 02 00 05`, are the target: 0x80 a HID action, 0x02 a keyboard key, 0x00 no modifier,
  0x05 the usage of B. This is the button layout libratbag documents for 0x8100 onboard profiles; that it is the
  same here is **unconfirmed**, as are `00 01` (one entry?), the `01` after the key and `00 80 00 00 00`.
- Function 9 names the feature the file belongs to (`1b 05`, full key customization) and repeats the length
  (`00 0d`); `01 02` is unknown. The checksum is the standard CRC-32 (the one of zlib, polynomial 0x04c11db7
  reflected, start and final xor 0xffffffff): `95 be f5 97` for F5, `a9 e1 9b 05` for Insert, both checked
  against the 13 bytes.
- So 0x8101 functions 2, 3 and 9 open, fill and close a file. Whether it is kept in the keyboard's memory is not
  known: PeriHub treats them as memory writes and refuses them (`is_memory_write` in
  `crates/perihub-core/src/hidpp/mod.rs`). The earlier captures of F1 → A and F2 → A came out the same, so
  they could not show the key.

**Game mode: one call to 0x4523 function 0.** Disabling A sent a single long report and nothing through 0x8101:
0x4523 function 0 with the 16 bytes `00 00 00 00 00 00 00 00 08 00 00 00 44 00 18 08`, answered with 16 zeros
1.2 s later. 16 bytes are 128 bits, as many as the 128 controls 0x1b10 function 0 reports on this keyboard
(`80`): a bitmap of the disabled controls by control index is the likely reading (**unconfirmed**). Six bits are
set (counting from the low bit of byte 0: 67, 98, 102, 115, 116 and 123), more than the one key changed, so the
game mode had other keys disabled before (which ones is not known); which bit is A cannot be told from one
capture. Whether the keyboard keeps the list after a power cycle is not known. Function 0 being a
write is why PeriHub stopped calling it for the test report (above).

What would settle the open points: the 0x1b10 control list (function 1, one entry per index, a read), and a game
mode capture with no key disabled, then with only one.
