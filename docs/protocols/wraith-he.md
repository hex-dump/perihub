# Wraith W60 (AULA WIN60 HE platform)

The Wraith W60 (`2e3c:c368`) is a magnetic (Hall effect) keyboard configured from `wraith.software`. `2e3c` is the
vendor id of Artery, the maker of its microcontroller, not of Wraith. It has the configuration collection of the
AULA WIN60 HE (`2e3c:c365`): USB interface 2, usage page `ff1b`, usage `91`. **Nobody has tried it with PeriHub
yet**, and for now PeriHub only reads.

Source, in PeriHub's own words: [AulaControl](https://github.com/sgtflixy/AulaControl) (Apache-2.0, commit
9a526ee), which documents the WIN60 HE from what its web page sends, and confirms the lighting and per-key
actuation on a real keyboard. That the W60 answers the same way is to be checked on a tester's keyboard.

## Transport

- Output report number `01` with 63 bytes; the answer is input report `01` with 63 bytes.
- A request: command, second byte, `00`, an index, then its values. The answer repeats the command and second byte,
  then a length and the values.

## Reads

| Request | Answer |
|---|---|
| `0d 00 00 00` | two answers, in either order: the model (such as `W669,34,KB,…,V3.17.07`) and the build date (`Apr 15 2026,11:20:35`); each has its length at byte 4, then the text |
| `07 01` | the key lighting: mode, speed 0–4, brightness 0–4, colour (RGB), second colour, direction, full colour |
| `08 01` | the same for the side lighting |

PeriHub shows the mode as its number: AULA's mode numbers are known, but whether the W60 uses the same has not
been checked.

## Key travel

While the vendor's travel test runs, the collection sends one report per key movement with how far the key is
pressed (`21 00 00 00 05 01 03 02 <travel>`, in AulaControl's captures). PeriHub never asks for it and skips
such reports. But any program that can open the collection could, so **on Linux PeriHub does not open this
keyboard yet**: it will take a rule the user turns on, as for QMK keyboards. On Windows, programs can open the
collection without any rule.

## Left out for now

Changing the lighting (`07`/`08` with values), per-key colours (`09`), actuation and rapid trigger (`21`), SOCD
(`24`), key remapping and polling rate. AulaControl decodes some of them only partly: they need captures of
`wraith.software` from a tester's own keyboard.
