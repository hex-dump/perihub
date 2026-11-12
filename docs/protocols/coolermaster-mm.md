# Cooler Master MM530, MM711, MM720 and MM730 (lighting)

How PeriHub reads and changes the lighting of four wired Cooler Master mice. **No mouse has been tried with PeriHub
yet**: every change is experimental until someone confirms it.

Sources, facts only (OpenRGB is GPL-2.0-or-later: no code was copied or translated):

- The USB captures of Cooler Master's own app attached to OpenRGB's device requests: issue 1782 (MM720: report
  descriptors, static red, green and blue, off, brightness, breathing speed, static and breathing in white) and issue
  2305 (MM530: colours, switching modes, saving).
- [OpenRGB](https://gitlab.com/CalcProgrammer1/OpenRGB), commit `4e7aa49`, `Controllers/CoolerMasterController`: which
  product ids belong to this family, and that it leaves the MM531 turned off.

## Which mice

| USB id | Mouse | Command byte |
| --- | --- | --- |
| `2516:0065` | MM530 | `2C` |
| `2516:0101` | MM711 | `2B` |
| `2516:0141` | MM720 | `2B` |
| `2516:0165` | MM730 | `2B` |

Left out: the MM531 (`0097`), which OpenRGB lists but keeps disabled, and the MM712 (`0169`), which speaks another
protocol on another interface.

## Interfaces

The MM720's report descriptors, read from the capture:

- interface 0: the mouse (buttons, X/Y, wheel, horizontal scroll);
- interface 1: **one vendor collection only**, usage page `FF00`, usage 1, a 64-byte input report and a 64-byte output
  report, without report ids;
- interface 2: system control, consumer control and a keyboard with report ids 1, 2 and 4: what a button bound to a
  key types.

So PeriHub talks to interface 1, the `FF00` collection, and the Linux udev rule opens interface 1 only. Interface 2
is never opened. Only the MM720's descriptors were seen; on the other three PeriHub asks for the same usage page too.

## Transport

The host writes a 64-byte output report (report number 0, so 65 bytes go to hidapi) and the mouse answers with a
64-byte input report. The same interface also sends reports of its own (`43 01 …`, about the DPI button), so an
answer is taken only when its first two bytes are the request's.

In the captures the official app sends `41 80` before every other command, reads included, and the mouse echoes it.
What it does is not documented. PeriHub sends it the same way, but counts it as a change: a read-only connection
(the command line) never sends it, and then shows nothing but the reason.

## Commands

Offsets below count from the first byte after the report number.

| Request | Answer | What |
| --- | --- | --- |
| `41 80` | echo | sent before every command |
| `52 28` | byte 4: the mode in use | read the mode in use |
| `52 CB 00 00 <mode>` | bytes 4–12 as in the write below | read the stored settings of one mode (`CB` = the model's command byte) |
| `51 CB 00 00 <mode> <speed> <flag> <a> <b> <brightness> <R> <G> <B>` | echo | write the stored settings of one mode |
| `51 28 00 00 <mode>` | echo | select the mode in use |
| `41 01`, then `50 55`, then `41 00` | echo | store everything in the mouse's memory |

Modes: `00` steady, `01` breathing, `02` colour cycle, `03` stars, `04` indicator, `B0` custom per zone, `E0`
multilayer, `FE` off. Each mode keeps its own speed, brightness and colour: the MM720 read steady as speed `03`,
flag `00`, `FF FF`, brightness `FF`, red; breathing as speed `31`, flag `20`; colour cycle as speed `28`, brightness
`7F`. The brightness is 0–255.

Never sent: `51 A8` (colours per zone), `51 30` and `51 A0` (zone and multilayer set-up), `41 03`, `52 18`, `52 10`
and `51 10` (buttons and other settings).

## How PeriHub changes them

Every change goes through the official app's save sequence: `41 80`, the mode's settings (`51 CB`), `41 80`, the mode
(`51 28`), then `41 01`, `50 55`, `41 00`. Since every change is stored, only **Save to memory** is offered, after
the No warranty box, with backup, read-back and the 15-second undo.

- Modes offered: steady, breathing, colour cycle and off. In stars, indicator, custom or multilayer the lighting is
  shown and not changed: putting such a mode back would need commands PeriHub does not send.
- PeriHub reads the settings of the mode in use and of the steady, breathing and colour cycle modes. A change of
  colour or brightness rewrites the settings of the mode in use, with its own speed, flag and the two bytes after
  it as read. A change of mode only selects the other mode, which keeps its own stored settings: the page shows them
  after the change. Off shows the steady mode's colour and brightness.
- The brightness is shown in percent; 0–100 % and back to 0–255 is exact. An unchanged brightness is written back
  with the byte as read.
- An answer whose first bytes are not the request's, a mode outside the list above, or a mode whose answer does not
  repeat it leaves the mouse read-only, with the bytes in the test report.

## Not done

Speed, the stars and indicator colours, custom colours per zone, buttons, DPI. The MM530's reads were never
captured: if its answers do not look like the MM720's, its page stays read-only.
