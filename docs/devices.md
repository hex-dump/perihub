# Devices

What PeriHub recognizes, and how far each device is supported. PeriHub only talks to the
devices listed here: it never sends anything to a device it does not recognize.

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
  are recognized, but nothing paired with them is supported yet: 👁 read-only. Every device that
  answers, mouse or keyboard, gets a page of its own in the sidebar, built from the HID++ features it
  reports: what kind of device it is, what it can do (battery, DPI, SmartShift wheel, programmable
  buttons…), the DPI and polling rates it accepts, and a test report. Nothing is changed on it.
  Changing settings comes next, starting with the DPI: the MX Master 3S of
  [issue #17](https://github.com/hex-dump/perihub/issues/17) is the first one. For now PeriHub
  looks behind the receivers only while no supported Logitech mouse is connected with its cable.
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

## Something else?

Open a [device request](https://github.com/hex-dump/perihub/issues/new?template=device_request.yml)
with the model and USB ID. Any brand can be requested.
