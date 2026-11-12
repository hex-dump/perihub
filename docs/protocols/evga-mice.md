# EVGA X12 and X20 mice on their cable: lighting

How PeriHub reads the lighting of two EVGA mice, and changes it on one of them. **No mouse has been tried with
PeriHub yet.**

Source: the facts in OpenRGB's EVGA drivers (`Controllers/EVGAUSBController`, OpenRGB commit `4e7aa49d`, GPL-2.0).
OpenRGB's code is GPL, so nothing was copied: this page describes the reports in our own words, and PeriHub's code
(`crates/perihub-core/src/peripheral/evga.rs`) was written from it.

## Which devices

| USB id | Device | Interface | Collection | PeriHub |
| --- | --- | --- | --- | --- |
| `3842:2422` | X12 | 1 | usage page `0008` (LEDs), usage `004B` | reads, and changes with Save to memory |
| `3842:2420` | X20 (cable) | 2 | usage page `FFFF` | reads only |
| `3842:2402` | X20 wireless receiver | 2 | usage page `FFFF` | left out |
| `3842:2608`, `260A`, `260E`, `2610` | Z15 and Z20 keyboards | 1 | usage page `0008`, usage `004B` | left out |

The keyboards are left out: their lighting collection is on interface 1, and no source shows that interface's
whole report descriptor. On Linux a hidraw node opens a whole interface, so if interface 1 also carries keys (many
keyboards put their n-key rollover report there), opening it would let PeriHub see what is typed.

The mice's interfaces may also carry the keys their own buttons can send, as for the Logitech G600.

## X12

Feature report **7**, 40 bytes with the report number, both ways.

```
byte 0     07
byte 1     EA (header)
byte 2     01 get, 02 set
byte 3     01 modes, 02 colours
bytes 4-   the data
```

A read is a set-feature of `07 EA 01 <what>` (zeros after it) followed by get-feature of report 7. The answer may
take a few milliseconds: OpenRGB waits 5 ms and reads it again, up to 5 times, until byte 1 is `EA` and byte 3 is
the command asked for. PeriHub does the same and takes nothing else.

The firmware has **four slots**: 0 front left, 1 front right, 2 wheel, 3 logo. OpenRGB drives three LEDs (front left,
wheel, logo) and writes the second slot as zeros; PeriHub shows the same three zones and writes the second slot
back as it read it.

**Modes** (`01`): byte `4 + slot` is the slot's mode: `00` off, `01` steady. EVGA's own software has other modes,
not documented: when a zone is in one of them, PeriHub shows it and changes nothing on the mouse.

**Colours** (`02`): for each slot, 4 bytes at `4 + 4 × slot`: brightness (0–255), red, green, blue. A write sets
bytes 20–23 to `01` (OpenRGB calls them "start with profile").

A write is `07 EA 02 <what>` with the data, as a set-feature. OpenRGB writes the colours, reads them back and
compares the 16 bytes of the slots, then writes the modes. PeriHub writes the colours only if they change, reads
them back and stops if any byte differs, then writes the modes the same way. Whether the mouse keeps a change
after it is unplugged is not known: PeriHub treats every change as one that stays (No warranty box, backup,
read-back, 15-second undo).

## X20 (cable)

Feature report **0**, 64 bytes after the number. Every request on the cable starts with:

```
00 00 00 1D 02 <command> 02 <LED>
```

where byte 2 is the connection (`00` cable, `02` wireless) and the LED is 0 front, 1 wheel, 2 logo.

**Read** (command `80`): set-feature of the request, then get-feature. The answer gives, counting from the first
byte after the report number:

| Byte | What |
| --- | --- |
| 0 | `A0`: not ready yet, read again; `A4`: asleep (wireless) |
| 8 | mode: 1 steady, 2 breathing, 3 colour cycle, 4 pulse, 6 lights on a click |
| 9 | brightness, 0–100 |
| 10 | speed, 0–100 |
| 12 | when a "click" effect starts and stops |
| 13 | how many colours follow (1–7) |
| 14– | the colours, 3 bytes each |

PeriHub takes an answer only when the mode is one of those, brightness and speed are at most 100, and 1 to 7
colours follow; anything else leaves the page with the reason and no lighting shown.

**Writes are not sent.** OpenRGB sets a mode with command `81` and the colours with command `00` followed by the same
report with byte 0 set to `A1` ("activate"), and reads nothing back to check them. The answers have no documented
header PeriHub could check either, so the X20 stays read only until someone with the mouse sends a test report.
