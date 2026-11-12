# Pulsar CrazyLight and Lamzu Atlantis on their cable (the 17-byte "Nordic" protocol)

How PeriHub talks to three wired mice whose firmware shares one configuration protocol: the Pulsar X2 CrazyLight,
the Pulsar X3 LHD CrazyLight and the Lamzu Atlantis. **No mouse has been tried with PeriHub yet**: every change is
experimental until someone confirms it.

Sources:

- [pulsar-mouse-linux](https://github.com/packerlschupfer/pulsar-mouse-linux) (MIT, © 2026 Packerlschupfer), commit
  `037cefe`: `src/pulsar_mouse/drivers/nordic.py`, `x2_crazylight.py`, `x2_crazylight_wired.py`,
  `x3_lhd_crazylight.py` and `docs/protocol-x2-crazylight.md`, built from USB captures of Pulsar's app with screen
  recordings. Its README says reads and writes of both CrazyLight mice were confirmed on hardware.
- [lamzu-cfg](https://github.com/LeadSun/lamzu-cfg) (MIT or Apache-2.0, © 2024 its contributors), commit `51c0879`,
  `src/atlantis.rs` and `src/atlantis/hid.rs`: the same frames, checksum and addresses on the Lamzu Atlantis, tried
  by its author on the Atlantis and the Thorn.

## Which mice

| USB id | Mouse | Profiles | DPI | PeriHub |
| --- | --- | --- | --- | --- |
| `3710:3414` | Pulsar X2 CrazyLight (cable) | 4 | 10–32000 | reads, and changes with Save to memory |
| `3710:3527` | Pulsar X3 LHD CrazyLight (cable) | 4 | 10–32000 | reads, and changes with Save to memory |
| `3554:f50f` | Lamzu Atlantis (cable) | read only | 50–26000 | reads only |

The Lamzu is read only: its vendor id (`3554`) is shared by other brands' mice, and only one small project has
tried its writes. The wireless dongles are left out: they need a link check and a model query, and their ids are
shared by many models.

## Transport

Interface 1. On these mice it is not a configuration-only interface: it also carries the mouse's own input (buttons,
wheel, consumer keys its buttons may send). It never carries a keyboard's typing, as for the Logitech G600.

A frame is a 17-byte report with **report id 8**, both ways:

```
byte 0     08
byte 1     command
byte 2     00 (status in an answer)
bytes 3-4  address, high byte first
byte 5     length
bytes 6-15 data
byte 16    checksum: 0x55 minus the sum of bytes 0-15, modulo 256
```

The host writes it as an output report; the mouse answers with an input report of the same command. PeriHub takes
an answer only when its checksum is right, its command is the request's, and, for a memory read, its address and
length are the request's. The mouse also sends `0A` frames of its own (an event: the DPI button, a profile switch):
they are skipped.

| Command | What |
| --- | --- |
| `08` | read up to 10 bytes of memory at the address |
| `07` | write `len` bytes of memory at the address |
| `0E` | the profile in use: byte 6, counting from 0 |
| `12` | firmware: bytes 6 and 7, packed decimal ("3.05") |

Never sent: `09` (factory reset), `0F` (switch profile), `01` and `02` (model query and configuration mode, for the
dongles), and any write outside the addresses below. PeriHub's list of commands names each writable address, so a
write anywhere else cannot leave the computer.

## Memory (the profile in use)

The memory window always holds the profile in use. Most one-byte settings are stored as a pair `(v, 0x55 − v)`;
a record ends with `0x55` minus the sum of its other bytes. PeriHub checks every pair and every record it reads: one
that does not add up leaves the mouse read-only.

| Address | Setting | Encoding |
| --- | --- | --- |
| `00` | polling rate | `01` 1000, `02` 500, `04` 250, `08` 125, `10` 2000, `20` 4000, `40` 8000 Hz (pair) |
| `02` | DPI stages | how many (pair) |
| `04` | DPI stage in use | from 0 (pair) |
| `0A` | lift-off distance | CrazyLight: `01` 1 mm, `02` 2 mm, `03` 0.7 mm; Lamzu: `01`, `02` (pair) |
| `0C` + 4·n | DPI stage n | `[x, y, high, check]` |
| `A9` | debounce time, ms | CrazyLight 0–20, Lamzu 0–15 (pair) |
| `AB` | motion sync | 0 or 1 (pair) |
| `AF` | angle snapping | 0 or 1 (pair) |
| `B1` | ripple control | 0 or 1 (pair) |

### DPI

`high` holds the same 4-bit value twice. Rotated left by two bits it reads `mode` (2 bits) and `page` (2 bits), and

```
DPI = (x + base + 256 × page) × step
```

| Mouse | mode | step | base | range Pulsar's app uses it for |
| --- | --- | --- | --- | --- |
| CrazyLight | 0 | 10 | 1 | up to 10240 |
| CrazyLight | 2 | 50 | 201 | up to 25600 |
| CrazyLight | 3 | 100 | 201 | above |
| Lamzu | 0 | 50 | 1 | all |

The source's 14 pairs read off Pulsar's app all decode this way (`27 27 00` 400, `3F 3F 44` 3200, `37 37 22`
12800, `17 17 66` 24000, `77 77 33` 32000…), and PeriHub's tests use them. lamzu-cfg writes the low byte only, which
holds to 12800; PeriHub reads the Lamzu with the formula above. A stage whose X and Y differ is shown and not
changed. PeriHub writes a DPI in the finest mode that reaches it, as Pulsar's app does, so above 10240 DPI it takes
steps of 50 and above 25600 steps of 100.

## How PeriHub changes the CrazyLight

Every write is stored by the mouse: only **Save to memory**, after the No warranty box, with backup, read-back and
the 15-second undo, on the profile in use only (an undo never writes onto another profile).

- Polling rate, lift-off distance, debounce, motion sync, angle snapping, ripple control and the DPI of each stage.
  How many stages there are and which is in use are not changed.
- On the cable the mouse runs at 1000 Hz at most, but the profile may hold a higher rate for the dongle: then the
  polling rate is shown and not changed, so the dongle's rate is never cut back.

## Not done

Lighting, buttons, macros, sleep timers, the other profiles, the dongles, the Feinmann mice (their 1-DPI encoding is
not documented in these sources).
