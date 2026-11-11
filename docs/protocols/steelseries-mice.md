# SteelSeries mice

How PeriHub talks to SteelSeries mice (Rival, Sensei, Aerox, Prime, Kana, Kinzu). **No mouse has been tried with
PeriHub yet.** Everything comes from [rivalcfg](https://github.com/flozz/rivalcfg) (WTFPL, commit 779ea52): its
device profiles, turned into `crates/perihub-core/src/peripheral/steelseries_models.rs` by
`tools/rivalcfg_to_rust.py`, and the reports its tests expect, in `crates/perihub-core/tests/fixtures`. Both are read
as text and never run.

## Transport

- Each mouse is reached through one USB interface, the one rivalcfg names (its `endpoint`): 0 or 3.
- A command is report number 0 followed by its bytes, sent as an **output report** or, on a few mice, a **feature
  report**. Some mice take every command padded with zeros to 32 bytes.
- rivalcfg waits 50 ms after every command. PeriHub does the same.
- Wireless mice, through their own 2.4 GHz receiver, take the same commands with bit 6 of the first byte set (`0x40`).
  The receiver answers each one with 64 bytes, which PeriHub reads and drops.

## What a command looks like

The command bytes, the value, then a suffix on some settings. The value is one of:

- **a choice**: the bytes of a named value, such as polling rate 1000 Hz → `01`;
- **a colour**: red, green, blue;
- **a number in a range**: the step of the input range matched to the same step of an output range, little-endian;
- **a number among choices**: each step of the input range has its own value, such as each DPI step of a sensor;
- **DPI presets**: the number of presets (or a bit per preset on some mice), the preset to select, then each DPI as a
  number in a range or among choices.

rivalcfg rounds a value between two steps to the nearest one. PeriHub refuses it instead, so that what is sent is
always what was asked.

## What PeriHub reads

Only the firmware version and, on wireless mice, the battery:

- `10 00` or `90 00`: firmware, two bytes;
- `92` (two bytes back: bit 7 of byte 1 is charging, the rest is `level / 5 + 1`) or `aa 01` (three bytes: level in
  percent, then charging in byte 2).

The settings themselves **cannot be read back** from these mice.

## Saving

rivalcfg sends a save command after changing settings, and documents its `--no-save` option as "do not persist
settings in the internal device memory": without it, the settings last until the mouse is unplugged or turned off.

PeriHub **never sends the save command** to these mice before someone has tested one: since nothing can be read back,
it could neither back up the saved settings nor undo a save. It is not even in the list of commands PeriHub may send.
A setting PeriHub sends is undone by unplugging the mouse.

## What PeriHub leaves out

Button mapping, colour gradients, reactive colours, separate X and Y DPI, and the settings without a value
(`SKIPPED` in the generated table lists them).
