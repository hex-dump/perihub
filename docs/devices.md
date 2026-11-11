# Devices

What PeriHub recognizes, and how far each device is supported. PeriHub only talks to the
devices listed here, and to the keyboards with QMK firmware described at the end: it never sends
anything to a device it does not recognize.

**Support levels**

| Level | What PeriHub does |
|---|---|
| ✅ **Supported** | Everything works, tested on real hardware. |
| 🧪 **Experimental** | Recognized, not tested yet. PeriHub reads it, and you can make experimental changes at your own risk (see each section). |
| 👁 **Read-only** | Recognized, not tested yet. PeriHub reads it and changes nothing. |

A device moves up a level when its owner sends a test report: **Open test form** on its page in
PeriHub opens the report on GitHub already filled in. For Logitech mice with onboard profiles the
level follows the memory format the mouse reports, so one report moves up every mouse that uses
the same format.

- [SteelSeries](#steelseries)
- [Logitech mice with onboard profiles](#logitech-mice-with-onboard-profiles)
- [Logitech G300](#logitech-g300)
- [Corsair](#corsair)
- [ASUS](#asus)
- [SteelSeries mice](#steelseries-mice)
- [Razer mice](#razer-mice)
- [Glorious mice](#glorious-mice)
- [Keyboards with QMK firmware (Vial, VIA)](#keyboards-with-qmk-firmware-vial-via)

## SteelSeries

| Device | USB ID | Level |
|---|---|---|
| Apex 3 TKL | `1038:1622` (firmware 3.0.4) | ✅ Supported |
| Apex 3 | `1038:161a` | Recognized, PeriHub sends nothing to it yet ([issue #9](https://github.com/hex-dump/perihub/issues/9)) |

## Logitech mice with onboard profiles

These mice speak Logitech's HID++ 2.0 protocol. The list comes from
[libratbag](https://github.com/libratbag/libratbag).

- **Logitech G502 HERO** (`046d:c08b`): ✅ supported.
- **Logitech G502 X Wireless** (`046d:c098`): ✅ supported with the USB cable, confirmed by a user
  on a real mouse ([issue #15](https://github.com/hex-dump/perihub/issues/15)).
- **Every other mouse below**: the level depends on the format of the mouse's onboard memory,
  which PeriHub reads from the mouse every time it connects. Many models share a format, and the
  same model can change format with a firmware update, so the level is not fixed per model:
  - ✅ **a tested format**: profile format 2 in 256-byte sectors (the G502 HERO's) or format 3 in
    255-byte sectors (the G502 X Wireless's). The mouse is supported, whatever the model.
  - 🧪 **a format PeriHub can write but nobody has tried yet** (format 2 or 3, in 255- or 256-byte
    sectors): you can save at your own risk with the USB cable. Before the first save to that
    mouse PeriHub asks you to tick a **No warranty** box, and each save is undone after 15 seconds
    unless you keep it. Enabling or disabling profiles stays off.
  - 👁 **any other format**: read-only.

  The test report shows the format of your mouse, and one report makes a format tested for every
  mouse that uses it.
- **A mouse nothing was ever saved to** has no profile list of its own and uses its factory
  profiles: PeriHub shows those. With a tested format the first save creates the list, with a copy
  of the factory profiles. With an untested one PeriHub does not create it: save a profile once
  with Logitech G HUB or Onboard Memory Manager first.
- **Wireless models** are also recognized through their LIGHTSPEED receiver (`046d:c539`, `c53a`,
  `c53d`, `c53f`, `c541`, `c543`, `c545`, `c547` or `c54d`). Nothing is saved over the radio:
  with a tested memory format PeriHub switches the profile and DPI level, otherwise it only reads. The receiver support has not
  been tried on real hardware yet: reports are welcome in
  [issue #4](https://github.com/hex-dump/perihub/issues/4).
- **The Logi Bolt receiver** (`046d:c548`) and **the Unifying receivers** (`046d:c52b`, `c532`)
  are recognized. Every device paired with them that answers, mouse or keyboard, gets a page of
  its own in the sidebar, built from the HID++ features it reports: what kind of device it is, what
  it can do (battery, DPI, SmartShift wheel, programmable buttons…), its battery and DPI as they
  are now, the DPI and polling rates it accepts, and a test report.
  - 🧪 **DPI**, on a device that has adjustable DPI (HID++ 0x2201) and no onboard profiles (with
    them, the active profile sets the DPI): PeriHub sets it in the device's RAM, never in its
    memory, and only to values the device says it accepts. Such devices forget it when they are
    turned off or fall asleep, so PeriHub sets it again when the device connects or wakes up (it
    checks every 10 seconds; a very short nap can go unnoticed), while PeriHub is running. Before the first change PeriHub asks you to tick
    the **No warranty** box. Nobody has tried it on a real device yet: the MX Master 3S of
    [issue #17](https://github.com/hex-dump/perihub/issues/17) is the first candidate.
  - 👁 Everything else is only read.
  - **Solaar and logiops** set their own DPI and may undo PeriHub's: the device page says so when
    one of them runs.
  - Logitech devices PeriHub does not know behind a **LIGHTSPEED** receiver get the same page,
    read-only if they have onboard profiles (most gaming mice).
- **On Linux PeriHub's udev rule does not open the Unifying receivers.** Linux puts them in DJ mode,
  where what is typed on the keyboards paired with them arrives on the same interface as HID++:
  opening it would let any program running as the user read that typing. The mouse page explains
  this and offers an optional rule, `73-perihub-unifying.rules`, which PeriHub installs only if you
  ask, after the warning, and can remove. With Solaar installed its own rule already opens them.
  Windows is not affected.

| Mouse | USB ID | Through a receiver |
|---|---|---|
| G502 Proteus Core | `046d:c07d` | |
| G502 Proteus Spectrum | `046d:c332` | |
| G502 HERO Wireless | `046d:c08d` | `407f` |
| G502 X | `046d:c099` | |
| G502 X Wireless | `046d:c098` | `409f` |
| G502 X PLUS | `046d:c095` | `4099` |
| G Pro | `046d:c085`, `046d:c08c` | |
| G Pro Wireless | `046d:c088` | `4079` |
| G Pro X Superlight | `046d:c094` | `4093` |
| G102 / G103 / G203 | `046d:c084`, `046d:c092`, `046d:c09d` | |
| G302 | `046d:c07f` | |
| G303 | `046d:c080` | |
| G303 Shroud Edition | `046d:c097` | |
| G402 | `046d:c07e` | |
| G403 | `046d:c083` | |
| G403 HERO | `046d:c08f` | |
| G403 Wireless | `046d:c082` | `405d` |
| G703 | `046d:c087` | `4070` |
| G703 HERO | `046d:c090` | `4086` |
| G705 | `046d:c096` | `409d` |
| G900 | `046d:c081` | `4053` |
| G903 | `046d:c086` | `4067` |
| G903 HERO | `046d:c091` | `4087` |
| MX518 | `046d:c08e` | |
| G305 | wireless only | `4074` |
| G603 | wireless only | `406c` |
| G604 | wireless only | `4085` |

**Memory formats known so far.** What other projects found, for mice PeriHub has not seen yet:
the format a mouse reports in PeriHub's test report is what counts.

| Mouse | Profile format | Sector size | Source |
|---|---|---|---|
| G502 HERO | 2 | 256 bytes, 255 on newer firmware | PeriHub ([issue #20](https://github.com/hex-dump/perihub/issues/20)) |
| G502 X Wireless | 3 | 255 bytes | PeriHub ([issue #15](https://github.com/hex-dump/perihub/issues/15)) |
| G900 | 3 | | libratbag ([PR #1676](https://github.com/libratbag/libratbag/pull/1676)) |
| G203 | 4 | | libratbag ([PR #1676](https://github.com/libratbag/libratbag/pull/1676)) |
| G502 X PLUS | 5 | | libratbag ([issue #1344](https://github.com/libratbag/libratbag/issues/1344)) |
| G Pro X Superlight 2 (not recognized yet) | 6, then 7 after a firmware update | | libratbag ([issue #1572](https://github.com/libratbag/libratbag/issues/1572)) |

Own one? Send a [mouse test report](https://github.com/hex-dump/perihub/issues/new?template=mouse_test.yml)
(**Open test form** on the mouse page fills it in).

## Logitech G300

| Device | USB ID | Level |
|---|---|---|
| G300 | `046d:c246` | ✅ Supported |

The G300 has a protocol of its own ([how it works](protocols/logitech-g300.md)). PeriHub shows its
three profiles: light colour, report rate, DPI levels and DPI shift, and what each button does (G4
to G9 as printed on the mouse), with the active profile and DPI level. The format was checked
against a real G300 and Logitech Gaming Software in
[issue #16](https://github.com/hex-dump/perihub/issues/16), and PeriHub 0.6.1 read it correctly on
Windows.

**Edit** on a profile changes it: light colour, report rate, DPI levels, default level and shift,
and the buttons except left and right, with only the values Logitech Gaming Software offers.
PeriHub backs up the three profiles before every save, reads the save back, and asks whether to
keep it: without an answer the backup goes back after 15 seconds. Until 0.6.3 this was an
experimental feature. It was confirmed on a real G300 in
[issue #21](https://github.com/hex-dump/perihub/issues/21), on Windows: the changes took effect,
they were still there after the mouse was unplugged and plugged back in, and a save that was not
kept was undone. No G300 has been tried on Linux yet. Something does not work on yours? Send a
[device test report](https://github.com/hex-dump/perihub/issues/new?template=device_test.yml).

## Corsair

🧪 Experimental: 26 mice and 23 keyboards of Corsair's newer generation, on their USB cable, and
the wireless ones also through their receiver (Slipstream `1ba6`, `1bdc`, `2b00`, and the older
HARPOON, DARK CORE RGB PRO and IRONCLAW receivers; not yet the Slipstream V2).

- PeriHub reads the firmware, polling rate and DPI.
- **Experimental lighting**, for the session only: one colour on every LED or key, at your own risk.
  The device goes back to its own settings when you turn it off, when PeriHub quits and as soon as
  anything fails. Nothing is written to its memory.
- With more than one device on the same receiver, PeriHub shows the first one only, for now.

**Mice:** KATAR PRO, KATAR PRO XT, M55, M55 RGB PRO, M65 RGB ULTRA, M65 RGB ULTRA WIRELESS, M75,
M75 WIRELESS, M75 AIR WIRELESS, SABRE RGB PRO, SABRE RGB PRO WIRELESS, SABRE PRO CS, SABRE V2 PRO
(and MG, CF: no lighting), IRONCLAW RGB WIRELESS, IRONCLAW WIRELESS SE, DARK CORE RGB PRO,
DARK CORE RGB PRO SE, HARPOON RGB WIRELESS, HARPOON V2, NIGHTSABRE WIRELESS, SCIMITAR RGB ELITE,
SCIMITAR RGB ELITE WIRELESS, SCIMITAR ELITE WIRELESS SE, DARKSTAR RGB WIRELESS.

**Keyboards:** K55 PRO RGB, K55 RGB PRO XT, K55 CORE RGB, K55 CORE TKL RGB, K57 RGB WIRELESS,
K60 RGB PRO, K60 RGB PRO LP, K65 PLUS WIRELESS, K65 PRO MINI, K65 RGB MINI, K70 CORE RGB,
K70 CORE TKL, K70 CORE TKL WIRELESS, K70 MAX, K70 PRO, K70 PRO MINI WIRELESS, K70 PRO TKL,
K70 RGB TKL, K70 RGB TKL CS, K95 RGB PLATINUM XT, K100 RGB, K100 AIR WIRELESS, MAKR 75.

USB IDs, packet sizes and LED counts are in [protocols/corsair-bragi.md](protocols/corsair-bragi.md).
Older Corsair devices (K70 RGB MK.2, K95 RGB PLATINUM, M65 RGB ELITE and others) speak an older
protocol and are not recognized.

Own one? Send a [device test report](https://github.com/hex-dump/perihub/issues/new?template=device_test.yml)
(**Open test form** on the device page fills it in).

## ASUS

🧪 Experimental: 21 ASUS ROG and TUF mice, on their USB cable and through their own wireless
receiver. Nobody has tried PeriHub on one yet. Their protocol comes from
[libratbag](https://github.com/libratbag/libratbag) (MIT), and is described in
[protocols/asus-rog.md](protocols/asus-rog.md).

- PeriHub reads the active profile, firmware, DPI levels, polling rate, button response, angle
  snapping, lighting and what each button does.
- **Apply** changes the DPI levels, polling rate, button response, angle snapping and lighting of
  the active profile without saving them to the mouse's memory, under a warning. **Save to
  memory** also stores them, after the No warranty box the first time. Every change is backed up
  first, read back after, and undone after 15 seconds unless you keep it.
- Not offered yet: switching profiles, button bindings, battery.
- The ROG Omni receiver (`0b05:1ace`) and the Strix Impact III Wireless, which uses it, are left
  out: that receiver is shared with other devices and may carry a keyboard's typing.
- On Linux the udev rule opens, where a source says which, only the interface of the configuration
  collection; elsewhere the whole mouse, as for the Logitech mice.

| Mouse | USB ID | Opened on Linux | Notes |
|---|---|---|---|
| ROG GX860 Buzzard | `0b05:1816` | whole mouse | no lighting |
| ROG Chakram Core | `0b05:1958` | interface 0 | — |
| ROG Chakram | `0b05:18e3`, `0b05:18e5` | interface 0 | — |
| ROG Chakram X | `0b05:1a18`, `0b05:1a1a` | interface 0 | DPI shown, not changed (separate X and Y); LED brightness kept as it is |
| ROG Gladius II | `0b05:1845` | interface 2 | — |
| ROG Gladius II Origin | `0b05:1877` | interface 2 | — |
| ROG Gladius II Origin PNK LTD | `0b05:18cd` | interface 2 | — |
| ROG Harpe Wireless | `0b05:1a92` | whole mouse | — |
| ROG Keris | `0b05:195c` | interface 0 | — |
| ROG Keris Wireless AimPoint | `0b05:1a66`, `0b05:1a68` | interface 0 | — |
| ROG Keris Wireless | `0b05:195e`, `0b05:1960` | interface 0 | — |
| ROG Pugio | `0b05:1846` | interface 2 | — |
| ROG Spatha X | `0b05:1979`, `0b05:1977` | interface 0 | — |
| ROG Strix Carry | `0b05:18b4` | whole mouse | no lighting |
| ROG Strix Impact II | `0b05:18e1` | interface 0 | — |
| ROG Strix Impact II Wireless | `0b05:1947`, `0b05:1949` | interface 0 | — |
| ROG Strix Impact III | `0b05:1a88` | interface 0 | — |
| ROG Strix Impact | `0b05:1847` | interface 2 | — |
| TUF Gaming M3 Gen II | `0b05:1a9b` | interface 2 | — |
| TUF Gaming M4 Air | `0b05:1a03` | whole mouse | no lighting |
| TUF Gaming Mini WL Mouse Miku | `0b05:1c56`, `0b05:1c57` | whole mouse | DPI shown, not changed (separate X and Y); LED brightness kept as it is |

## SteelSeries mice

🧪 Experimental: 33 SteelSeries mice (Rival, Sensei, Aerox, Prime, Kana, Kinzu), on their cable and
through their own 2.4 GHz receiver. Nobody has tried PeriHub on one yet. Their commands come from
[rivalcfg](https://github.com/flozz/rivalcfg) (WTFPL), described in
[protocols/steelseries-mice.md](protocols/steelseries-mice.md); PeriHub sends the same bytes as
rivalcfg's own tests expect (616 reports compared).

- These mice **cannot report their settings**: PeriHub shows the firmware and, on wireless mice,
  the battery, and for each setting what it last sent.
- **Send** changes one setting (DPI presets, polling rate, colours, lighting effects, brightness,
  timers…) until the mouse is unplugged or turned off, under a warning that is always on screen.
  PeriHub **never saves to their memory** until someone has tested it: it could not back up or
  undo a save. Unplugging the mouse brings back the settings saved in it.
- Not offered yet: button mapping, colour gradients, reactive colours, separate X and Y DPI.

| Mouse | USB ID | Interface | Settings PeriHub sends |
|---|---|---|---|
| Aerox 3 | `1038:1836` | 3 | 8 |
| Aerox 3 Wireless | `1038:183a`, `1038:187a` | 3 | 8, battery |
| Aerox 5 | `1038:1850` | 3 | 8 |
| Aerox 5 Wireless | `1038:1854`, `1038:185e`, `1038:1862` | 3 | 8, battery |
| Aerox 9 Wireless | `1038:185a`, `1038:1876` | 3 | 8, battery |
| Kana v2 | `1038:137a` | 0 | 5 |
| Kinzu v2 | `1038:1366`, `1038:1378` | 0 | 3 |
| Prime | `1038:182e`, `1038:182a`, `1038:1856` | 0 | 4 |
| Prime Mini | `1038:184d` | 3 | 4 |
| Prime+ | `1038:182c` | 0 | 4 |
| Prime Wireless | `1038:1842`, `1038:184a` | 3 | 6, battery |
| Rival 100 / Rival 105 | `1038:1702`, `1038:170a`, `1038:170b`, `1038:170c`, `1038:1814` | 0 | 6 |
| Rival 110 / Rival 106 | `1038:1729`, `1038:1816` | 0 | 6 |
| Rival 3 | `1038:1824`, `1038:184c` | 3 | 7 |
| Rival 300 / Rival | `1038:1384`, `1038:1392`, `1038:1710`, `1038:1712`, `1038:171c`, `1038:1394`, `1038:171a`, `1038:1716`, `1038:1714`, `1038:1718` | 0 | 7 |
| Rival 300S | `1038:1810` | 0 | 6 |
| Rival 310 | `1038:1720`, `1038:171e`, `1038:1736` | 0 | 3 |
| Rival 3 Gen 2 | `1038:1870` | 3 | 7 |
| Rival 3 Wireless | `1038:1830` | 3 | 2, battery |
| Rival 3 Wireless Gen 2 | `1038:1872` | 3 | 1, battery |
| Rival 5 | `1038:183c`, `1038:183e` | 0 | 14 |
| Rival 500 | `1038:170e` | 0 | 3 |
| Rival 600 | `1038:1724`, `1038:172e` | 0 | 3 |
| Rival 650 Wireless | `1038:172b`, `1038:1726` | 0 | 5, battery |
| Rival 700 / Rival 710 | `1038:1700`, `1038:1730` | 0 | 3 |
| Rival 95 / Rival 100 PC Bang | `1038:1706`, `1038:1707`, `1038:1704`, `1038:1708` | 0 | 4 |
| Sensei 310 | `1038:1722` | 0 | 3 |
| Sensei [RAW] | `1038:1369`, `1038:1362`, `1038:136d`, `1038:136f`, `1038:1380`, `1038:1390` | 0 | 5 |
| Sensei TEN | `1038:1832`, `1038:1834` | 0 | 2 |
| Aerox 3 Wireless (through its receiver) | `1038:1838`, `1038:1878` | 3 | 8, battery |
| Aerox 5 Wireless (through its receiver) | `1038:1852`, `1038:185c`, `1038:1860` | 3 | 8, battery |
| Aerox 9 Wireless (through its receiver) | `1038:1858`, `1038:1874` | 3 | 8, battery |
| Prime Wireless (through its receiver) | `1038:1840`, `1038:1848` | 3 | 6, battery |

## Razer mice

🧪 Experimental: 75 Razer mice, on their USB cable. Nobody has tried PeriHub on one yet. Their protocol
is described in [protocols/razer-mice.md](protocols/razer-mice.md), from facts found in
[OpenRazer](https://github.com/openrazer/openrazer) (GPL-2.0): no code was copied.

- PeriHub reads the firmware, DPI and polling rate.
- **Save to memory** changes the DPI and polling rate, after the No warranty box the first time.
  These mice store every change by themselves, the way OpenRazer sends it, so there is no change
  that lasts only until they are unplugged: PeriHub saves the settings first, reads them back, and
  puts them back after 15 seconds unless you keep the change.
- Their wireless receivers and docks are left out, since some can be shared with a keyboard: use
  the USB cable. Not offered yet: DPI stages, lighting, buttons, battery.

| Mouse | USB ID | Interface | What PeriHub changes |
|---|---|---|---|
| Orochi 2011 | `1532:0013` | 0 | polling rate shown |
| Naga | `1532:0015` | 0 | polling rate |
| Naga Epic | `1532:001f` | 0 | polling rate |
| Abyssus 1800 | `1532:0020` | 0 | polling rate |
| Mamba 2012 (Wired) | `1532:0024` | 0 | DPI up to 6400, polling rate |
| Naga 2012 | `1532:002e` | 0 | polling rate |
| Imperator 2012 | `1532:002f` | 0 | DPI up to 6400, polling rate |
| Ouroboros | `1532:0032` | 0 | DPI up to 8200, polling rate |
| Taipan | `1532:0034` | 0 | DPI up to 8200, polling rate |
| Naga Hex (Red) | `1532:0036` | 0 | polling rate |
| DeathAdder 2013 | `1532:0037` | 0 | polling rate |
| DeathAdder 1800 | `1532:0038` | 0 | polling rate |
| Orochi 2013 | `1532:0039` | 0 | DPI up to 6400, polling rate |
| Naga Epic Chroma (Wired) | `1532:003e` | 0 | DPI up to 8200, polling rate |
| Naga 2014 | `1532:0040` | 0 | DPI up to 8200, polling rate |
| Naga Hex | `1532:0041` | 0 | polling rate |
| Abyssus | `1532:0042` | 0 | polling rate |
| DeathAdder Chroma | `1532:0043` | 0 | DPI up to 10000, polling rate |
| Mamba Chroma (Wired) | `1532:0044` | 0 | DPI up to 16000, polling rate shown |
| Mamba Tournament Edition | `1532:0046` | 0 | DPI up to 16000 |
| Orochi (Wired) | `1532:0048` | 0 | DPI up to 8200, polling rate |
| Diamondback Chroma | `1532:004c` | 0 | DPI up to 16000 |
| DeathAdder 2000 | `1532:004f` | 0 | DPI up to 2000, polling rate |
| Naga Hex V2 | `1532:0050` | 0 | DPI up to 16000, polling rate |
| Naga Chroma | `1532:0053` | 0 | DPI up to 16000, polling rate |
| DeathAdder 3500 | `1532:0054` | 0 | DPI up to 3500, polling rate |
| Lancehead (Wired) | `1532:0059` | 0 | DPI up to 16000, polling rate |
| Abyssus V2 | `1532:005b` | 0 | DPI up to 5000, polling rate |
| DeathAdder Elite | `1532:005c` | 0 | DPI up to 16000, polling rate |
| Abyssus 2000 | `1532:005e` | 0 | DPI up to 2000, polling rate |
| Lancehead Tournament Edition | `1532:0060` | 0 | DPI up to 16000, polling rate |
| Basilisk | `1532:0064` | 0 | DPI up to 16000, polling rate |
| Basilisk Essential | `1532:0065` | 0 | DPI up to 6400, polling rate |
| Naga Trinity | `1532:0067` | 0 | DPI up to 16000, polling rate |
| Abyssus Elite (D.Va Edition) | `1532:006a` | 0 | DPI up to 7200, polling rate |
| Abyssus Essential | `1532:006b` | 0 | DPI up to 7200, polling rate |
| Mamba Elite | `1532:006c` | 0 | DPI up to 16000, polling rate |
| DeathAdder Essential | `1532:006e` | 0 | DPI up to 6400, polling rate |
| Lancehead Wireless (Wired) | `1532:0070` | 0 | DPI up to 16000, polling rate |
| DeathAdder Essential (White Edition) | `1532:0071` | 0 | DPI up to 6400, polling rate |
| Mamba Wireless (Wired) | `1532:0073` | 0 | DPI up to 16000, polling rate |
| Viper | `1532:0078` | 0 | DPI up to 16000, polling rate |
| Viper Ultimate (Wired) | `1532:007a` | 0 | DPI up to 20000, polling rate |
| DeathAdder V2 Pro (Wired) | `1532:007c` | 0 | DPI up to 20000, polling rate |
| Pro Click (Wired) | `1532:0080` | 0 | DPI up to 16000, polling rate |
| DeathAdder V2 | `1532:0084` | 0 | DPI up to 20000, polling rate |
| Basilisk V2 | `1532:0085` | 0 | DPI up to 20000, polling rate |
| Basilisk Ultimate | `1532:0086` | 0 | DPI up to 20000, polling rate shown |
| Viper Mini | `1532:008a` | 0 | DPI up to 8500, polling rate |
| DeathAdder V2 Mini | `1532:008c` | 0 | DPI up to 8500, polling rate |
| Naga Left Handed Edition 2020 | `1532:008d` | 0 | DPI up to 20000, polling rate |
| Naga Pro (Wired) | `1532:008f` | 0 | DPI up to 20000, polling rate |
| Viper 8KHz | `1532:0091` | 0 | DPI up to 20000 |
| Naga X | `1532:0096` | 3 | DPI up to 18000, polling rate |
| DeathAdder Essential (2021) | `1532:0098` | 0 | DPI up to 6400, polling rate |
| Basilisk V3 | `1532:0099` | 3 | DPI up to 26000, polling rate |
| Viper Mini SE (Wired) | `1532:009e` | 0 | DPI up to 30000 |
| DeathAdder V2 Lite | `1532:00a1` | 0 | DPI up to 8500, polling rate |
| Cobra | `1532:00a3` | 0 | DPI up to 8500, polling rate |
| Viper V2 Pro (Wired) | `1532:00a5` | 0 | DPI up to 30000, polling rate |
| Naga V2 Pro (Wired) | `1532:00a7` | 0 | DPI up to 30000, polling rate |
| Basilisk V3 Pro (Wired) | `1532:00aa` | 0 | DPI up to 30000, polling rate |
| Cobra Pro (Wired) | `1532:00af` | 0 | DPI up to 30000, polling rate |
| DeathAdder V3 | `1532:00b2` | 0 | DPI up to 30000 |
| DeathAdder V3 Pro (Wired) | `1532:00b6` | 0 | DPI up to 35000, polling rate |
| DeathAdder V4 Pro (Wired) | `1532:00be` | 0 | DPI up to 45000 |
| Viper V3 Pro (Wired) | `1532:00c0` | 0 | DPI up to 35000, polling rate |
| DeathAdder V3 Pro (Wired) | `1532:00c2` | 0 | DPI up to 35000, polling rate |
| Pro Click V2 Vertical Edition (Wired) | `1532:00c7` | 0 | DPI up to 30000, polling rate |
| Basilisk V3 35K | `1532:00cb` | 3 | DPI up to 35000, polling rate |
| Basilisk V3 Pro 35K (Wired) | `1532:00cc` | 0 | DPI up to 35000, polling rate |
| Pro Click V2 (Wired) | `1532:00d0` | 0 | DPI up to 30000, polling rate |
| Basilisk Mobile (Wired) | `1532:00d3` | 0 | DPI up to 18000, polling rate |
| Basilisk V3 Pro 35K Phantom Green Edition (Wired) | `1532:00d6` | 0 | DPI up to 35000, polling rate |
| Viper V3 Pro SE (Wired) | `1532:00de` | 0 | DPI up to 35000, polling rate |

## Glorious mice

🧪 Experimental: 3 Glorious mice with a cable, built on a SinoWealth controller. Nobody has tried PeriHub
on one yet. Their protocol comes from [libratbag](https://github.com/libratbag/libratbag) (MIT), and is
described in [protocols/sinowealth-glorious.md](protocols/sinowealth-glorious.md).

- PeriHub reads the firmware, the DPI of each slot that is on, the slot in use and the polling rate.
- **Save to memory** changes the DPI of those slots and the polling rate, after the No warranty box the
  first time. These mice have no change that lasts only until they are unplugged: every change rewrites
  their settings in memory. PeriHub reads them again just before, changes only those values, reads them
  back after, and puts them back after 15 seconds unless you keep the change. It never writes on top of a
  reading that differs from the one taken when the mouse was connected anywhere but in those values and the
  slot in use.
- **On Windows they are read-only for now**: there the mouse's settings may arrive padded to 520 bytes,
  and then PeriHub cannot tell how long they are, which a write must say.
- PeriHub reads only the firmware of a mouse whose firmware is not the one below, as libratbag does.
- The generic SinoWealth ids libratbag also lists (`258a:0027`, `0029`, `0051`, `1007`) are left out: mice
  of other brands share them, some with another protocol. So are the wireless Model O and
  Model D, which speak another protocol. Not offered yet: lighting, buttons, debounce time.
- On Linux the udev rule opens the whole mouse: no source names the interface of the configuration.

| Mouse | USB ID | Firmware | Opened on Linux |
|---|---|---|---|
| Model O/O- | `258a:0036` | `V103` | whole mouse |
| Model D | `258a:0033` | `V102` | whole mouse |
| Model O Eternal | `3794:a000` | `1009` | whole mouse |

## Keyboards with QMK firmware (Vial, VIA)

🧪 Experimental: nobody has tried PeriHub on one yet. The protocols come from facts found in the QMK
and Vial sources, described in [protocols/qmk-via-vial.md](protocols/qmk-via-vial.md); no code was
copied.

These keyboards have no USB ids of their own: every keyboard has its own. PeriHub finds them by the
raw HID channel QMK gives them for VIA and Vial (usage page `0xFF60`, usage `0x61`), and lists them
on the **QMK keyboards** page.

| Keyboard | How PeriHub knows it | What PeriHub does |
|---|---|---|
| Vial firmware | its USB serial number holds `vial:f64c2b3c` | reads it by itself: Vial and VIA protocol, layers, macros; with VialRGB, lighting (effect by name, speed, colour, brightness in steps of 5 %) |
| Any other keyboard with the raw HID channel | nothing: it may be VIA, or firmware that answers that channel its own way | nothing until you turn it on as a VIA keyboard; then VIA protocol, layers, macros; with VIA protocol 12 or 13, lighting (per-key RGB, RGB light, per-key light: off or the effect in use, speed, colour, brightness in steps of 10 %) |
| Vial's bootloader (serial `vibl:d4f8159c`) | | never opened |

- **Apply** changes the lighting without storing it; **Save to memory** stores it, after the No warranty
  box the first time. Both are read back and undone after 15 seconds unless kept.
- Never sent: keymaps, macros, encoders, layout options, the keymap, macro and memory resets, the
  bootloader jump, the state of the key matrix, Vial's unlock, the keyboard's own custom values, and the
  backlight (its breathing setting writes the memory at once).
- On Linux a keyboard needs a line in the optional rule `74-perihub-qmk.rules`, which PeriHub writes
  from the **QMK keyboards** page: it opens only the interface whose descriptor holds the raw HID
  collection and nothing else, checked in `/sys` first.

## Something else?

Open a [device request](https://github.com/hex-dump/perihub/issues/new?template=device_request.yml)
with the model and USB ID. Any brand can be requested.
