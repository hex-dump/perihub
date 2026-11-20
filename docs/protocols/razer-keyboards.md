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
| Transaction id: the lighting effects (all stored, on the backlight) | `1f` | `9f` |
| Transaction id: the macro light's state, read and write (stored) | `ff` | `ff` |
| The macro light's effect | written without storage, `1f` | written stored, `ff` |
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
| `03 80` | `03` | `01` (stored), `07` (macro light) | the same for the macro light; only the V4 Mini, see "The macro light" |
| `03 82` | `03` | `01` (stored), `07` (macro light) | storage, LED, the macro light's effect (`00` static, `01` blinking); only the V4 Mini through its receiver |
| `00 86` | `02` | none | the keyboard's region (`11` Italian, `06` UK…), with transaction id `ff`: see `keyboard-layouts.md` |

Status `05` means the keyboard does not take the command: PeriHub then shows that value as not supported. Status
`04` (timed out), which a receiver may give while its keyboard is off, makes PeriHub ask again later.

## Changes

Only on the BlackWidow V4 Mini HyperSpeed, on its cable and through its receiver. PeriHub sends nothing else to a
keyboard than the reads above, the two reads of the macro light below, and the commands in the tables below. Every
transaction id is the one OpenRazer sends for that keyboard and command. The brightness and game mode changes use
the same transaction id as their reads, and the custom frame and the lighting effects the one of the brightness.

| Class, command | Size | Arguments | What |
|---|---|---|---|
| `0f 04` | `03` | `01` (stored), `05` (backlight), brightness 0–255 | The backlight brightness. Transaction id `1f` on the cable, `9f` through the receiver. |
| `03 00` | `03` | `01` (stored), `08` (game mode light), `01` on or `00` off | The game mode. Transaction id `ff`. |
| `0f 03` | `47` | `00 00`, row, first column, last column, then red, green and blue of each column | One row of the custom colour frame. The V4 Mini's matrix has 5 rows of 14 columns (the daemon's matrix size). Same transaction id as the brightness, `1f` or `9f`. |
| `0f 02` | `0c` | `00` (not stored), `00`, `08` | Shows the custom frame, with the transaction id of the frame. |
| `0f 02` | `06`, `09` or `0c` | `01` (stored), `05` (backlight), the effect number and its values | A lighting effect of the keyboard's own memory: see below. Same transaction id as the brightness. |
| `03 00` | `03` | `01` (stored), `07` (macro light), `01` on or `00` off | The macro light's state. Transaction id `ff`. |
| `03 02` | `03` | storage, `07` (macro light), `00` static or `01` blinking | The macro light's effect: see below. |

OpenRazer sends the brightness, the game mode and the macro light's state with "variable storage" (`01`): the keyboard keeps
them in its memory. PeriHub therefore treats them as changes that stay: the No warranty box the first time, a backup of
the settings, a read-back with the commands above, and the settings put back after 15 seconds unless the user keeps the
change. The undo writes back the exact brightness byte first read, not the nearest percent.

OpenRazer draws a custom frame by sending each row, then the custom effect, with storage `00`. PeriHub does the same
for **one colour**, the same in all 70 positions, and never with storage `01`. Nothing reads the frame back, so
PeriHub cannot show it or put back what was there: it is offered as a setting it cannot read, with the warning always
on screen. What the keyboard shows after it is turned off, unplugged or put to sleep has not been checked.

### Lighting effects of the keyboard's own memory

OpenRazer's driver builds each effect with `0f 02`; the arguments are the storage, the LED, the effect number and what
follows below. It sends **every one of them with storage `01` on the backlight LED `05`** (the daemon's `set_none_effect`,
`set_static_effect`, `set_breath_*_effect`, `set_spectrum_effect`, `set_wave_effect`, `set_reactive_effect` and
`set_starlight_*_effect` are all in the V4 Mini's method list), with the transaction id `1f` on the cable and `9f`
through the receiver, one request each. PeriHub sends them the same way and offers every one of them:

| Effect | Number | Size | Arguments 3 to the end | Values |
|---|---|---|---|---|
| none (off) | `00` | `06` | `00 00 00` | |
| static | `01` | `09` | `00 00 01`, r g b | one colour |
| breathing, random colours | `02` | `06` | `00 00 00` | |
| breathing, one colour | `02` | `09` | `01 00 01`, r g b | |
| breathing, two colours | `02` | `0c` | `02 00 02`, r g b, r g b | |
| spectrum | `03` | `06` | `00 00 00` | |
| wave | `04` | `06` | direction, `28`, `00` | direction `01` (OpenRazer's client: to the right, left to right) or `02` (to the left), the two the daemon accepts for this keyboard; the speed byte is always `28` (lower is faster): the effect takes no speed |
| reactive | `05` | `09` | `00`, speed, `01`, r g b | speed 1 to 4: 500, 1000, 1500, 2000 ms (OpenRazer's client constants) |
| starlight, random colours | `07` | `06` | `00`, speed, `00` | speed 1 to 3: fast, normal, slow |
| starlight, one colour | `07` | `09` | `00`, speed, `01`, r g b | |
| starlight, two colours | `07` | `0c` | `00`, speed, `02`, r g b, r g b | |

The driver clamps the speeds and the wave direction to those ranges; PeriHub refuses any other value instead. OpenRazer's
daemon also offers a "ripple" effect: it is not a command of the keyboard, it is the daemon drawing rings on the custom
frame around the keys that were just pressed, from the key presses it reads. PeriHub never reads what is typed, so it
leaves ripple out.

**No command reads the effect in use.** OpenRazer's driver creates the files of these effects write-only for the V4 Mini,
and its daemon has no method that reads the effect: it keeps its own record of the effect, colours and speed it sent last
(its persistence config, "state for persisting across sessions"). PeriHub does the same, and is open about what that can
and cannot do:

- PeriHub keeps the last effect it sent, with its colours and speed, and up to 8 before it, in `remembered-settings.json`
  in its settings folder, by USB id (`1532:02b9` on the cable, `1532:02ba` through the receiver: the two connections do
  not share it, and nothing tells two units of one model apart).
- **Put back** sends the effect before the last one again, with the bytes of the first time, and drops the last. Every
  change after the first one can be undone this way, one step at a time.
- The effect the keyboard had before PeriHub's first change was set by Razer Synapse (or is the keyboard's own default)
  and **cannot be read, so it cannot be put back**. The page says so before the first change and suggests noting that
  effect in Synapse first. What PeriHub last sent is out of date if the effect was changed elsewhere since.
- The effects are stored changes (storage `01`): the No warranty box the first time, as for the brightness. There is no
  15-second countdown: nothing reads the effect back to check it.
- The test report lists each lighting command with its bytes, the transaction id and the keyboard's answer: `accepted
  (status 02)`, `not accepted` with the status the keyboard gave (`05` is "not supported"), or no answer. A refused
  effect is not remembered.

### The macro light

OpenRazer's driver reads and writes the macro light for the V4 Mini (`macro_led_state` and `macro_led_effect` files, and
`get_macro_mode`, `set_macro_mode`, `get_macro_effect`, `set_macro_effect` in the daemon). LED id `07`.

| Class, command | Size | Arguments | Transaction id | What |
|---|---|---|---|---|
| `03 80` | `03` | `01`, `07` | `ff` | Reads the state: `01` on, `00` off, in the third argument |
| `03 00` | `03` | `01`, `07`, state | `ff` | Sets the state, stored |
| `03 82` | `03` | `01`, `07` | `ff` | Reads the effect: `00` static, `01` blinking (the client accepts only these two; the driver clamps to 5) |
| `03 02` | `03` | `00`, `07`, effect | `1f` on the cable | Sets the effect **without storage** |
| `03 02` | `03` | `01`, `07`, effect | `ff` through the receiver | Sets the effect **stored** |

The state is stored and has a read, so PeriHub treats it like the game mode: backup, read-back and the 15-second undo. The
effect through the receiver is stored and read with the same transaction id, so it is treated the same way. On the
cable OpenRazer sends the effect without storage, so the stored effect that the read returns is not what that write
changes: PeriHub offers it as a setting that cannot be read back ("Macro light effect, not saved"), with the warning
always on screen, and does not read the stored effect there. A V4 Mini has no key row for macros, so the keyboard may
not have this light at all: if it answers the read with status `05`, or with an answer that does not look right, PeriHub
leaves the macro light out, shows why and sends nothing to it.

## Left out for now

- **The ripple effect**, as above: it needs the key presses.
- Colours for single keys or groups: the frame's rows and columns are known, but no source PeriHub may use says which
  key sits at which position of the V4 Mini's matrix.
- The device mode, key remapping, the Hypershift layer, macros and onboard profiles. Key remapping,
  the Hypershift layer and profiles are not described by any source PeriHub may use: they would need USB captures
  of Razer Synapse from a tester. OpenRazer gives the V4 Mini no polling rate, no idle time and no low-battery
  threshold.
- Changes on the other keyboards: the same commands exist for many of them, but PeriHub changes none until a
  tester has confirmed the V4 Mini.
