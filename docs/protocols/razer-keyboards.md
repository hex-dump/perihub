# Razer keyboards

How PeriHub talks to a Razer keyboard. **No keyboard has been tried with PeriHub yet.** PeriHub reads every
keyboard below; it also changes the BlackWidow V4 Mini HyperSpeed, as an experimental feature (see "Changes"). The
transport is the one of the Razer mice (`razer-mice.md`): one 90-byte feature report, number 0, read back after a
wait, with the status, transaction id, command class and id, arguments and an XOR checksum. The facts below were
found in [OpenRazer](https://github.com/openrazer/openrazer) (GPL-2.0, commit cd5c424) and are written here in
PeriHub's own words. No code was copied.

The facts of each keyboard are written into `crates/perihub-core/src/peripheral/razer_keyboard_models.rs` by
`tools/openrazer_facts.py --keyboards`, which reads OpenRazer's keyboard driver, the commands it builds and its daemon
as text. It leaves out
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
| Transaction id: the custom frame and showing it | `1f` | `9f` |
| Matrix of the custom frame | 5 rows of 14 columns | 5 rows of 14 columns |

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

## Changes

Only on the BlackWidow V4 Mini HyperSpeed, on its cable and through its receiver. PeriHub sends nothing else to a
keyboard than the reads above, and these four commands. Every transaction id is the one OpenRazer sends for that
keyboard and command. The brightness and game mode changes use the same transaction id as their reads, and the
custom frame the one of the brightness.

| Class, command | Size | Arguments | What |
|---|---|---|---|
| `0f 04` | `03` | `01` (stored), `05` (backlight), brightness 0–255 | The backlight brightness. Transaction id `1f` on the cable, `9f` through the receiver. |
| `03 00` | `03` | `01` (stored), `08` (game mode light), `01` on or `00` off | The game mode. Transaction id `ff`. |
| `0f 03` | `47` | `00 00`, row, first column, last column, then red, green and blue of each column | One row of the custom colour frame. The V4 Mini's matrix has 5 rows of 14 columns (the daemon's matrix size). Same transaction id as the brightness, `1f` or `9f`. |
| `0f 02` | `0c` | `00` (not stored), `00`, `08` | Shows the custom frame, with the transaction id of the frame. |

OpenRazer sends the brightness and the game mode with "variable storage" (`01`): the keyboard keeps them in its
memory. PeriHub therefore treats both as changes that stay: the No warranty box the first time, a backup of the
settings, a read-back with the commands above, and the settings put back after 15 seconds unless the user keeps the
change. The undo writes back the exact brightness byte first read, not the nearest percent.

OpenRazer draws a custom frame by sending each row, then the custom effect, with storage `00`. PeriHub does the same
for **one colour**, the same in all 70 positions, and never with storage `01`. Nothing reads the frame back, so
PeriHub cannot show it or put back what was there: it is offered as a setting it cannot read, with the warning always
on screen. What the keyboard shows after it is turned off, unplugged or put to sleep has not been checked.

## Left out for now

- **The lighting effects of the keyboard's own memory** (none, static, breathing, spectrum, wave, reactive,
  starlight). OpenRazer builds all of them with the command `0f 02`, arguments storage, LED `05`, then the effect
  (`00` none, `01` static, `02` breathing, `03` spectrum, `04` wave, `05` reactive, `07` starlight) and its
  values, and always sends them with storage `01`. It has no command that reads the effect in use, so an effect
  could not be read back, undone or put back after a failed change. PeriHub never sends one. They wait for a tester
  to show a form that does not store, or a way to read the effect. The daemon's ripple effect (a ring that spreads
  from a pressed key) is not one of them: OpenRazer draws it on the custom frame from the key presses it reads, and
  PeriHub never reads what is typed.
- Colours for single keys or groups: the frame's rows and columns are known, but no source PeriHub may use says which
  key sits at which position of the V4 Mini's matrix.
- The macro light, the device mode, key remapping, the Hypershift layer, macros and onboard profiles. Key remapping,
  the Hypershift layer and profiles are not described by any source PeriHub may use: they would need USB captures
  of Razer Synapse from a tester. OpenRazer gives the V4 Mini no polling rate, no idle time and no low-battery
  threshold.
- Changes on the other keyboards: the same commands exist for many of them, but PeriHub changes none until a
  tester has confirmed the V4 Mini.
