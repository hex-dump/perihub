# Razer keyboards

How PeriHub reads a Razer keyboard. **No keyboard has been tried with PeriHub yet**, and for now PeriHub only
reads. The transport is the one of the Razer mice (`razer-mice.md`): one 90-byte feature report, number 0, read
back after a wait, with the status, transaction id, command class and id, arguments and an XOR checksum. The
facts below were found in [OpenRazer](https://github.com/openrazer/openrazer) (GPL-2.0, commit dfe987c) and are
written here in PeriHub's own words. No code was copied.

## BlackWidow V4 Mini HyperSpeed

| | On its cable | Through its HyperSpeed receiver |
|---|---|---|
| USB id | `1532:02b9` | `1532:02ba` |
| USB interface of the reports | 3 | 3 |
| Wait before reading the answer | 600 µs | 4.9 ms |
| Transaction id: battery, charging, brightness | `1f` | `9f` |
| Transaction id: firmware, game mode | `ff` | `ff` |

Interface 3 holds, through the receiver, only a consumer control collection (usage page `000c`, the media keys),
as the device request in [issue #24](https://github.com/hex-dump/perihub/issues/24) shows. On its cable it has not
been seen yet. On Linux PeriHub reads each keyboard node's report descriptor first and does not open one that
declares typed keys (the Keyboard/Keypad page, or a keyboard or keypad of the Generic Desktop page), whatever rule
gave access to it.

The receiver belongs to this keyboard: OpenRazer gives it a product id of its own and the keyboard's commands.

## Reads

| Class, command | Size | Arguments | Answer |
|---|---|---|---|
| `00 81` | `02` | none | firmware major, minor |
| `07 80` | `02` | none | battery level 0–255 in the second byte |
| `07 84` | `02` | none | `01` while charging, in the second byte |
| `0f 84` | `03` | `01` (stored), `05` (backlight) | storage, LED, brightness 0–255 |
| `03 80` | `03` | `01` (stored), `08` (game mode light) | storage, LED, `01` on or `00` off |
| `00 86` | `02` | none | the keyboard's region (`11` Italian, `06` UK…), with transaction id `ff`: see `keyboard-layouts.md` |

Status `05` means the keyboard does not take the command: PeriHub then shows that value as not supported. Status
`04` (timed out), which a receiver may give while its keyboard is off, makes PeriHub ask again later.

## Left out for now

Lighting effects, the custom frame of 5 rows × 14 columns, changing the brightness and game mode, macros and key
remapping. OpenRazer stores every lighting effect of these keyboards in their memory and has no command that
reads the effect back, so a stored effect could not be undone. Key remapping, the Hypershift layer and onboard
profiles are not described by any source PeriHub may use: they would need USB captures of Razer Synapse from a
tester.
