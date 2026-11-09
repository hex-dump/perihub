# Logitech onboard profiles: the profile list and the factory profiles

This page describes, in our own words, how a Logitech HID++ mouse with onboard profiles (feature
`0x8100`) decides which profiles it uses, and what PeriHub does with a mouse nothing was ever saved
to. PeriHub's code for this is written from this page. No code from other projects was copied or
translated.

Sources studied (read only, for the protocol facts):

- [libratbag](https://github.com/libratbag/libratbag) (MIT), `hidpp20.c`: the profile list
  ("dictionary"), its checksum, the fallback to the factory profiles, how a commit writes the
  profiles and then the list, the G305 that refuses to read a list that was never written.
- [Solaar](https://github.com/pwr-Solaar/Solaar) (GPL-2.0), `hidpp20.py` and
  `settings_templates.py`: the factory list in sector `0x0100`, and profiles selected by their
  two-byte address, `0x01nn` for a factory profile.
- A real G502 HERO (`046d:c08b`, firmware U1 27.03): everything marked **(G502)**.
- Issues [#15](https://github.com/hex-dump/perihub/issues/15) and
  [#20](https://github.com/hex-dump/perihub/issues/20): the memory formats of a G502 X Wireless and
  of a G502 HERO with newer firmware.

What is not marked **(G502)** comes from the two projects above and has not been seen by PeriHub
on a real mouse.

## Sectors

- The mouse's own memory is a row of sectors of the same size, numbered from `0x0000`. Sector 0 is
  the **profile list**; sector *n* holds profile *n*; the rest is for macros **(G502)**.
- The factory profiles are in read-only sectors: `0x0100` is the **factory list**, `0x0100 + n`
  factory profile *n*. A mouse can have fewer factory profiles than profiles (2 and 5 on the
  G502 X Wireless of #15); the profiles past them start as copies of factory profile 1.
- A sector ends with a CRC-CCITT (initial value `0xFFFF`) of everything before it, big-endian
  **(G502)**. With 256-byte sectors it sits in bytes 254–255, with 255-byte sectors in 253–254.
- Sector sizes seen: 256 (G502 HERO, U1 27.03), 255 (G502 HERO, U1 74.00, #20; G502 X Wireless,
  #15). A 255-byte sector is written as 16 blocks, the last one carrying 15 bytes.
- Profile formats seen: 2 (G502 HERO), 3 (G502 X Wireless). libratbag and Solaar read and write
  formats 1 to 5 with one layout.

## The profile list

Four bytes for each profile, then an end marker **(G502)**:

| Bytes | Meaning |
|---|---|
| 0–1 | Sector of the profile, big-endian: `00 01` for profile 1 (`01 01` in the factory list) |
| 2 | 1 if the profile is enabled, 0 if the profile switch skips it |
| 3 | `00` |

The end marker is `FF FF 00 00`; the rest of the sector is `FF`, with the CRC at the end.

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
- **The first save** (tested models only). PeriHub writes, in this order: the macros of the saved
  profile, the factory copy of every other profile, the saved profile, and last the list, with the
  flags of the factory list. Until the list is there the mouse keeps using its factory profiles,
  so a save that stops halfway changes nothing. Then it selects, in the mouse's own memory, the
  profile that was active. Every sector is read back after it is written.
- **Enabling a profile** whose sector was never written: its factory copy is written first, so
  the list never points to an empty sector.
- **Untested models** (experimental editing): PeriHub does not create the list, because the
  backup taken before the save would hold no list to put back. The save is refused with an
  explanation; a profile saved once with Logitech G HUB or Onboard Memory Manager creates it.
- A backup taken while the mouse had no list cannot be restored: there is nothing in it the mouse
  would use. **Factory settings** on a profile gives the same result.
