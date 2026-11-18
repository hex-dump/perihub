# Razer keyboards

How PeriHub reads a Razer keyboard. **No keyboard has been tried with PeriHub yet**, and for now PeriHub only
reads. The transport is the one of the Razer mice (`razer-mice.md`): one 90-byte feature report, number 0, read
back after a wait, with the status, transaction id, command class and id, arguments and an XOR checksum. The
facts below were found in [OpenRazer](https://github.com/openrazer/openrazer) (GPL-2.0, commit cd5c424) and are
written here in PeriHub's own words. No code was copied.

The facts of each keyboard are written into `crates/perihub-core/src/peripheral/razer_keyboard_models.rs` by
`tools/openrazer_facts.py --keyboards`, which reads OpenRazer's keyboard driver and daemon as text. It leaves out
Razer laptops and keypads (Tartarus, Orbweaver, Nostromo), and any product id another OpenRazer driver also
claims, such as a receiver shared with a mouse.

## Per keyboard

| | Where it comes from in OpenRazer | Values |
|---|---|---|
| USB interface of the reports | the interface the keyboard driver sends its requests to and reads the answers from (the index of the USB request) | 1 on most keyboards, 2 or 3 on the newer ones |
| Wait before reading the answer | the same place | 600 µs on a cable, 4.9 ms through a HyperSpeed receiver |
| Battery and charging | the keyboards whose daemon offers the battery, where both reads use the same transaction id | `1f` on a cable, `9f` through the receiver, `3f` for the BlackWidow V3 Pro on its cable |
| Backlight brightness | the read of the backlight (LED `05`) from the stored values (`01`): the extended command or the standard one | extended with `1f`, `3f` or `9f`; standard with `ff` |
| Game mode | the read of the game mode light (LED `08`) from the stored values, where the daemon offers the game mode | `ff`; left out on the Huntsman V2 and V2 Tenkeyless, which read it from another storage |
| Firmware, layout | the same on every keyboard | `ff` |

A keyboard with no brightness or game mode read in OpenRazer is not asked for it.

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
been seen yet. The receiver belongs to this keyboard: OpenRazer gives it a product id of its own and the
keyboard's commands.

## Linux

On Linux the udev rule opens a keyboard's interface only where a tester showed that it carries no typed keys:
for now only the BlackWidow V4 Mini HyperSpeed's interface 3. On most Razer keyboards the interface OpenRazer
sends to also carries what is typed: OpenRazer's daemon reads the key presses of these keyboards from interface 0
and 1. A rule that opened such an interface would let any program running as the user read the typing, so
PeriHub does not open the other keyboards on Linux, and **Send a report** lists them with their interfaces. On
Windows the system keeps typed keys from programs.

On Linux PeriHub also reads each keyboard node's report descriptor first and does not open one that declares typed
keys (the Keyboard/Keypad page, or a keyboard or keypad of the Generic Desktop page), whatever rule gave access to it.

## Reads

| Class, command | Size | Arguments | Answer |
|---|---|---|---|
| `00 81` | `02` | none | firmware major, minor |
| `07 80` | `02` | none | battery level 0–255 in the second byte |
| `07 84` | `02` | none | `01` while charging, in the second byte |
| `0f 84` | `03` | `01` (stored), `05` (backlight) | storage, LED, brightness 0–255 (extended) |
| `03 83` | `03` | `01` (stored), `05` (backlight) | storage, LED, brightness 0–255 (standard, older keyboards) |
| `03 80` | `03` | `01` (stored), `08` (game mode light) | storage, LED, `01` on or `00` off |
| `00 86` | `02` | none | the keyboard's region (`11` Italian, `06` UK…), with transaction id `ff`: see `keyboard-layouts.md` |

Status `05` means the keyboard does not take the command: PeriHub then shows that value as not supported. Status
`04` (timed out), which a receiver may give while its keyboard is off, makes PeriHub ask again later.

## Left out for now

Lighting effects, the custom frame, changing the brightness and game mode, macros and key remapping. OpenRazer
stores every lighting effect of these keyboards in their memory and has no command that reads the effect back, so
a stored effect could not be undone. Key remapping, the Hypershift layer and onboard profiles are not described by
any source PeriHub may use: they would need USB captures of Razer Synapse from a tester.
