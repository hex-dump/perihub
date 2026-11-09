# Devices

What PeriHub recognizes, and how far each device is supported. PeriHub only talks to the
devices listed here: it never sends anything to a device it does not recognize.

**Support levels**

| Level | What PeriHub does |
|---|---|
| ✅ **Supported** | Everything works, tested on real hardware. |
| 🧪 **Experimental** | Recognized, not tested yet. PeriHub reads it, and you can turn on experimental changes at your own risk (see each section). |
| 👁 **Read-only** | Recognized, not tested yet. PeriHub reads it and changes nothing. |

A device moves up a level when its owner sends a test report: **Open test form** on its page in
PeriHub opens the report on GitHub already filled in.

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
- **Every other mouse below**: 🧪 experimental. PeriHub shows their profiles, DPI levels, buttons,
  lighting and battery. With **Edit anyway…** on the mouse page you can also change and save them,
  at your own risk, if the mouse is connected with its USB cable and its memory uses a format
  PeriHub can write: profile format 2 (the G502 HERO's) or 3 (the G502 X's), in 255- or
  256-byte sectors. Otherwise, and always through a receiver, they are read-only. The test report
  shows the format of your mouse.
- **A mouse nothing was ever saved to** has no profile list of its own and uses its factory
  profiles: PeriHub shows those. On the G502 HERO and G502 X Wireless the first save creates the
  list, with a copy of the factory profiles. On an untested mouse PeriHub does not create it: save
  a profile once with Logitech G HUB or Onboard Memory Manager first.
- **Wireless models** are also recognized through their LIGHTSPEED receiver (`046d:c539`, `c53a`,
  `c53d`, `c53f`, `c541`, `c543`, `c545`, `c547` or `c54d`), read-only. The receiver support has not
  been tried on real hardware yet: reports are welcome in
  [issue #4](https://github.com/hex-dump/perihub/issues/4).

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
