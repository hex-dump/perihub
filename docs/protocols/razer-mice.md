# Razer mice

How PeriHub talks to Razer mice. **No mouse has been tried with PeriHub yet.** This is written in PeriHub's own words
from facts found in [OpenRazer](https://github.com/openrazer/openrazer) (GPL-2.0, commit 477f4d6): the report layout,
the commands, and per mouse its USB interface, transaction ids and maximum DPI. No code was copied.
`tools/openrazer_facts.py` reads OpenRazer's driver and daemon as text and writes the facts into
`crates/perihub-core/src/peripheral/razer_models.rs`.

## Transport

- One **feature report** of 90 bytes, report number 0, sent to the mouse's USB interface (0, or 3 on some newer mice),
  then read back after a short wait (0.6 ms on most mice, up to 60 ms on some wireless mice on their cable).
- Bytes: status, transaction id, remaining packets (2 bytes, big-endian), protocol (0), data size, command class,
  command id, 80 bytes of arguments, a checksum (the XOR of bytes 2 to 87), and a reserved byte.
- In the answer, the status is `02` when the command was carried out, `01` while the mouse is busy (PeriHub reads
  again, five times at most), `03` failed, `04` timed out, `05` not supported.
- The answer must carry the same remaining packets, class and command as the request. PeriHub refuses one that
  does not, as OpenRazer does.
- Each mouse expects a given **transaction id**, `1f`, `3f` or `ff`, sometimes a different one per command. PeriHub
  uses the one OpenRazer uses for that mouse and command, and does not send a command where OpenRazer does not.

## Commands

| Class, command | What | Arguments |
|---|---|---|
| `00 81` | Read the firmware version | answer: major, minor |
| `04 85` | Read the DPI | `00` (current), or `01` (stored) on two old mice; answer: storage, X and Y, 2 bytes each, big-endian |
| `04 05` | Set the DPI | `01` (stored), X, Y, `00 00` |
| `00 85` | Read the polling rate | answer: `01` 1000 Hz, `02` 500 Hz, `08` 125 Hz |
| `00 05` | Set the polling rate | the same codes |

Everything else (lighting, DPI stages, device mode, macros, firmware) is not in the list of commands PeriHub may
send.

## Saving

OpenRazer's function that builds the DPI command takes a storage argument but always writes `01`, "variable storage":
the DPI goes to the mouse's memory. PeriHub sends it the same way, so it treats every change as one that stays: the
No warranty box the first time, a backup of the settings, a read-back, and the settings put back after 15 seconds
unless the user keeps the change.

## What PeriHub leaves out

- Receivers, docks and the wireless ids of wireless mice: some receivers can be shared with a keyboard. Wireless mice
  are reached on their USB cable.
- Mice whose DPI or polling rate OpenRazer reads with another command (a one-byte DPI, the 8000 Hz polling of newer
  mice): shown where PeriHub can read them, not changed.
- DPI stages, lighting, buttons and battery.
