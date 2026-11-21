# Logitech onboard profiles: the profile list and the factory profiles

This page describes, in our own words, how a Logitech HID++ mouse with onboard profiles (feature
`0x8100`) decides which profiles it uses, and what PeriHub does with a mouse nothing was ever saved
to. PeriHub's code for this is written from this page. No code from other projects was copied or
translated.

Sources studied (read only, for the protocol facts):

- [libratbag](https://github.com/libratbag/libratbag) (MIT), `hidpp20.c`: the profile list
  ("dictionary"), its checksum, the fallback to the factory profiles, how a commit writes the
  profiles and then the list, the G305 that refuses to read a list that was never written;
  `driver-hidpp20.c` and `libratbag.c`: a disabled profile is never selected.
- [Solaar](https://github.com/pwr-Solaar/Solaar) (GPL-2.0), `hidpp20.py` and
  `settings_templates.py`: the factory list in sector `0x0100`, and profiles selected by their
  two-byte address, `0x01nn` for a factory profile.
- A real G502 HERO (`046d:c08b`, firmware U1 27.03), which has a profile list of its own:
  everything marked **(G502)**.
- libratbag pull requests [#1676](https://github.com/libratbag/libratbag/pull/1676) (support for the
  PRO X Superlight 2, by Nathan Rossi) and [#1806](https://github.com/libratbag/libratbag/pull/1806)
  (a probe failure on the same mouse, by nt-slug), both still open and both MIT like the project:
  the extended DPI and polling rate features and profile format 7, tried on a real PRO X 2.
- Issues [#15](https://github.com/hex-dump/perihub/issues/15) and
  [#20](https://github.com/hex-dump/perihub/issues/20): the memory formats of a G502 X Wireless and
  of a G502 HERO with newer firmware.
- libratbag's `data/devices/logitech-g604.device` (MIT) and its issue
  [#1477](https://github.com/libratbag/libratbag/issues/1477), which holds the debug log of a G604
  behind a LIGHTSPEED receiver (every request and answer): everything marked **(G604)**.
- [Solaar](https://github.com/pwr-Solaar/Solaar) (GPL-2.0), `base.py`: how a Bluetooth Low Energy
  device is addressed and how it answers.
- For profile format 4 and the lights of the G903 HERO (issues [#22](https://github.com/hex-dump/perihub/issues/22)
  and [#115](https://github.com/hex-dump/perihub/issues/115)): libratbag's issue
  [#774](https://github.com/libratbag/libratbag/issues/774) (a G903 HERO refused as "profile layout not supported:
  0x04", with its debug log), its pull requests [#777](https://github.com/libratbag/libratbag/pull/777) (format 4
  accepted) and [#778](https://github.com/libratbag/libratbag/pull/778) (0x8071), both merged in July 2019, and its
  `src/driver-hidpp20.c` and `data/devices/logitech-g502-x-plus.device`, `logitech-g705.device` (MIT);
  Solaar's `hidpp20.py` (`OnboardProfile`, `OnboardProfiles`); [libhidpp](https://github.com/cvuchener/hidpp)
  (GPL-3.0), `src/libhidpp/hidpp20/ProfileFormat.cpp` and `profile_format.md`; the research notes of
  [logirgb](https://github.com/mevdschee/logirgb) (MIT), `research/8100-onboard-profiles.md`, with its tests on a
  G502 SE HERO. Logitech has not published a `0x8100` specification: its public HID++ folder (the copy in logirgb's
  `research/drive-download.zip`) has `0x8070` and `0x8071` but no `0x8100`. See
  [Profile format 4](#profile-format-4-the-g903-hero) and [Lights through 0x8071](#lights-through-0x8071-on-a-mouse).

What is not marked **(G502)** comes from the two projects above and has not been seen by PeriHub
on a real mouse. In particular, PeriHub has never met a mouse without a profile list.

## Sectors

- The mouse's own memory is a row of sectors of the same size, numbered from `0x0000`. Sector 0 is
  the **profile list**; sector *n* holds profile *n*; the rest is for macros **(G502)**.
- The factory profiles are in read-only sectors: `0x0100` is the **factory list**, `0x0100 + n`
  factory profile *n* **(G502)**. A mouse can have fewer factory profiles than profiles (2 and 5 on the
  G502 X Wireless of #15); the profiles past them start as copies of factory profile 1.
- A sector ends with a CRC-CCITT (initial value `0xFFFF`) of everything before it, big-endian
  **(G502)**. With 256-byte sectors it sits in bytes 254–255, with 255-byte sectors in 253–254.
- Sector sizes seen: 256 (G502 HERO, U1 27.03), 255 (G502 HERO, U1 74.00, #20; G502 X Wireless,
  #15). A 255-byte sector is written as 16 blocks, the last one carrying 15 bytes.
- Profile formats seen: 2 (G502 HERO), 3 (G502 X Wireless), 4 (G903 HERO, issues #22 and #115). libratbag and
  Solaar read and write formats 1 to 5 with one layout.

## The profile list

Four bytes for each profile, then an end marker **(G502)**:

| Bytes | Meaning |
|---|---|
| 0–1 | Sector of the profile, big-endian: `00 01` for profile 1 (`01 01` in the factory list) |
| 2 | 1 if the profile is enabled, 0 if the profile switch skips it |
| 3 | `00` |

The end marker is `FF FF 00 00`; the rest of the sector is `FF`, with the CRC at the end.

The factory list of the G502 HERO reads `01 01 01 FF`, `01 02 01 FF` … `01 05 01 FF`, then `FF FF`
and the name of the firmware in ASCII (`U127.03_B0010`): the same entries with a last byte of
`FF`, all five profiles enabled, and no CRC to check **(G502)**.

## A mouse nothing was saved to

- Its sector 0 holds no list: the sector is blank, so the CRC does not match. The mouse then uses
  the **factory list and the factory profiles**. Whatever its own profile sectors hold is not in
  use. libratbag tells by the CRC, Solaar by the first four bytes being all `FF` or all `00`.
- The G305 answers a read of that sector with an "invalid argument" error instead.
- A profile is selected by a two-byte address: `00 nn` in the mouse's own memory **(G502)**,
  `01 nn` among the factory profiles. The active profile is reported in the same form; PeriHub
  reads its second byte **(G502)**.
- Writing a profile sector alone changes nothing the mouse uses: it needs a list that points to
  it. Both projects write the list whenever they save.

## What PeriHub does

- **Telling a blank list from a damaged one.** PeriHub takes the mouse for one nothing was saved
  to only if both signs are there: the CRC does not match and the first four bytes are blank (or
  the mouse refuses the read, like the G305). A sector with something in it and a CRC that does
  not match is a *damaged* list: PeriHub reads the mouse as before (the profiles in its own
  memory, all enabled) and refuses to save, so that it never replaces profiles the mouse may be
  using. Restoring a backup still works.
- **Reading.** Without a list PeriHub shows the factory profiles, with the enabled flags of the
  factory list (every profile enabled if that list cannot be read or makes no sense), and selects
  profiles by their factory address.
- **The first save** (tested memory formats only). PeriHub writes, in this order: the macros of the saved
  profile, the factory copy of every other profile, the saved profile, and last the list, with the
  flags of the factory list. Until the list is there the mouse keeps using its factory profiles,
  so a save that stops halfway changes nothing. Then it selects, in the mouse's own memory, the
  profile that was active. Every sector is read back after it is written.
- **Enabling a profile** whose sector was never written: its factory copy is written first, so
  the list never points to an empty sector.
- **Untested memory formats** (experimental saves): PeriHub does not create the list, because the
  backup taken before the save would hold no list to put back. The save is refused with an
  explanation; a profile saved once with Logitech G HUB or Onboard Memory Manager creates it.
- A backup taken while the mouse had no list cannot be restored: there is nothing in it the mouse
  would use. **Factory settings** on a profile gives the same result.

## Selecting a profile

- libratbag never selects a profile that the list disables: asked to, its HID++ driver first
  enables it and writes the profiles and the list again, and only then selects it; the library
  above it refuses to make a disabled profile active at all.
- After a save PeriHub loads the saved settings by selecting another profile and then the saved
  one again, since selecting the same profile keeps a preview **(G502)**. It used to fall back on a
  disabled profile when no other one was enabled. A save to a G303 Shroud Edition behind its
  receiver ([issue #34](https://github.com/hex-dump/perihub/issues/34)) failed with "invalid
  argument" (`0x02`), and so did its undo. That fits the switch being refused, after the profile
  had been written and read back (the undo writes only sectors that changed, so a refused write
  would have left it nothing to do): the write commands are the ones a G502 HERO Wireless with the
  same memory (format 3, 16 sectors of 255 bytes, one factory profile) took through its receiver in
  [issue #28](https://github.com/hex-dump/perihub/issues/28). The report does not say which
  profiles the list enables, so a disabled profile is the likely reason, not a proven one.
- PeriHub now goes only through an enabled profile, and with none sends nothing: a written profile
  takes effect by itself **(G502)**. A switch the mouse refuses once every sector is written and read
  back is only logged: the save stands, and on an experimental save the 15-second undo is still
  offered.

## Profile format 4 (the G903 HERO)

The G903 HERO (`046d:c091`, `4087` through its receiver) reports memory model 1, profile format 4, macro format
1, 5 profiles of which 1 is a factory profile, 12 buttons and 16 sectors of 255 bytes, in issue #115 (PeriHub
0.9.7, Windows) as in issue #22 (0.8.2, Linux) and in libratbag's issue #774 (2019, the same `01 04 01 05 01 0c 10
00 ff 0a 04` answer).

### What the sources say

- **libratbag** refused format 4 until its pull request #777. In issue #774 a libratbag maintainer wrote that, like
  the profile layouts before it, the new one is backwards compatible and only adds more. Since then libratbag reads
  and writes formats 1 to 5 with one layout, the same 256-byte structure for all of them (format 4 is named after
  the G915), and nothing in its code depends on the format number once it is accepted.
- **Solaar** reads and writes every format up to 5 with one layout, and refuses the formats above 5.
- **libratbag's pull request #1676** (open) was tried on a G203 that reports format 4: profile names read and
  written with the shared layout.
- **libhidpp** documents formats 1 to 3 only (2 and 3 with the same 230 bytes): it says nothing of format 4.
- What PeriHub reads and writes is at the same place in every one of these: polling interval (byte 0), default
  and shift DPI level (1, 2), five DPI levels (3 to 12, little-endian), 16 button slots of 4 bytes (32 to 95), the
  16 G-shift slots (96 to 159), two LED blocks of 11 bytes (208 to 229), the CRC in the last two bytes. What the
  sources place after that (the alternate LED blocks from 230, a custom animation byte at 252 for format 5) PeriHub
  never writes: a save starts from the sector's own bytes and changes only its fields.

No source shows a format 4 field where PeriHub writes something else, so format 4 is written like 2 and 3.

### What PeriHub does with it

- **With the USB cable**, a mouse with format 4 is **experimental**, as for any memory format nobody has saved to:
  the **No warranty** box before the first save, a backup, every write read back, and the 15-second undo.
- **Through a receiver** it stays **read-only** until a tester confirms format 4 in 255-byte sectors with the cable
  (`cargo xtask confirm --memory-format 4:255`); then saving over the radio opens too, as an experimental feature,
  as for format 3.
- The G903 HERO has 12 buttons: PeriHub writes the 12 slots the mouse reports in each layer and leaves the other
  four as they are. Its profile list (sector 0) in issue #115 enables profile 1 only, with `FF` as the last byte of
  each entry, and its CRC (`40 37`) matches: PeriHub reads it like any other list
  (`crates/perihub-core/tests/fixtures/transcripts/logitech-g903-hero-receiver.json`).

## Lights through 0x8071 on a mouse

The G903 HERO lists `0x8071` (RGB effects) and not `0x8070` (colour LED effects), so PeriHub 0.9.7 reported
"LEDs: none reported" (issue #115).

- **Reading.** A mouse that has no `0x8070` and whose feature table lists `0x8071` is asked `0x8071` function 0
  only, which reads: the zone count, each zone's location and number of effects, and each effect's id (the layout
  of the answers is in [logitech-keyboard-lighting.md](logitech-keyboard-lighting.md)). In libratbag's pull request
  #778 a G903 HERO answered 2 zones: zone 0 at location 1 (primary) and zone 1 at location 2 (logo), each with 4
  effects. A zone that cannot be read leaves the list empty; the mouse still opens. Nothing else of `0x8071` is
  sent, not even a preview in RAM.
- **Which LED block of a profile holds which zone: the sources disagree.**
  - libratbag puts zone *n* in LED block *n*, for `0x8070` and `0x8071` alike, except on the G502 X PLUS and the
    G705 (its `G502X_PLUS` quirk): their one zone is in the second block. The tester of pull request #778 said he
    could control the G903 HERO's lights with Piper, not which light took which setting.
  - libhidpp names the block at 208 the logo effect and the block at 219 the side effect.
  - logirgb wrote green to block 0 and red to block 1 of a G502 SE HERO: its `0x8070` then reported zone 1 (logo)
    green, and the logo was green. Block 0 drives the logo (zone 1) and block 1 the DPI indicator (zone 0), the
    reverse of libratbag.
  - In PeriHub's issue [#28](https://github.com/hex-dump/perihub/issues/28) a G502 HERO Wireless showed the effect
    PeriHub saved for zone 0 on its logo, which fits logirgb's finding.
  - **Checked on a wired G502 HERO (046d:c08b, firmware U1 27.03.0010) on 2026-11-21**, with PeriHub's command line:
    a profile saved with red fixed in block 0 and green fixed in block 1 lit the G logo red and the DPI indicator
    green; a preview in RAM of `0x8070` zone 0 (location 1) in blue then turned the DPI indicator blue, the logo
    staying red. So on that mouse block 0 is the logo (zone 1, location 2) and block 1 the DPI indicator (zone 0,
    location 1), as logirgb found. The profile was saved back as it was afterwards.

  So PeriHub does not tie a `0x8071` zone to a profile LED block. The mouse page lists the zones, where they are and
  the effects each one offers, and what the two LED blocks of the profile hold, and changes neither: a save keeps the
  LED blocks of a `0x8071` mouse byte for byte. The test report lists the zones with "(8071)" and "no profile
  slot". A tester who says which light shows what, compared with the profile's blocks, can settle it.
- **0x8070 mice:** on the wired G502 HERO (`c08b`) PeriHub now puts the logo (location 2) in block 0 and the DPI
  indicator (location 1) in block 1, from the check above; before 0.9.8 it put zone *n* in block *n*, which swapped
  the two lights in saves and in what the Lighting tab showed of a profile. Every other `0x8070` mouse keeps zone *n*
  in block *n*: right on the G502 HERO Wireless (#28), not checked on the others (`profile_led_slot` in
  crates/perihub-core/src/hidpp/mod.rs).

## Format 7 and the extended DPI and polling rate features

The PRO 2 LIGHTSPEED (`046d:c09a`, through a receiver `40a8`) and the PRO X 2 DEX (`c0a0`, `40b8`)
of issues [#33](https://github.com/hex-dump/perihub/issues/33) and
[#32](https://github.com/hex-dump/perihub/issues/32) report memory model 1, profile format 7, macro
format 1 and 16 sectors of 255 bytes, and have no `0x2201` and no `0x8060`: they use `0x2202` and
`0x8061` instead. Their firmware (MPM 31.04.0012 and 37.03.0007) is among those libratbag's pull
request lists for format 7; the older firmware of the PRO X Superlight 2 reports format 6, which has
the same layout without the bunny-hop byte. What follows is what the pull requests describe, in our
words, and what the test reports confirm: **nothing here has been written by PeriHub, only read.**

### 0x2202, extended adjustable DPI (functions as the number of the call)

All answers start with the sensor number, as 0x2201's do.

- **0** sensor count.
- **1** (sensor) axes: the third byte of the answer has bit 0 when the sensor has its own Y axis and
  bit 1 when it has an adjustable lift-off distance.
- **2** (sensor, axis, page) one page of the DPI list of that axis (0 = X, 1 = Y): the answer repeats
  the three bytes and then carries 13 bytes of the list. The list is a row of big-endian 16-bit
  values that goes on from page to page (a value may be cut between two pages) and ends with a 0.
  A value whose top three bits are set (`0xE000 | step`) is followed by an end value: it stands for
  every DPI after the previous one, `step` apart, up to the end value. A list of 100, `E032`, 25600 is
  100 to 25600 DPI in steps of 50.
- **5** (sensor) the DPI now: X, the default X, Y, the default Y (big-endian 16-bit, in this order,
  after the sensor byte) and the lift-off distance (1 low, 2 medium, 3 high).
- **6** sets the DPI and lift-off distance. PeriHub does not send it.

### 0x8061, extended adjustable polling rate

The connection is a parameter: 0 with the cable, 1 through the LIGHTSPEED receiver. Rates are
numbered 0 to 6: 125, 250, 500, 1000, 2000, 4000 and 8000 Hz.

- **0** (connection) the rates the mouse offers on it, a big-endian 16-bit mask with bit *n* for rate
  *n*. libratbag reads it from the first two bytes of the answer; PeriHub shows those two bytes in
  its test report, to check.
- **2** (connection) the rate in use: its number, first byte of the answer.
- **3** sets it. PeriHub does not send it.

### Format 7 profile (255-byte sector, little-endian numbers)

| Offset | Length | What |
|---|---|---|
| 0 | 1 | polling rate number through the receiver |
| 1 | 1 | polling rate number with the cable |
| 2 | 1 | default DPI level (0-based) |
| 3 | 1 | DPI shift level |
| 4 | 5 × 5 | five DPI levels: X (2), Y (2), lift-off distance (1); an X of 0 or 0xFFFF ends the list |
| 29 | 8 | not known |
| 37 | 1 | bunny-hop ("BHOP"): 0 off; libratbag reads 10 to 100 as 100 ms to 1000 ms. Only in format 7 |
| 38 | 6 | not known |
| 44 | 2 | power-save timeout (unit not known) |
| 46 | 2 | power-off timeout (unit not known) |
| 48 | 16 × 4 | button bindings, in the usual 4-byte form |
| 112 | 12 × 4 | the bindings of the second layer |
| 160 | 48 | profile name, UTF-16LE, ending at a 0 or 0xFFFF |
| 208 | 2 × 11 | lights: two zones of 11 bytes, in the usual form |
| 230 | 2 × 11 | the lights of the second layer |
| 253 | 2 | CRC-CCITT of the sector, as in the other formats |

### What PeriHub does with it

- It reads all of this when the mouse connects and puts it in the test report: the DPI ranges, the
  DPI in use, the rates on both connections and the one in use, the two raw answers behind them, and
  one line for each profile, so that a tester can compare them with G HUB. The mouse page shows the
  profiles too (X levels, the polling rate of the connection in use, buttons).
- **Nothing is written and the onboard mode is never switched**: libratbag's second pull request
  found that putting a PRO X 2 into onboard mode left its buttons without assignments until it was
  unplugged. The same pull request found that the profile list (sector 0) of such a mouse can
  answer "invalid argument", which PeriHub already reads as "never saved to" (the G305 does the
  same): the factory profiles are then shown.
- Saves wait until a tester confirms the readings, and then follow the memory format, not the
  model, like the others.

## The G604, and a mouse over Bluetooth Low Energy

Issue [#43](https://github.com/hex-dump/perihub/issues/43) asks for a G604, with the id it has over
Bluetooth.

### Its ids

libratbag's device file names the G604 (driver `hidpp20`) with `usb:046d:4085;bluetooth:046d:b024`.
`4085` is the id the mouse reports through a LIGHTSPEED receiver (PeriHub's *wireless id*, as for the
other wireless mice); `b024` is the id of its Bluetooth HID device. The G604 has no data cable, so it
has no USB id of its own. A mouse that PeriHub names with a Bluetooth id is found by it, and by nothing
else.

### What a G604 answers **(G604)**

From the debug log of libratbag's issue #1477 (Linux, a G604 behind a `c539` receiver, which PeriHub
reads the same way as the receiver of any other mouse):

- HID++ 4.2, with 33 features in its table, the root included. The log lists them all, the hidden ones
  too (`0x0020` reset and `0x00C2` firmware update among them, which PeriHub never calls).
- `0x1000` battery: `5A 32 00` is 90 %, the next level 50 %, state 0 (discharging).
- `0x8060` polling rate: the list byte `8B` is 1, 2, 4 and 8 ms; 1 ms (1000 Hz) in use.
- `0x2201` DPI: 100 to 25600 in steps of 50 (list `0064 E032 6400 0000`); 4750 now, 800 the default.
- `0x8070` colour LEDs: one zone, the logo, with four effects.
- `0x8100` onboard profiles: `01 03 01 05 01 10 10 00FF 0A 04` is memory model 1, profile format 3, macro
  format 1, 5 profiles of which 1 is a factory profile, 16 buttons, 16 sectors of 255 bytes. That is the
  memory format a G502 X Wireless confirmed in issue #15 (format 3, 255 bytes), so a G604 is read and
  written like the other mice that use it, by its memory format and not by its name.
- The profile list (sector 0) enables profiles 1 to 3 and not 4 and 5; profile 3 is the active one
  (`0x8100` function 4 answers 3). The first profile (sector 1) has 4000 DPI, 1 ms and the name
  "Firefox", and its 16 buttons are left, right and middle click, a G-shift button, the two side
  buttons, seven macros (all in sector 6) and three slots with a function PeriHub has no name for
  (`90 10 FF 00`, `90 11 FF 00`, `90 00 FF 00`): PeriHub keeps those bytes as they are.
- `crates/perihub-core/tests/fixtures/g604/` holds the first eight sectors of that log, as the mouse
  answered them, and the tests build the simulated mouse from them. The log has no unit id; the one the
  tests give the mouse is made up and stays out of the transcript like the others.

Nothing from this list is a PeriHub finding: nobody has run PeriHub on a G604 yet.

### Over Bluetooth

The request of issue #43 (PeriHub 0.9.0 on Windows) lists the Bluetooth G604 as `046d:b024` with four
HID collections: the mouse (`0001/0002`), a keyboard (`0001/0006`, for the keys the mouse's own buttons
send), consumer keys (`000C/0001`) and a vendor one, usage page `FF43` usage `0202`. That last one is
the HID++ collection: Logitech devices that speak HID++ over Bluetooth Low Energy have it.

- **Which node.** Windows gives every collection a path of its own: PeriHub opens `FF43/0202` and not
  the mouse or keyboard ones. Linux has one hidraw node for the whole device, whose parent HID device is
  named `0005:046D:B024.NNNN` (the kernel's number for the Bluetooth bus is 5): PeriHub's udev rule
  opens that node to the user (`KERNELS=="0005:046D:B024.*"`), not by a USB attribute, which a Bluetooth
  device does not have.
- **Addressing.** A device that is connected directly is asked on device index `0xFF`, as with its
  cable. Solaar accepts an answer whose device number is the request's or its complement (`0xFF`
  asked, `0x00` answered; its comment says "BT device returning 0x00"): PeriHub accepts those too, over
  Bluetooth only. PeriHub sends only long reports (`0x11`) on every connection.
- **Asleep, away or silent.** A Bluetooth mouse that stops answering, or that the system can no longer
  write to, is asleep or out of range, as a mouse behind a receiver that does not answer is: PeriHub asks
  again when it is back. If its node is there and nothing answers when PeriHub opens it, the mouse's page
  says that it is connected but does not answer, so that it does not simply stay out of sight.
- **What is read.** Everything that is read through a receiver: the memory model and its sectors, the
  profile list, the active profile, DPI, polling rate, LEDs, battery and the feature table. The
  extended polling rate (`0x8061`) numbers two connections only, the cable and the receiver, so it is
  not asked over Bluetooth (the G604 has `0x8060`, which has no connection parameter).
- **Nothing is written.** No call that changes anything is sent over Bluetooth, not even to RAM: no
  save, no profile switch, no DPI, no change of the onboard mode. Nobody has tried a write that way, and
  the transport (a Bluetooth HID report, a link that can drop in the middle of a transfer) is not one
  PeriHub has seen. The mouse page says so and points to the receiver. The test report names the
  Bluetooth id and the connection, so that a clean report can lift this later.
