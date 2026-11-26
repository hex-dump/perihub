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
- [Logitech keyboards on their own USB connection](#logitech-keyboards-on-their-own-usb-connection)
- [Corsair](#corsair)
- [ASUS](#asus)
- [SteelSeries mice](#steelseries-mice)
- [SteelSeries Apex Pro, Apex 7, Apex 7 TKL and Apex 5](#steelseries-apex-pro-apex-7-apex-7-tkl-and-apex-5)
- [Razer mice](#razer-mice)
- [Razer keyboards](#razer-keyboards)
- [Glorious mice](#glorious-mice)
- [Logitech G213 Prodigy](#logitech-g213-prodigy)
- [Logitech G600](#logitech-g600)
- [Older Logitech mice: G9, G9x, G500, G500s, G700, G700s](#older-logitech-mice-g9-g9x-g500-g500s-g700-g700s)
- [Roccat mice](#roccat-mice)
- [Pulsar mice](#pulsar-mice)
- [Pulsar CrazyLight and Lamzu Atlantis](#pulsar-crazylight-and-lamzu-atlantis)
- [Cooler Master mice](#cooler-master-mice)
- [EVGA mice](#evga-mice)
- [YJX mice](#yjx-mice)
- [Wraith W60 and Artery W669 keyboards](#wraith-w60)
- [EVision keyboards](#evision-keyboards)
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
- **Logitech G502 X Wireless** (`046d:c098`): ✅ supported with the USB cable on its tested memory format
  (format 3 in 255-byte sectors, [issue #15](https://github.com/hex-dump/perihub/issues/15)). A firmware
  update that moves to an untested memory format (such as profile format 10 on newer 44K firmware) stays read-only
  until that format is added.
- **Every other mouse below**: the level depends on the format of the mouse's onboard memory,
  which PeriHub reads from the mouse every time it connects. Many models share a format, and the
  same model can change format with a firmware update, so the level is not fixed per model:
  - ✅ **a tested format**: profile format 2 in 256-byte sectors (the G502 HERO's) or format 3 in
    255-byte sectors (the G502 X Wireless's). The mouse is supported, whatever the model.
  - 🧪 **a format PeriHub can write but nobody has tried yet** (format 2, 3 or 4, in 255- or 256-byte
    sectors; format 4 is the G903 HERO's, with the layout of 2 and 3 where PeriHub writes, see
    [protocols/logitech-onboard-profiles.md](protocols/logitech-onboard-profiles.md)): you can save at your own risk with the USB cable. Before the first save to that
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
  `c53d`, `c53f`, `c541`, `c543`, `c545`, `c547` or `c54d`). With a tested memory format PeriHub
  switches the profile and DPI level, and saves over the radio as G HUB does, as an experimental
  feature (No warranty box, 15-second undo, not below 15% battery); otherwise it only reads. A G903 HERO
  was reached and read through a `c539` receiver on Linux and Windows
  ([issues #22](https://github.com/hex-dump/perihub/issues/22) and [#115](https://github.com/hex-dump/perihub/issues/115)), and a G502 HERO Wireless through a
  POWERPLAY mat (`c53a`) on Windows was read and saved to ([issues #4](https://github.com/hex-dump/perihub/issues/4)
  and [#28](https://github.com/hex-dump/perihub/issues/28)): the only mouse saved to over the radio so far.
- **The G604** (`4085` through its receiver) is also recognized over **Bluetooth** (`046d:b024`, issue
  [#43](https://github.com/hex-dump/perihub/issues/43)): PeriHub reads its profiles, buttons, DPI,
  polling rate and battery there, and on a tested memory format (the G604's is one) switches profiles and
  DPI levels and saves profiles over Bluetooth as an experimental feature, as through a receiver: the
  **No warranty** box first, a backup, every sector read back, the 15-second undo, no save below 15%
  battery, the profile list left alone. Any other memory format, and the command line, only read it. On Linux a udev rule opens the mouse's Bluetooth node (`KERNELS=="0005:046D:B024.*"`). Through the
  receiver a G604 follows its memory format (format 3 in 255-byte sectors, as its own answers say, like
  the G502 X Wireless's): see [protocols/logitech-onboard-profiles.md](protocols/logitech-onboard-profiles.md).
- **PRO 2 LIGHTSPEED and PRO X 2 DEX** (profile format 7) are recognized, with a cable and through
  their LIGHTSPEED receiver (`c543`, `c54d`), and only read. Format 7 is a layout PeriHub cannot
  write yet, and these mice have neither the usual adjustable DPI (`0x2201`) nor the usual polling
  rate (`0x8060`) feature: they use the extended ones (`0x2202`, `0x8061`), which PeriHub reads (DPI
  ranges of both axes and lift-off distance, DPI in use, polling rates up to 8000 Hz and the one in
  use) and shows in the test report, together with each profile as stored. PeriHub never switches
  their onboard mode: another project found that a PRO X 2 loses its button assignments when it
  does. Nothing is changed and nothing is saved until testers confirm the readings against G HUB
  ([issues #32](https://github.com/hex-dump/perihub/issues/32) and
  [#33](https://github.com/hex-dump/perihub/issues/33)).
- **Lights through `0x8071`** (the G903 HERO, the PRO 2 LIGHTSPEED): 👁 a mouse with `0x8071` and no `0x8070` gets
  its lighting zones read and shown (where each one is, the effects it offers, and what the profile's two LED blocks
  hold), and they are never changed: the sources disagree on which LED block of a profile drives which zone, and
  PeriHub sends `0x8071` reads only. A save leaves the profile's lighting as it is.
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
  - 👁 **Lighting**, on a device with the HID++ lighting features of the newer keyboards (0x8040
    brightness, 0x8071 effects by zone, 0x8081 per-key colours, 0x4540 layout): the brightness now, who
    drives the lights (the device or a program such as G HUB), the effects each zone offers, how many
    keys have their own colour, the startup and shutdown effects and when the lights dim. The test report
    carries every reply. The first one is a G915 X LS through a `c547` receiver on Windows
    ([issue #30](https://github.com/hex-dump/perihub/issues/30)), see
    [protocols/logitech-keyboard-lighting.md](protocols/logitech-keyboard-lighting.md).
  - 🧪 **Lights driven by PeriHub (experimental)**, on a keyboard that also has 0x8101 profile management:
    an effect per zone, colours per group of keys and the brightness, in host mode as G HUB does with its
    on-board memory mode off. Everything but the brightness stays in the keyboard's RAM; PeriHub dims and
    turns the lights off when the keyboard is idle, and gives them back on request, from the tray, when
    G HUB or Solaar starts and when it quits. Once asked, it takes them again at every start and when
    G HUB or Solaar quit, until you give them back. Tried on a G915 X LS ([issue #30](https://github.com/hex-dump/perihub/issues/30));
    **No warranty** box first.
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

<!-- generated: table logitech-onboard -->
| Mouse | USB ID | Opened on Linux | Level | Through a receiver |
|---|---|---|---|---|
| G502 HERO | `046d:c08b` | whole mouse | ✅ Supported |  |
| G502 Proteus Core | `046d:c07d` | whole mouse | depends on its memory format |  |
| G502 Proteus Spectrum | `046d:c332` | whole mouse | depends on its memory format |  |
| G502 HERO Wireless | `046d:c08d` | whole mouse | depends on its memory format | `407f` |
| G502 X | `046d:c099` | whole mouse | depends on its memory format |  |
| G502 X Wireless | `046d:c098` | whole mouse | ✅ Supported | `409f` |
| G502 X PLUS | `046d:c095` | whole mouse | depends on its memory format | `4099` |
| G Pro | `046d:c085`, `046d:c08c` | whole mouse | depends on its memory format |  |
| G Pro Wireless | `046d:c088` | whole mouse | depends on its memory format | `4079` |
| G Pro X Superlight | `046d:c094` | whole mouse | depends on its memory format | `4093` |
| G102/G103/G203 | `046d:c084`, `046d:c092`, `046d:c09d` | whole mouse | depends on its memory format |  |
| G302 | `046d:c07f` | whole mouse | depends on its memory format |  |
| G303 | `046d:c080` | whole mouse | depends on its memory format |  |
| G303 Shroud Edition | `046d:c097` | whole mouse | depends on its memory format | `409e` |
| G402 | `046d:c07e` | whole mouse | depends on its memory format |  |
| G403 | `046d:c083` | whole mouse | depends on its memory format |  |
| G403 HERO | `046d:c08f` | whole mouse | depends on its memory format |  |
| G403 Wireless | `046d:c082` | whole mouse | depends on its memory format | `405d` |
| G703 | `046d:c087` | whole mouse | depends on its memory format | `4070` |
| G703 HERO | `046d:c090` | whole mouse | depends on its memory format | `4086` |
| G705 | `046d:c096` | whole mouse | depends on its memory format | `409d` |
| G900 | `046d:c081` | whole mouse | depends on its memory format | `4053` |
| G903 | `046d:c086` | whole mouse | depends on its memory format | `4067` |
| G903 HERO | `046d:c091` | whole mouse | depends on its memory format | `4087` |
| MX518 | `046d:c08e` | whole mouse | depends on its memory format |  |
| PRO 2 LIGHTSPEED | `046d:c09a` | whole mouse | depends on its memory format | `40a8` |
| PRO X 2 DEX | `046d:c0a0` | whole mouse | depends on its memory format | `40b8` |
| G305 | wireless only | through a receiver | depends on its memory format | `4074` |
| G603 | wireless only | through a receiver | depends on its memory format | `406c` |
| G604 | wireless only, Bluetooth `046d:b024` | through a receiver, or its Bluetooth node (whole mouse) | depends on its memory format | `4085` |
<!-- end generated -->

**Memory formats known so far.** What other projects found, for mice PeriHub has not seen yet:
the format a mouse reports in PeriHub's test report is what counts.

| Mouse | Profile format | Sector size | Source |
|---|---|---|---|
| G502 HERO | 2 | 256 bytes, 255 on newer firmware | PeriHub ([issue #20](https://github.com/hex-dump/perihub/issues/20)) |
| G502 X Wireless | 3 | 255 bytes | PeriHub ([issue #15](https://github.com/hex-dump/perihub/issues/15)) |
| G900 | 3 | | libratbag ([PR #1676](https://github.com/libratbag/libratbag/pull/1676)) |
| G203 | 4 | | libratbag ([PR #1676](https://github.com/libratbag/libratbag/pull/1676)) |
| G303 Shroud Edition | 3 | 255 bytes | PeriHub ([issue #34](https://github.com/hex-dump/perihub/issues/34)) |
| G903 HERO | 4 | 255 bytes | PeriHub ([issue #22](https://github.com/hex-dump/perihub/issues/22)) |
| G502 X PLUS | 5 | | libratbag ([issue #1344](https://github.com/libratbag/libratbag/issues/1344)) |
| PRO 2 LIGHTSPEED | 7 | 255 bytes | PeriHub ([issue #33](https://github.com/hex-dump/perihub/issues/33)) |
| PRO X 2 DEX | 7 | 255 bytes | PeriHub ([issue #32](https://github.com/hex-dump/perihub/issues/32)) |
| G Pro X Superlight 2 (not recognized yet) | 6, then 7 after a firmware update | | libratbag ([issue #1572](https://github.com/libratbag/libratbag/issues/1572)) |

Own one? Send a [test report](https://github.com/hex-dump/perihub/issues/new?template=01_test_report.yml)
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
[test report](https://github.com/hex-dump/perihub/issues/new?template=01_test_report.yml).

## Logitech keyboards on their own USB connection

👁 Read-only: Logitech devices with HID++ 2.0 on a USB connection of their own, without a receiver,
whose HID++ collections use the newer vendor usage page `ff43`
([how PeriHub finds them](protocols/logitech-usb-hidpp.md)). Nobody has tried PeriHub on one yet.
Each gets the page of a device PeriHub knows by its features: its name, firmware, the features it
lists, its battery if it has one, and the lighting it reports (zones and effects, brightness, who
drives the lights). Nothing is changed, not even in RAM: the DPI and **Drive the lights** are not
offered, and the connection refuses anything but reads. The test report has every answer as it came.

- **PRO K/DA** keyboard ([issue #40](https://github.com/hex-dump/perihub/issues/40)): on Windows
  only. Its HID++ collections share USB interface 1 with what is typed on it, so on Linux, where
  one device node holds both, PeriHub neither opens it nor asks for a udev rule.

<!-- generated: table logitech-direct -->
| Keyboard | USB ID | Opened on Linux |
|---|---|---|
| PRO K/DA | `046d:c339` | — |
<!-- end generated -->

## Corsair

🧪 Experimental: <!-- generated: count corsair-mouse -->26<!-- end generated --> mice and <!-- generated: count corsair-keyboard -->23<!-- end generated --> keyboards of Corsair's newer generation, on their USB cable, and
the wireless ones also through their receiver (Slipstream `1ba6`, `1bdc`, `2b00`, and the older
HARPOON, DARK CORE RGB PRO and IRONCLAW receivers; not yet the Slipstream V2).

- PeriHub reads the firmware, polling rate and DPI.
- **Experimental lighting**, for the session only: one colour on every LED or key, at your own risk.
  The device goes back to its own settings when you turn it off, when PeriHub quits and as soon as
  anything fails. Nothing is written to its memory.
- With more than one device on the same receiver, PeriHub shows the first one only, for now.
- Through a receiver PeriHub also reads the battery level.
- **KATAR PRO WIRELESS** (receiver `1b1c:1b94`): read only, through its own receiver (it has no USB
  data cable). No lighting until someone tests it.

**Mice:**
<!-- generated: names corsair-mouse -->
KATAR PRO, KATAR PRO XT, M55 RGB PRO, M65 RGB ULTRA, M75, SABRE RGB PRO, SABRE PRO CS,
IRONCLAW RGB WIRELESS, IRONCLAW WIRELESS SE, DARK CORE RGB PRO, DARK CORE RGB PRO SE,
HARPOON RGB WIRELESS, HARPOON V2, M65 RGB ULTRA WIRELESS, M75 WIRELESS, M75 AIR WIRELESS,
NIGHTSABRE WIRELESS, SCIMITAR RGB ELITE WIRELESS, SCIMITAR ELITE WIRELESS SE,
SABRE RGB PRO WIRELESS, DARKSTAR RGB WIRELESS, SABRE V2 PRO, SABRE V2 PRO MG, SABRE V2 PRO CF,
SCIMITAR RGB ELITE, M55.
<!-- end generated -->

**Keyboards:**
<!-- generated: names corsair-keyboard -->
K55 PRO RGB, K55 RGB PRO XT, K55 CORE RGB, K55 CORE TKL RGB, K57 RGB WIRELESS, K60 RGB PRO,
K60 RGB PRO LP, K65 PLUS WIRELESS, K65 PRO MINI, K65 RGB MINI, K70 CORE RGB, K70 CORE TKL,
K70 CORE TKL WIRELESS, K70 MAX, K70 PRO, K70 PRO MINI WIRELESS, K70 PRO TKL, K70 RGB TKL,
K70 RGB TKL CS, K95 RGB PLATINUM XT, K100 RGB, K100 AIR WIRELESS, MAKR 75.
<!-- end generated -->

USB IDs, packet sizes and LED counts are in [protocols/corsair-bragi.md](protocols/corsair-bragi.md).
Older Corsair devices (K70 RGB MK.2, K95 RGB PLATINUM, M65 RGB ELITE and others) speak an older
protocol and are not recognized.

Own one? Send a [test report](https://github.com/hex-dump/perihub/issues/new?template=01_test_report.yml)
(**Open test form** on the device page fills it in).

## ASUS

🧪 Experimental: <!-- generated: count asus -->21<!-- end generated --> ASUS ROG and TUF mice, on their USB cable and through their own wireless
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

<!-- generated: table asus -->
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
<!-- end generated -->

## SteelSeries mice

🧪 Experimental: <!-- generated: count steelseries-mice -->33<!-- end generated --> SteelSeries mice (Rival, Sensei, Aerox, Prime, Kana, Kinzu), on their cable and
through their own 2.4 GHz receiver. One Rival 5 (cable, Windows, issue #71) was tried: its name, finding it again after unplugging, DPI, polling rate, lighting, and changes taking effect and still there after unplugging all worked; nothing else has been tried, so they stay experimental. Their commands come from
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

<!-- generated: table steelseries-mice -->
| Mouse | USB ID | Opened on Linux | Level | Settings PeriHub sends |
|---|---|---|---|---|
| Aerox 3 | `1038:1836` | interface 3 | 🧪 Experimental | 8 |
| Aerox 3 Wireless | `1038:183a`, `1038:187a` | interface 3 | 🧪 Experimental | 8, battery |
| Aerox 5 | `1038:1850` | interface 3 | 🧪 Experimental | 8 |
| Aerox 5 Wireless | `1038:1854`, `1038:185e`, `1038:1862` | interface 3 | 🧪 Experimental | 8, battery |
| Aerox 9 Wireless | `1038:185a`, `1038:1876` | interface 3 | 🧪 Experimental | 8, battery |
| Kana v2 | `1038:137a` | interface 0 | 🧪 Experimental | 5 |
| Kinzu v2 | `1038:1366`, `1038:1378` | interface 0 | 🧪 Experimental | 3 |
| Prime | `1038:182e`, `1038:182a`, `1038:1856` | interface 0 | 🧪 Experimental | 4 |
| Prime Mini | `1038:184d` | interface 3 | 🧪 Experimental | 4 |
| Prime+ | `1038:182c` | interface 0 | 🧪 Experimental | 4 |
| Prime Wireless | `1038:1842`, `1038:184a` | interface 3 | 🧪 Experimental | 6, battery |
| Rival 100 / Rival 105 | `1038:1702`, `1038:170a`, `1038:170b`, `1038:170c`, `1038:1814` | interface 0 | 🧪 Experimental | 6 |
| Rival 110 / Rival 106 | `1038:1729`, `1038:1816` | interface 0 | 🧪 Experimental | 6 |
| Rival 3 | `1038:1824`, `1038:184c` | interface 3 | 🧪 Experimental | 7 |
| Rival 300 / Rival | `1038:1384`, `1038:1392`, `1038:1710`, `1038:1712`, `1038:171c`, `1038:1394`, `1038:171a`, `1038:1716`, `1038:1714`, `1038:1718` | interface 0 | 🧪 Experimental | 7 |
| Rival 300S | `1038:1810` | interface 0 | 🧪 Experimental | 6 |
| Rival 310 | `1038:1720`, `1038:171e`, `1038:1736` | interface 0 | 🧪 Experimental | 3 |
| Rival 3 Gen 2 | `1038:1870` | interface 3 | 🧪 Experimental | 7 |
| Rival 3 Wireless | `1038:1830` | interface 3 | 🧪 Experimental | 2, battery |
| Rival 3 Wireless Gen 2 | `1038:1872` | interface 3 | 🧪 Experimental | 1, battery |
| Rival 5 | `1038:183c`, `1038:183e` | interface 0 | 🧪 Experimental | 14 |
| Rival 500 | `1038:170e` | interface 0 | 🧪 Experimental | 3 |
| Rival 600 | `1038:1724`, `1038:172e` | interface 0 | 🧪 Experimental | 3 |
| Rival 650 Wireless | `1038:172b`, `1038:1726` | interface 0 | 🧪 Experimental | 5, battery |
| Rival 700 / Rival 710 | `1038:1700`, `1038:1730` | interface 0 | ✅ Supported | 3 |
| Rival 95 / Rival 100 PC Bang | `1038:1706`, `1038:1707`, `1038:1704`, `1038:1708` | interface 0 | 🧪 Experimental | 4 |
| Sensei 310 | `1038:1722` | interface 0 | 🧪 Experimental | 3 |
| Sensei [RAW] | `1038:1369`, `1038:1362`, `1038:136d`, `1038:136f`, `1038:1380`, `1038:1390` | interface 0 | 🧪 Experimental | 5 |
| Sensei TEN | `1038:1832`, `1038:1834` | interface 0 | 🧪 Experimental | 2 |
| Aerox 3 Wireless (through its receiver) | `1038:1838`, `1038:1878` | interface 3 | 🧪 Experimental | 8, battery |
| Aerox 5 Wireless (through its receiver) | `1038:1852`, `1038:185c`, `1038:1860` | interface 3 | 🧪 Experimental | 8, battery |
| Aerox 9 Wireless (through its receiver) | `1038:1858`, `1038:1874` | interface 3 | 🧪 Experimental | 8, battery |
| Prime Wireless (through its receiver) | `1038:1840`, `1038:1848` | interface 3 | 🧪 Experimental | 6, battery |
<!-- end generated -->

## SteelSeries Apex Pro, Apex 7, Apex 7 TKL and Apex 5

🧪 Experimental: nobody has tried PeriHub on one yet. The first request is [issue #111](https://github.com/hex-dump/perihub/issues/111),
an Apex Pro (`1038:1610`) on Windows. The protocol is described in [protocols/steelseries-apex.md](protocols/steelseries-apex.md) from
facts found in [OpenRGB](https://gitlab.com/CalcProgrammer1/OpenRGB) (GPL-2.0-or-later; no code was copied) and, for the display,
[apex-tux](https://github.com/not-jan/apex-tux) (Unlicense). The four keyboards share one command set on the vendor collection of USB
interface 1 (usage page `ffc0`), the only interface PeriHub opens: the others carry what is typed.

- PeriHub reads the keyboard's and the LED controller's firmware and the **brightness**. An answer that does not look as documented
  leaves the keyboard read-only, with the reason.
- **Brightness** (tens of percent) can be changed as an experimental feature, with a backup, a read-back and the 15-second undo.
  Nothing is saved to the keyboard.
- **Colours**: one colour for the whole keyboard, or one for each group of keys (letters, number row, F1 to F12, navigation keys and
  arrows, number pad, modifiers, the other keys), sent as one frame of the 112 keys OpenRGB lists, and **back to the keyboard's own
  lighting**. The keyboard cannot report what it shows, so these are sent one at a time under a warning, cannot be read back or
  undone, and last until the keyboard is told to go back or is unplugged.
- **The display** (OLED): clear it, light every pixel, or a test pattern.
- Left out, waiting for a recording of SteelSeries GG made by a tester (docs/captures.md): per-key actuation points (OmniPoint),
  key remapping, macros, onboard profiles and the lighting effects of the keyboard's own profiles, which no public source documents,
  and a picture of your own on the display.
- The Apex Pro TKL (`1614`), the Apex Pro 3 (`1640`), the Apex 9 and the 2023 and Gen 3 models are not added: OpenRGB gives them
  other commands, or commands that depend on the firmware (see the protocol page).

<!-- generated: table steelseries-apex -->
| Keyboard | USB ID | Opened on Linux |
|---|---|---|
| Apex Pro | `1038:1610` | interface 1 |
| Apex 7 | `1038:1612` | interface 1 |
| Apex 7 TKL | `1038:1618` | interface 1 |
| Apex 5 | `1038:161c` | interface 1 |
<!-- end generated -->

## Razer mice

🧪 Experimental: <!-- generated: count razer -->76<!-- end generated --> Razer mice, on their USB cable. Nobody has tried PeriHub on one yet. Their protocol
is described in [protocols/razer-mice.md](protocols/razer-mice.md), from facts found in
[OpenRazer](https://github.com/openrazer/openrazer) (GPL-2.0): no code was copied.

- PeriHub reads the firmware, DPI and polling rate, and on 47 of them the brightness of each light
  (logo, scroll wheel, sides, backlight).
- **Save to memory** changes the DPI, polling rate and brightness, after the No warranty box the
  first time. These mice store every change by themselves, the way OpenRazer sends it, so there is
  no change that lasts only until they are unplugged: PeriHub saves the settings first, reads them
  back, and puts them back after 15 seconds unless you keep the change.
- **One colour, not saved** (<!-- generated: count colour -->27<!-- end generated --> mice): **Send** shows one colour on every column of the mouse's custom
  frame, sent the way OpenRazer sends it, without saving it to the mouse's memory. Which lights the frame
  covers has not been checked: on some mice OpenRazer gives it fewer columns than they have lights. The lighting effect of
  these mice cannot be read back, so PeriHub cannot show it, check the colour or put the effect from
  before back, and the warning stays on screen. Nobody has checked yet how long the colour lasts or
  what comes back after the mouse is unplugged. PeriHub never stores a lighting effect on them.
- Their wireless receivers and docks are left out, since some can be shared with a keyboard: use
  the USB cable. Not offered yet: DPI stages, lighting effects, buttons, battery.

<!-- generated: table razer -->
| Mouse | USB ID | Opened on Linux | What PeriHub changes |
|---|---|---|---|
| Orochi 2011 | `1532:0013` | interface 0 | polling rate shown |
| Naga | `1532:0015` | interface 0 | polling rate |
| Naga Epic | `1532:001f` | interface 0 | polling rate; brightness of scroll wheel; battery shown |
| Abyssus 1800 | `1532:0020` | interface 0 | polling rate |
| Mamba 2012 (Wired) | `1532:0024` | interface 0 | DPI up to 6400, polling rate; brightness of scroll wheel; battery shown |
| Naga 2012 | `1532:002e` | interface 0 | polling rate |
| Imperator 2012 | `1532:002f` | interface 0 | DPI up to 6400, polling rate |
| Ouroboros | `1532:0032` | interface 0 | DPI up to 8200, polling rate; brightness of scroll wheel; battery shown |
| Taipan | `1532:0034` | interface 0 | DPI up to 8200, polling rate |
| Naga Hex (Red) | `1532:0036` | interface 0 | polling rate |
| DeathAdder 2013 | `1532:0037` | interface 0 | polling rate |
| DeathAdder 1800 | `1532:0038` | interface 0 | polling rate |
| Orochi 2013 | `1532:0039` | interface 0 | DPI up to 6400, polling rate |
| Naga Epic Chroma (Wired) | `1532:003e` | interface 0 | DPI up to 8200, polling rate; brightness of scroll wheel and backlight; battery shown |
| Naga 2014 | `1532:0040` | interface 0 | DPI up to 8200, polling rate |
| Naga Hex | `1532:0041` | interface 0 | polling rate |
| Abyssus | `1532:0042` | interface 0 | polling rate |
| DeathAdder Chroma | `1532:0043` | interface 0 | DPI up to 10000, polling rate; brightness of logo and scroll wheel |
| Mamba Chroma (Wired) | `1532:0044` | interface 0 | DPI up to 16000, polling rate shown; battery shown |
| Mamba Tournament Edition | `1532:0046` | interface 0 | DPI up to 16000; brightness of backlight |
| Orochi (Wired) | `1532:0048` | interface 0 | DPI up to 8200, polling rate; brightness of scroll wheel |
| Diamondback Chroma | `1532:004c` | interface 0 | DPI up to 16000; brightness of backlight |
| DeathAdder 2000 | `1532:004f` | interface 0 | DPI up to 2000, polling rate; brightness of logo and scroll wheel |
| Naga Hex V2 | `1532:0050` | interface 0 | DPI up to 16000, polling rate; brightness of logo, scroll wheel and backlight |
| Naga Chroma | `1532:0053` | interface 0 | DPI up to 16000, polling rate; brightness of logo, scroll wheel and backlight; one colour, not saved |
| DeathAdder 3500 | `1532:0054` | interface 0 | DPI up to 3500, polling rate; brightness of logo and scroll wheel |
| Lancehead (Wired) | `1532:0059` | interface 0 | DPI up to 16000, polling rate; brightness of logo, scroll wheel, left side and right side; one colour, not saved; battery shown |
| Abyssus V2 | `1532:005b` | interface 0 | DPI up to 5000, polling rate; brightness of logo and scroll wheel |
| DeathAdder Elite | `1532:005c` | interface 0 | DPI up to 16000, polling rate; brightness of logo and scroll wheel; one colour, not saved |
| Abyssus 2000 | `1532:005e` | interface 0 | DPI up to 2000, polling rate |
| Lancehead Tournament Edition | `1532:0060` | interface 0 | DPI up to 16000, polling rate; brightness of logo, scroll wheel, left side and right side; one colour, not saved |
| Basilisk | `1532:0064` | interface 0 | DPI up to 16000, polling rate; brightness of logo and scroll wheel; one colour, not saved |
| Basilisk Essential | `1532:0065` | interface 0 | DPI up to 6400, polling rate; brightness of logo; one colour, not saved |
| Naga Trinity | `1532:0067` | interface 0 | DPI up to 16000, polling rate |
| Abyssus Elite (D.Va Edition) | `1532:006a` | interface 0 | DPI up to 7200, polling rate; brightness of logo |
| Abyssus Essential | `1532:006b` | interface 0 | DPI up to 7200, polling rate; brightness of logo |
| Mamba Elite | `1532:006c` | interface 0 | DPI up to 16000, polling rate; brightness of logo, scroll wheel, left side and right side; one colour, not saved |
| DeathAdder Essential | `1532:006e` | interface 0 | DPI up to 6400, polling rate; brightness of logo and scroll wheel |
| Lancehead Wireless (Wired) | `1532:0070` | interface 0 | DPI up to 16000, polling rate; brightness of logo, scroll wheel, left side and right side; one colour, not saved; battery shown |
| DeathAdder Essential (White Edition) | `1532:0071` | interface 0 | DPI up to 6400, polling rate; brightness of logo and scroll wheel |
| Mamba Wireless (Wired) | `1532:0073` | interface 0 | DPI up to 16000, polling rate; brightness of logo and scroll wheel; one colour, not saved; battery shown |
| Viper | `1532:0078` | interface 0 | DPI up to 16000, polling rate; brightness of logo; one colour, not saved |
| Viper Ultimate (Wired) | `1532:007a` | interface 0 | DPI up to 20000, polling rate; brightness of logo; one colour, not saved; battery shown |
| DeathAdder V2 Pro (Wired) | `1532:007c` | interface 0 | DPI up to 20000, polling rate; brightness of logo; one colour, not saved; battery shown |
| Pro Click (Wired) | `1532:0080` | interface 0 | DPI up to 16000, polling rate; battery shown |
| DeathAdder V2 | `1532:0084` | interface 0 | DPI up to 20000, polling rate; brightness of logo and scroll wheel; one colour, not saved |
| Basilisk V2 | `1532:0085` | interface 0 | DPI up to 20000, polling rate; brightness of logo and scroll wheel; one colour, not saved |
| Basilisk Ultimate | `1532:0086` | interface 0 | DPI up to 20000, polling rate shown; brightness of logo, scroll wheel, left side and right side; one colour, not saved; battery shown |
| Viper Mini | `1532:008a` | interface 0 | DPI up to 8500, polling rate; brightness of logo; one colour, not saved |
| DeathAdder V2 Mini | `1532:008c` | interface 0 | DPI up to 8500, polling rate; brightness of logo; one colour, not saved |
| Naga Left Handed Edition 2020 | `1532:008d` | interface 0 | DPI up to 20000, polling rate; brightness of logo, scroll wheel and right side; one colour, not saved |
| Naga Pro (Wired) | `1532:008f` | interface 0 | DPI up to 20000, polling rate; brightness of logo and scroll wheel; one colour, not saved; battery shown |
| Viper 8KHz | `1532:0091` | interface 0 | DPI up to 20000, polling rate; brightness of logo |
| Naga X | `1532:0096` | interface 3 | DPI up to 18000, polling rate; brightness of scroll wheel and left side; one colour, not saved |
| DeathAdder Essential (2021) | `1532:0098` | interface 0 | DPI up to 6400, polling rate; brightness of logo |
| Basilisk V3 | `1532:0099` | interface 3 | DPI up to 26000, polling rate; brightness of logo and scroll wheel; one colour, not saved |
| Viper Mini SE (Wired) | `1532:009e` | interface 0 | DPI up to 30000, polling rate; battery shown |
| DeathAdder V2 Lite | `1532:00a1` | interface 0 | DPI up to 8500, polling rate; brightness of logo; one colour, not saved |
| Cobra | `1532:00a3` | interface 0 | DPI up to 8500, polling rate; brightness of logo |
| Viper V2 Pro (Wired) | `1532:00a5` | interface 0 | DPI up to 30000, polling rate; battery shown |
| Naga V2 Pro (Wired) | `1532:00a7` | interface 0 | DPI up to 30000, polling rate; brightness of logo; one colour, not saved; battery shown |
| Basilisk V3 Pro (Wired) | `1532:00aa` | interface 0 | DPI up to 30000, polling rate; brightness of logo and scroll wheel; one colour, not saved; battery shown |
| Cobra Pro (Wired) | `1532:00af` | interface 0 | DPI up to 30000, polling rate; brightness of logo and scroll wheel; battery shown |
| DeathAdder V3 | `1532:00b2` | interface 0 | DPI up to 30000, polling rate |
| DeathAdder V3 Pro (Wired) | `1532:00b6`, `1532:00c2` | interface 0 | DPI up to 35000, polling rate; battery shown |
| DeathAdder V4 Pro (Wired) | `1532:00be` | interface 0 | DPI up to 45000, polling rate; battery shown |
| Viper V3 Pro (Wired) | `1532:00c0` | interface 0 | DPI up to 35000, polling rate; battery shown |
| Viper V3 Pro (Wireless) | `1532:00c1` | interface 0 | DPI up to 35000, polling rate; what the dongle light shows; battery shown |
| Pro Click V2 Vertical Edition (Wired) | `1532:00c7` | interface 0 | DPI up to 30000, polling rate; battery shown |
| Basilisk V3 35K | `1532:00cb` | interface 3 | DPI up to 35000, polling rate; brightness of logo and scroll wheel; one colour, not saved |
| Basilisk V3 Pro 35K (Wired) | `1532:00cc` | interface 0 | DPI up to 35000, polling rate; brightness of logo and scroll wheel; one colour, not saved; battery shown |
| Pro Click V2 (Wired) | `1532:00d0` | interface 0 | DPI up to 30000, polling rate; battery shown |
| Basilisk Mobile (Wired) | `1532:00d3` | interface 0 | DPI up to 18000, polling rate |
| Basilisk V3 Pro 35K Phantom Green Edition (Wired) | `1532:00d6` | interface 0 | DPI up to 35000, polling rate; brightness of scroll wheel; one colour, not saved; battery shown |
| Viper V3 Pro SE (Wired) | `1532:00de` | interface 0 | DPI up to 35000, polling rate; battery shown |
| Viper V3 Pro SE (Wireless) | `1532:00df` | interface 0 | DPI up to 35000, polling rate; battery shown |
<!-- end generated -->

## Razer keyboards

🧪 Experimental: the BlackWidow V4 Mini HyperSpeed, on its cable and through its HyperSpeed receiver.
👁 Read-only: the other <!-- generated: count razer-keyboard:read-only -->67<!-- end generated --> Razer keyboards. Nobody has tried PeriHub on one yet. The protocol is the one of the Razer
mice, from facts found in [OpenRazer](https://github.com/openrazer/openrazer) (GPL-2.0), described in
[protocols/razer-keyboards.md](protocols/razer-keyboards.md): no code was copied. The list below is written from
OpenRazer's tables by `tools/openrazer_facts.py`; Razer laptops and keypads (Tartarus, Orbweaver, Nostromo) are left out.

- PeriHub reads the firmware and the layout, and where OpenRazer reads them on that keyboard, the backlight
  brightness, the game mode, and on the wireless ones the battery and whether it is charging.
- **BlackWidow V4 Mini HyperSpeed** (the one a user asked for in [issue #24](https://github.com/hex-dump/perihub/issues/24)):
  **Save to memory** changes the backlight brightness, the game mode and the macro light, after the No warranty box the
  first time. OpenRazer sends them with "variable storage", so PeriHub treats every change as one that stays: it saves
  the settings first, reads them back, and puts them back after 15 seconds unless you keep the change.
  **Lighting effects**: **Send** puts one of the keyboard's own effects (off, static, breathing, spectrum, wave,
  reactive, starlight, with their colours, directions and speeds) into its memory, the way OpenRazer does. The keyboard
  cannot report which effect it shows, so PeriHub cannot check it: it remembers what it sent itself, and **Put back**
  sends the effect before the last one again. The effect the keyboard had before the first change (set by Razer
  Synapse) cannot be read, so PeriHub cannot put it back; the page says so first, and suggests noting it in Synapse.
  **One colour, not saved**: **Send** draws one colour on every key of the keyboard's custom frame (5 rows of 14
  columns), the way OpenRazer draws its custom frames, without saving it to the keyboard's memory. The frame cannot
  be read back, so PeriHub cannot show it, check the colour or put back what was there, and the warning stays on
  screen. Nobody has checked yet how long the colour lasts, or what comes back after the keyboard is turned off or
  unplugged.
- Wireless keyboards are recognized on their cable and through their own HyperSpeed receiver, where OpenRazer gives
  that receiver to the keyboard alone.
- Left out: **the ripple effect** (OpenRazer draws it from the key presses it reads, and PeriHub never reads what is
  typed), colours for single keys, macros, key remapping, the Hypershift layer and profiles. Changes on the other Razer
  keyboards wait until a tester has confirmed this one.
- **On Linux only the BlackWidow V4 Mini HyperSpeed for now.** On most Razer keyboards the interface OpenRazer sends
  to also carries what is typed, and the udev rule would let any program read it, so the rule opens only interface 3
  of the V4 Mini, which a tester showed holds only the media keys. The others are read on Windows, where the system
  keeps typed keys from programs. On Linux, **Send a report** lists such a keyboard with its interfaces, which is what
  is needed to open it there safely. PeriHub also reads the report descriptor of a keyboard's node first: a node that
  declares typed keys is never opened, whatever rule gave access to it.

<!-- generated: table razer-keyboard -->
| Keyboard | USB ID | Opened on Linux | Level | What PeriHub changes |
|---|---|---|---|---|
| BlackWidow Ultimate 2012 | `1532:010d` | — | 👁 Read-only | — |
| BlackWidow Stealth Edition | `1532:010e` | — | 👁 Read-only | — |
| Anansi | `1532:010f` | — | 👁 Read-only | — |
| DeathStalker/DeathStalker Essential | `1532:0118` | — | 👁 Read-only | — |
| BlackWidow Ultimate 2013 | `1532:011a` | — | 👁 Read-only | — |
| BlackWidow (Classic) | `1532:011b` | — | 👁 Read-only | — |
| BlackWidow Tournament Edition 2014 | `1532:011c` | — | 👁 Read-only | — |
| DeathStalker Expert | `1532:0202` | — | 👁 Read-only | — |
| BlackWidow Chroma | `1532:0203` | — | 👁 Read-only | — |
| DeathStalker Chroma | `1532:0204` | — | 👁 Read-only | — |
| BlackWidow Tournament Edition Chroma | `1532:0209` | — | 👁 Read-only | — |
| BlackWidow Chroma (Overwatch) | `1532:0211` | — | 👁 Read-only | — |
| BlackWidow Ultimate 2016 | `1532:0214` | — | 👁 Read-only | — |
| BlackWidow X Chroma | `1532:0216` | — | 👁 Read-only | — |
| BlackWidow X Ultimate | `1532:0217` | — | 👁 Read-only | — |
| BlackWidow X Tournament Edition Chroma | `1532:021a` | — | 👁 Read-only | — |
| Ornata Chroma | `1532:021e` | — | 👁 Read-only | — |
| Ornata | `1532:021f` | — | 👁 Read-only | — |
| BlackWidow Chroma V2 | `1532:0221` | — | 👁 Read-only | — |
| Huntsman Elite | `1532:0226` | — | 👁 Read-only | — |
| Huntsman | `1532:0227` | — | 👁 Read-only | — |
| BlackWidow Elite | `1532:0228` | — | 👁 Read-only | — |
| Cynosa Chroma | `1532:022a` | — | 👁 Read-only | — |
| Cynosa Chroma Pro | `1532:022c` | — | 👁 Read-only | — |
| BlackWidow Lite | `1532:0235` | — | 👁 Read-only | — |
| BlackWidow Essential | `1532:0237` | — | 👁 Read-only | — |
| Cynosa Lite | `1532:023f` | — | 👁 Read-only | — |
| BlackWidow 2019 | `1532:0241` | — | 👁 Read-only | — |
| Huntsman Tournament Edition | `1532:0243` | — | 👁 Read-only | — |
| BlackWidow V3 | `1532:024e` | — | 👁 Read-only | — |
| Huntsman Mini | `1532:0257` | — | 👁 Read-only | — |
| BlackWidow V3 Mini HyperSpeed (Wired) | `1532:0258` | — | 👁 Read-only | — |
| BlackWidow V3 Pro Wired | `1532:025a` | — | 👁 Read-only | — |
| BlackWidow V3 Pro 2.4 Ghz Wireless | `1532:025c` | — | 👁 Read-only | — |
| Ornata V2 | `1532:025d` | — | 👁 Read-only | — |
| Cynosa V2 | `1532:025e` | — | 👁 Read-only | — |
| Huntsman V2 Analog | `1532:0266` | — | 👁 Read-only | — |
| Huntsman Mini JP | `1532:0269` | — | 👁 Read-only | — |
| Huntsman V2 Tenkeyless | `1532:026b` | — | 👁 Read-only | — |
| Huntsman V2 | `1532:026c` | — | 👁 Read-only | — |
| BlackWidow V3 Mini HyperSpeed (Wireless) | `1532:0271` | — | 👁 Read-only | — |
| Huntsman Mini Analog | `1532:0282` | — | 👁 Read-only | — |
| BlackWidow V4 | `1532:0287` | — | 👁 Read-only | — |
| BlackWidow V4 Pro | `1532:028d` | — | 👁 Read-only | — |
| Ornata V3 (Alternate) | `1532:028f` | — | 👁 Read-only | — |
| DeathStalker V2 Pro (Wireless) | `1532:0290` | — | 👁 Read-only | — |
| DeathStalker V2 Pro (Wired) | `1532:0292` | — | 👁 Read-only | — |
| BlackWidow V4 X | `1532:0293` | — | 👁 Read-only | — |
| Ornata V3 X | `1532:0294` | — | 👁 Read-only | — |
| DeathStalker V2 | `1532:0295` | — | 👁 Read-only | — |
| DeathStalker V2 Pro TKL (Wireless) | `1532:0296` | — | 👁 Read-only | — |
| DeathStalker V2 Pro TKL (Wired) | `1532:0298` | — | 👁 Read-only | — |
| Ornata V3 | `1532:02a1` | — | 👁 Read-only | — |
| Ornata V3 X (Alternate) | `1532:02a2` | — | 👁 Read-only | — |
| Ornata V3 Tenkeyless | `1532:02a3` | — | 👁 Read-only | — |
| BlackWidow V4 75% | `1532:02a5` | — | 👁 Read-only | — |
| Huntsman V3 Pro | `1532:02a6` | — | 👁 Read-only | — |
| Huntsman V3 Pro TKL | `1532:02a7` | — | 👁 Read-only | — |
| Huntsman V3 Pro Mini | `1532:02b0` | — | 👁 Read-only | — |
| BlackWidow V4 Mini HyperSpeed | `1532:02b9` | interface 3 | 🧪 Experimental | backlight brightness, game mode, macro light, lighting effects (saved in the keyboard) and one colour, not saved |
| BlackWidow V4 Mini HyperSpeed (wireless) | `1532:02ba` | interface 3 | 🧪 Experimental | backlight brightness, game mode, macro light, lighting effects (saved in the keyboard) and one colour, not saved |
| Joro | `1532:02cd` | — | 👁 Read-only | — |
| Huntsman V3 Pro 8KHz | `1532:02cf` | — | 👁 Read-only | — |
| BlackWidow V4 Low-profile Tenkeyless HyperSpeed (Wireless) | `1532:02d2` | — | 👁 Read-only | — |
| BlackWidow V4 Low-profile Tenkeyless HyperSpeed (Wired) | `1532:02d4` | — | 👁 Read-only | — |
| BlackWidow V4 Tenkeyless HyperSpeed (Wireless) | `1532:02d5` | — | 👁 Read-only | — |
| BlackWidow V4 Tenkeyless HyperSpeed (Wired) | `1532:02d7` | — | 👁 Read-only | — |
| Huntsman V3 HE Magnetic Tenkeyless 8KHz | `1532:02ea` | — | 👁 Read-only | — |
| BlackWidow V3 TK | `1532:0a24` | — | 👁 Read-only | — |
<!-- end generated -->

## Glorious mice

🧪 Experimental: <!-- generated: count glorious -->3<!-- end generated --> Glorious mice with a cable, built on a SinoWealth controller. Nobody has tried PeriHub
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

<!-- generated: table glorious -->
| Mouse | USB ID | Opened on Linux | Firmware |
|---|---|---|---|
| Model O/O- | `258a:0036` | whole mouse | `V103` |
| Model D | `258a:0033` | whole mouse | `V102` |
| Model O Eternal | `3794:a000` | whole mouse | `1009` |
<!-- end generated -->

## Logitech G213 Prodigy

🧪 Experimental, on Windows only for now: nobody has tried PeriHub on one yet ([issue #96](https://github.com/hex-dump/perihub/issues/96)).
It is a keyboard that speaks Logitech's HID++ 2.0 on a vendor collection of its own. Its protocol comes from facts in
[OpenRGB](https://gitlab.com/CalcProgrammer1/OpenRGB) and [g810-led](https://github.com/MatMoul/g810-led) (GPL: no code was copied)
and in [G213Colors](https://github.com/SebiTimeWaster/G213Colors) and [gdevd](https://github.com/R1tschY/gdevd) (MIT), and is
described in [protocols/logitech-g213.md](protocols/logitech-g213.md).

- PeriHub asks the keyboard for its HID++ version and for where it lists its lighting (the colour LED effects, feature `8070`)
  and a few other features, for the test report. If the lighting is not where every source sends it, the page stays read-only,
  and says why.
- **The lighting, one setting at a time, never stored**: one colour for the whole keyboard or for each of its five areas (left,
  middle, right, arrows and home keys, numpad), off, breathing in a colour, a colour cycle and waves in five directions, with the
  time of one cycle and a brightness for the effects that move. The keyboard cannot report its lighting, so PeriHub cannot show
  it, check a change or undo one: the page says so, and the sources say the keyboard forgets what was sent when it is unplugged
  (**to go back to its own lighting, unplug it and plug it in again**). Nothing asks for the No warranty box, because nothing is saved.
- Left out: the **startup effect** (whether the keyboard starts with its colour cycle). The keyboard stores it, no source reads
  it, and the two that send it disagree on the command, so PeriHub could neither check nor undo it. It will come after a test
  report shows which feature the lighting is, and someone has tried the command on a real G213.
- The G213 has no per-key colours, and PeriHub changes nothing else on it.
- Its HID++ collection (usage page `ff43`) is on interface 1 with two keyboard collections. Windows gives each collection a path of
  its own, and PeriHub opens only the vendor one. On Linux they share one node that also carries what is typed, so PeriHub does not
  open the keyboard there, and the udev rule does not list it.

<!-- generated: table g213 -->
| Keyboard | USB ID | Opened on Linux |
|---|---|---|
| G213 Prodigy | `046d:c336` | — |
<!-- end generated -->

## Logitech G600

🧪 Experimental: nobody has tried PeriHub on one yet. Its protocol comes from
[libratbag](https://github.com/libratbag/libratbag) (MIT), and is described in
[protocols/logitech-g600.md](protocols/logitech-g600.md). It is not a HID++ mouse.

- PeriHub reads the profile in use: the DPI of each slot that is on, the slot in use, the polling rate,
  the lighting (steady, breathing or colour cycle, colour, duration) and what each of the 20 buttons does.
- **Save to memory** changes the DPI of the slots that are on, the polling rate and the lighting, after the
  No warranty box the first time. Writing a profile stores it: there is no change that lasts only until
  the mouse is unplugged. PeriHub reads the profile again just before, writes nothing if it changed since
  the mouse was connected, writes every other byte back as read, reads it back after, puts back the DPI
  slot that was in use, and puts the profile back after 15 seconds unless you keep the change.
- Not offered yet: buttons, G-Shift, the other two profiles, switching profile.

<!-- generated: table g600 -->
| Mouse | USB ID | Opened on Linux |
|---|---|---|
| G600 | `046d:c24a` | interface 1 |
<!-- end generated -->

## Older Logitech mice: G9, G9x, G500, G500s, G700, G700s

Logitech removed their software, Logitech Gaming Software, from its site on 31 October 2026. They speak HID++ 1.0, an older protocol than the one of the mice above. Their protocol comes from
[libratbag](https://github.com/libratbag/libratbag) (MIT), and is described in
[protocols/logitech-hidpp10.md](protocols/logitech-hidpp10.md). Nobody has tried PeriHub on one yet.

- PeriHub reads the DPI in use, the polling rate, which profile is in use and, from the mouse's memory, what
  that profile stores: its name, its DPI levels, its polling rate, its colour (G9, G9x) and what each button
  does. It shows the stored profile as it is.
- 🧪 **G500s and G700s**: **Apply** changes the DPI in use, from 50 to 8200 in steps of 50. That is a setting in
  the mouse's RAM according to libratbag: it should last until the mouse is unplugged or its own DPI buttons are
  pressed, but nobody has confirmed that yet. PeriHub reads it back and puts it back after 15 seconds unless you keep
  the change.
- 👁 **G9, G9x, G500, G700**: read-only. The sources disagree on how their DPI is coded, or code it in steps of
  23.53 DPI that nobody has checked.
- PeriHub **never writes their memory**: the stored profiles, the buttons and the macros stay as Logitech
  Gaming Software left them. It never switches profile, not even away from the factory one.
- With the cable only: the G700 and G700s through their receiver are not supported.
- Not supported: the **G400s** (`046d:c24c`). The only tool known to configure it sends a raw USB request
  that PeriHub cannot send, and nothing can be read back from it.

<!-- generated: table logitech-hidpp10 -->
| Mouse | USB ID | Opened on Linux | Level |
|---|---|---|---|
| G9 | `046d:c048` | interface 1 | 👁 Read-only |
| G9x | `046d:c066` | interface 1 | 👁 Read-only |
| G9x (Call of Duty MW3 Edition) | `046d:c249` | interface 1 | 👁 Read-only |
| G500 | `046d:c068` | interface 1 | 👁 Read-only |
| G500s | `046d:c24e` | interface 1 | 🧪 Experimental |
| G700 | `046d:c06b` | interface 1 | 👁 Read-only |
| G700s | `046d:c07c` | interface 1 | 🧪 Experimental |
<!-- end generated -->

## Roccat mice

👁 Read-only: nobody has tried PeriHub on one yet. Their protocol comes from
[libratbag](https://github.com/libratbag/libratbag) (MIT), and is described in
[protocols/roccat-kone.md](protocols/roccat-kone.md). PeriHub reads the profile in use, the DPI of each slot that is
on, the slot in use and the polling rate, and changes nothing: the sources disagree on what a write needs. The only
report it sends chooses which profile the next read returns.

<!-- generated: table roccat -->
| Mouse | USB ID | Opened on Linux |
|---|---|---|
| Kone Pure | `1e7d:2dbe`, `1e7d:2dc2` | interface 0 |
| Kone XTD | `1e7d:2e22` | interface 0 |
| Kone EMP | `1e7d:2e24` | interface 0 |
<!-- end generated -->

`1e7d:2dcb`, which libratbag also lists as a Kone Pure, is left out: its interfaces are laid out differently and no
source says where its configuration is.

## Pulsar mice

🧪 Experimental: <!-- generated: count pulsar -->7<!-- end generated --> Pulsar mice on their USB cable, built on a Sonix controller. Nobody has tried PeriHub on one yet.
Their protocol comes from [pulsar-mouse-linux](https://github.com/packerlschupfer/pulsar-mouse-linux) (MIT), a
recent project built from USB captures of Pulsar's own app, and is described in
[protocols/pulsar-sonix.md](protocols/pulsar-sonix.md). That project's author tried the X2A, the X3 and the X3 LHD;
the Xlite Wired and X2 Wired are said to speak the X2A's protocol, and the Xlite v4 was only partly read.

- PeriHub reads the firmware, the profile in use, its DPI stages and the stage in use, and the global settings:
  polling rate, debounce time, angle snapping, ripple control and motion sync.
- **Save to memory** changes those global settings and the DPI of each stage of the profile in use, after the No
  warranty box the first time. These mice store every change by themselves. PeriHub reads them again just before,
  writes nothing if the profile in use changed, writes the stage in use and how many stages there are back as
  read, reads everything back after, and puts it back after 15 seconds unless you keep the change.
- A stage whose X and Y DPI differ is shown and not changed. A mouse that does not answer which profile is in use
  shows its global settings without the DPI stages: PeriHub never guesses a profile.
- Not offered yet: lift-off distance, lighting, buttons, the other profiles, polling rates above 1000 Hz. Never
  sent: switching profile, and the profile reset, which also resets every global setting.
- The CrazyLight mice speak another protocol: see [Pulsar CrazyLight and Lamzu Atlantis](#pulsar-crazylight-and-lamzu-atlantis).
  The wireless dongles, the Feinmann mice, the first X2H (`3554`) and the X2 Wireless (`25a7`) are left out.

<!-- generated: table pulsar -->
| Mouse | USB ID | Opened on Linux |
|---|---|---|
| Xlite Wired | `3710:1401` | interface 3 |
| X2 Wired | `3710:1402` | interface 3 |
| X2H Wired Medium | `3710:1403` | interface 3 |
| X2A Medium Wired | `3710:1404` | interface 3 |
| Xlite v4 | `3710:3401` | interface 3 |
| X3 | `3710:3409` | interface 3 |
| X3 LHD | `3710:3410` | interface 3 |
<!-- end generated -->

Interface 1, which carries the keys a button bound to a key or a macro types, is never opened.

## Pulsar CrazyLight and Lamzu Atlantis

🧪 Experimental: nobody has tried PeriHub on one yet. Three wired mice whose firmware shares one protocol (17-byte
reports, report id 8), from [pulsar-mouse-linux](https://github.com/packerlschupfer/pulsar-mouse-linux), whose author
confirmed reads and writes on both CrazyLight mice, and [lamzu-cfg](https://github.com/LeadSun/lamzu-cfg) (both MIT).
Described in [protocols/nordic-mice.md](protocols/nordic-mice.md).

- PeriHub reads the firmware, the profile in use, its DPI stages and the stage in use, polling rate, lift-off
  distance, debounce time, motion sync, angle snapping and ripple control. Every value is checked against the
  check byte the mouse stores beside it: one that does not add up leaves the mouse read-only.
- **Save to memory** (CrazyLight only) changes those settings and the DPI of each stage, after the No warranty box the
  first time, with backup, read-back and the 15-second undo. Each address PeriHub may write is a command of its own,
  so nothing else can be written. Above 10240 DPI the mouse takes steps of 50, above 25600 steps of 100.
- On the cable the mouse runs at 1000 Hz at most, but a profile may hold a faster rate for the dongle: then the
  polling rate is shown and not changed.
- The **Lamzu Atlantis** is read only: its vendor id is shared by other brands' mice, and only one small project has
  tried writing to it.
- Never sent: the factory reset, profile switches, buttons, macros. The wireless dongles and the Feinmann mice are
  left out.

<!-- generated: table nordic -->
| Mouse | USB ID | Opened on Linux | Level |
|---|---|---|---|
| Pulsar X2 CrazyLight | `3710:3414` | interface 1 | 🧪 Experimental |
| Pulsar X3 LHD CrazyLight | `3710:3527` | interface 1 | 🧪 Experimental |
| Lamzu Atlantis | `3554:f50f` | interface 1 | 👁 Read-only |
<!-- end generated -->

## Cooler Master mice

🧪 Experimental, lighting only: nobody has tried PeriHub on one yet. Their protocol comes from the USB captures of
Cooler Master's own app that users attached to OpenRGB's device requests (facts only, no code), and is described in
[protocols/coolermaster-mm.md](protocols/coolermaster-mm.md).

- PeriHub reads the lighting mode in use and the colour and brightness each mode keeps.
- **Save to memory** changes the colour and brightness of the mode in use, or switches between steady, breathing,
  colour cycle and off, after the No warranty box the first time. Every change goes through the save sequence of
  Cooler Master's app, with backup, read-back and the 15-second undo. A new mode keeps its own colour and brightness:
  change them after switching.
- In stars, indicator, custom or multilayer mode the lighting is shown and not changed.
- Not offered: effect speed, colours per zone, buttons, DPI. The MM531 and MM712 are left out.

<!-- generated: table coolermaster -->
| Mouse | USB ID | Opened on Linux |
|---|---|---|
| MM530 | `2516:0065` | interface 1 |
| MM711 | `2516:0101` | interface 1 |
| MM720 | `2516:0141` | interface 1 |
| MM730 | `2516:0165` | interface 1 |
<!-- end generated -->

Interface 2, which carries the keys a button bound to a key types, is never opened.

## EVGA mice

🧪 Experimental, lighting only: nobody has tried PeriHub on one yet. The protocol comes from facts in OpenRGB's EVGA
drivers (no code copied), described in [protocols/evga-mice.md](protocols/evga-mice.md).

- **X12**: PeriHub reads the mode, colour and brightness of its front, wheel and logo zones. **Save to memory** turns
  a zone off or steady and changes its colour and brightness, after the No warranty box the first time, with backup,
  read-back and the 15-second undo. Whether the mouse keeps a change after it is unplugged is not known yet. A zone in
  one of the other modes of EVGA's software is shown and not changed.
- **X20** on its cable: read only (mode, brightness and colour of each zone). Its writes are not checked by anything
  the mouse answers, so they wait for a tester. Its wireless receiver is left out.
- The **Z15 and Z20 keyboards** are left out: their lighting shares an interface that may carry what is typed.

<!-- generated: table evga -->
| Mouse | USB ID | Opened on Linux | Level |
|---|---|---|---|
| X12 | `3842:2422` | interface 1 | 🧪 Experimental |
| X20 | `3842:2420` | interface 2 | 👁 Read-only |
<!-- end generated -->

## YJX mice

👁 Read-only: nobody has tried PeriHub on one yet. Many wireless mice (MAMBASNAKE M4, Rajin Blaze Claws, K-Snake X11,
EWEADN GS02 Pro…) use a receiver with the id `a8a5:2255` (`a8a4:2255` on the cable), and the same id stands for
mice that wrap their reports in different ways. The protocol is described in [protocols/yjx-mice.md](protocols/yjx-mice.md),
from [claws-mouse-linux](https://github.com/naevui/claws-mouse-linux) and
[EWEADN-GS02-Pro-Control](https://github.com/mimuruu/EWEADN-GS02-Pro-Control) (both MIT) and facts from
[OpenMouse](https://github.com/OpenMouse-Project/mouse-protocol) (AGPL-3.0): no code was copied.

- PeriHub first reads the report descriptor of the configuration collection, which asks the system and sends nothing to
  the mouse, and sends its reads only when the descriptor has no report numbers, the format the two MIT sources tried
  on real mice. Any other mouse is left alone and shown read-only, with the descriptor in its test report.
- It reads the model name, the DPI levels and the one in use, the polling rate, the lighting mode and lift-off
  distance (as numbers) and the battery. A mouse that is off or away from its receiver does not answer: PeriHub asks
  again later.
- Left out: changing any setting (a malformed settings write can stop the sensor until the mouse is turned off),
  buttons, lighting and macros.
- On Linux only interface 2, the vendor collection, is opened. The receiver's other interfaces carry the mouse and a
  keyboard.

<!-- generated: table yjx -->
| Mouse | USB ID | Opened on Linux |
|---|---|---|
| mouse (YJX receiver) | `a8a5:2255` | interface 2 |
| mouse (YJX, on its cable) | `a8a4:2255` | interface 2 |
<!-- end generated -->

## Wraith W60

👁 Read-only, on Windows only for now: read on a real W60 (`2e3c:c368`, firmware V3.22.19, [issue #35](https://github.com/hex-dump/perihub/issues/35)). `2e3c:c365` is the shared USB ID of other keyboards on the same Artery/W669 platform (such as the AULA WIN60 HE and the X68HE of [issue #123](https://github.com/hex-dump/perihub/issues/123)), whose configuration collection matches byte-for-byte. Described in [protocols/wraith-he.md](protocols/wraith-he.md)
from [AulaControl](https://github.com/sgtflixy/AulaControl) (Apache-2.0): no code was copied. PeriHub reads the model,
the firmware version, the firmware date and the key and side lighting (mode as a number, speed, brightness, both
colours, direction, full colour).

- Left out: changing the lighting, per-key colours, actuation and rapid trigger, SOCD, key remapping and the polling rate.
- Its configuration collection can also send how far each key is pressed (while the vendor's travel test runs). PeriHub
  never asks for it and skips such reports, but on Linux any program that could open the collection could read it, so
  PeriHub does not open this keyboard there yet: it will take a rule you turn on.

<!-- generated: table wraith -->
| Keyboard | USB ID | Opened on Linux |
|---|---|---|
| Wraith W60 | `2e3c:c368` | — |
| Artery W669 keyboard 2e3c c365 | `2e3c:c365` | — |
<!-- end generated -->

## EVision keyboards

🧪 Experimental on `320f:505b` and `320f:5000`, 👁 read-only on the other ids, **on Windows only for now**. A Redragon K580 (`320f:5000`, [issue #117](https://github.com/hex-dump/perihub/issues/117)) has been read with PeriHub; no one has tried changing a keyboard yet.
These are keyboards on an EVision controller, which many brands use (the Surmen V400 of
[issue #80](https://github.com/hex-dump/perihub/issues/80) is a `320f:505b`). The protocol is described in
[protocols/evision.md](protocols/evision.md) from facts found in [OpenRGB](https://gitlab.com/CalcProgrammer1/OpenRGB)'s EVision
controllers and in [OpenEVision](https://github.com/iefedev/OpenEVision) (both GPL-2.0), a Windows app that tried it on
a `320f:505b`: no code was copied. The ids are the ones OpenRGB lists; only `320f:505b` and `320f:5000` have been seen answering, so only
they may be changed.

- PeriHub reads the lighting in use (mode, brightness, speed, direction, random colour, colour) and, on the older layout,
  counts the key table for the test report. On `320f:5041`, which stores three profiles, it reads the profile in use and
  its lighting. A value outside what OpenRGB documents leaves the keyboard read-only, with the reason.
- On `320f:505b` and `320f:5000` only, **Save to memory** changes, after the **No warranty** box, with the same backup,
  read-back and 15-second undo, everything the sources document:
  - the lighting: mode (Custom per key included; Starlight slow and Surmount on `320f:505b`), brightness, colour,
    speed, direction and random colour;
  - the **colours per key**, LED by LED or all at once, in a grid of rows and columns named with OpenRGB's and
    OpenEVision's maps (not checked on every keyboard: a key may sit elsewhere, and macro keys have no name);
  - the **key table**: a key, a modifier or a media key can be set to another key, a modifier, a media key, or
    nothing. Each key is named by what it sent the first time PeriHub read the keyboard, which PeriHub keeps. The Fn
    key and the entries no source describes (the K580's macro keys among them) are never changed.

  Every change stays in the keyboard's flash, as the sources say.
- Left out, waiting for captures from a tester (`docs/captures.md`): macros and the macro keys, onboard profiles, the Fn
  layers, the polling rate and Surmount's colour: no source documents them (see
  [protocols/evision.md](protocols/evision.md#left-out-waiting-for-captures)).
- **Not opened on Linux.** The vendor collection is on interface 1 together with a keyboard, a mouse and two more
  collections, and Linux has one hidraw node per interface: opening it also gives access to what is typed, and a udev
  rule would give that to any program running as you. **Send a report** lists the keyboard with its interfaces. Opening it
  safely would take a rule you turn on yourself, with that warning, which PeriHub does not have yet.

<!-- generated: table evision -->
| Keyboard | USB ID | Opened on Linux | Level |
|---|---|---|---|
| RGB keyboard 320f 505b | `320f:505b` | — | 🧪 Experimental |
| RGB keyboard 320f 5000 | `320f:5000` | — | 🧪 Experimental |
| RGB keyboard 320f 502a | `320f:502a` | — | 👁 Read-only |
| RGB keyboard 320f 5064 | `320f:5064` | — | 👁 Read-only |
| RGB keyboard 320f 5078 | `320f:5078` | — | 👁 Read-only |
| RGB keyboard 320f 5084 | `320f:5084` | — | 👁 Read-only |
| RGB keyboard 320f 5041 | `320f:5041` | — | 👁 Read-only |
| RGB keyboard 0c45 5004 | `0c45:5004` | — | 👁 Read-only |
| RGB keyboard 0c45 5104 | `0c45:5104` | — | 👁 Read-only |
| RGB keyboard 0c45 5204 | `0c45:5204` | — | 👁 Read-only |
| RGB keyboard 0c45 652f | `0c45:652f` | — | 👁 Read-only |
| RGB keyboard 0c45 7672 | `0c45:7672` | — | 👁 Read-only |
| RGB keyboard 0c45 7698 | `0c45:7698` | — | 👁 Read-only |
| RGB keyboard 0c45 8520 | `0c45:8520` | — | 👁 Read-only |
<!-- end generated -->

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
- On Linux a keyboard needs a line in the optional rule `73-perihub-qmk.rules`, which PeriHub writes
  from the **QMK keyboards** page: it opens only the interface whose descriptor holds the raw HID
  collection and nothing else, checked in `/sys` first.

## Something else?

Open a [device request](https://github.com/hex-dump/perihub/issues/new?template=device_request.yml)
with the model and USB ID. Any brand can be requested. PeriHub's **Send a report** page fills it in, and
its **Record the official software** button adds a USB recording of what the device's own software sends
it ([captures.md](captures.md)).
