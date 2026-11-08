# Corsair "Bragi" protocol: what PeriHub relies on

This page describes, in our own words, the facts about the protocol that recent Corsair mice
and keyboards speak, called Bragi by the projects that documented it. PeriHub's code is written
from this page only. No code from other projects was copied or translated.

Sources studied (read only, for the protocol facts):

- [ckb-next](https://github.com/ckb-next/ckb-next) (GPL-2.0): the Bragi constants, how replies
  are checked, how lighting is written, LED counts of the devices it supports.
- [OpenLinkHub](https://github.com/jurkovic-nikola/OpenLinkHub) (GPL-3.0): the HID interface the
  mice use, the keep-alive, LED counts of newer mice.

Nothing here was verified on real hardware by PeriHub yet. Facts that both sources agree on are
marked **(both)**; facts from one source only are marked with it.

## Transport

- Vendor id `1b1c`.
- Mice talk on **HID interface 1** (OpenLinkHub: the first field of its device table is the
  interface number). On Windows an interface can have several collections: which one takes the
  requests is not documented; PeriHub prefers a vendor-defined one (usage page `0xFF00` or above).
- One request, one reply. A request is an output report with report id 0 followed by the packet;
  the reply is read from the same interface.
- Packets are **64 bytes** on most mice. A few use 128 bytes (Scimitar RGB Elite `1be3`, M55
  `2b03`) and some keyboards 1024 bytes **(both)**.
- Unused bytes of a packet are zero.

## Packet layout

Request:

| Byte | Meaning |
|---|---|
| 0 | `0x08`, marks a Bragi packet **(both)** |
| 1 | command |
| 2… | arguments |

Reply:

| Byte | Meaning |
|---|---|
| 1 | the command again: a reply to another command is not ours |
| 2 | status: `0x00` success, `0x05` not supported; `0x03` when opening a handle that is already open (ckb-next) |
| 3… | value |

## Commands

| Command | Name | Arguments | Notes |
|---|---|---|---|
| `0x01` | set property | property, `0x00`, value as 16-bit little endian | **(both)** |
| `0x02` | get property | property | value in reply bytes 3–5, little endian **(both)** |
| `0x05` | close handle | `0x01`, handle | ckb-next |
| `0x06` | write data | handle, data length as 32-bit little endian, then the data from byte 7 | **(both)** |
| `0x07` | continue writing | handle, then more data from byte 3 | for data that does not fit one packet (ckb-next) |
| `0x0d` | open handle | handle, resource as 16-bit little endian | **(both)** |
| `0x12` | keep-alive | none | sent about every 20 seconds while the host controls the device **(both)** |

Other commands exist (reading data, probing a handle). PeriHub does not use them.

## Properties

| Property | Meaning | Values |
|---|---|---|
| `0x01` | polling rate | 1 = 125 Hz, 2 = 250 Hz, 3 = 500 Hz, 4 = 1000 Hz (ckb-next) |
| `0x02` | brightness | 0–1000 (ckb-next) |
| `0x03` | mode | 1 = hardware (the device follows its own settings), 2 = software (the host drives it) **(both)** |
| `0x11` | USB vendor id | read only (ckb-next) |
| `0x12` | USB product id | read only (ckb-next) |
| `0x13` | firmware version | major = byte 3, minor = byte 4, build = bytes 5–6 little endian **(both)** |
| `0x14` | bootloader version | (ckb-next) |
| `0x1e` | active DPI stage | (ckb-next) |
| `0x21`, `0x22` | DPI on X and Y | (ckb-next) |

## Resources

A handle is opened on a resource, then data is written to the handle.

| Resource | Meaning |
|---|---|
| `0x01` | lighting: all red values, then all green, then all blue, one byte per LED **(both)** |
| `0x22` | lighting for some devices, interleaved red-green-blue per LED (ckb-next) |
| `0x05`, `0x06` | wireless pairing id and encryption key (ckb-next). **PeriHub never opens these.** |

## Modes and lighting

- Lighting written by the host shows only in **software mode** (property `0x03` = 2) **(both)**.
- Both projects switch to software mode when they start, send the keep-alive about every 20
  seconds, and put the device back in **hardware mode** (`0x03` = 1) when they stop.
- Writing lighting: open handle 0 on resource `0x01` (if the reply says it is already open, close
  it and open it again), then write `3 × LEDs` bytes to handle 0 with command `0x06`.
- Our reading, not stated by either source: software-mode lighting is not saved to the device's
  memory, since both projects write it again after every connection and neither has a "save"
  step for it. A report on OpenRGB's tracker (issue 4843, Katar
  Pro XT) shows a mouse that kept misbehaving after a replug while the software was still running
  and took it back: the way out is to stop the software, then replug.
- ckb-next switches to software mode even to read properties, and back to hardware mode after.
  Whether reads work in hardware mode is **not known**: PeriHub tries to read without changing
  the mode, and reports what fails.

## Mice PeriHub recognizes

All of them on their USB cable, on HID interface 1 (OpenLinkHub). For wireless mice the product
id below is the one they report with the cable; through their receiver they report another one,
which PeriHub does not handle yet.

| Mouse | Product id | Packet | LEDs | Source of the LED count |
|---|---|---|---|---|
| KATAR PRO | `1b93` | 64 | 1 | both |
| KATAR PRO XT | `1bac` | 64 | 1 | both |
| M55 RGB PRO | `1b70` | 64 | 2 | both |
| M65 RGB ULTRA | `1b9e` | 64 | 3 | OpenLinkHub |
| M75 | `1bf0` | 64 | 2 | OpenLinkHub |
| SABRE RGB PRO | `1b79` | 64 | 5 | OpenLinkHub |
| SABRE PRO CS | `1b7a` | 64 | 1 | OpenLinkHub |
| IRONCLAW RGB WIRELESS | `1b4c` | 64 | 6 | both |
| IRONCLAW WIRELESS SE | `2b32` | 64 | 6 | OpenLinkHub |
| DARK CORE RGB PRO | `1b80` | 64 | 12 | both |
| DARK CORE RGB PRO SE | `1b7e` | 64 | 12 | both |
| HARPOON RGB WIRELESS | `1b5e` | 64 | 2 | both |
| HARPOON V2 | `2b2e` | 64 | 2 | OpenLinkHub |
| M65 RGB ULTRA WIRELESS | `1bb5` | 64 | 2 | OpenLinkHub |
| M65 RGB ULTRA WIRELESS | `1bd6` | 64 | 2 | OpenLinkHub |
| M75 WIRELESS | `1bf1` | 64 | 2 | OpenLinkHub |
| M75 WIRELESS | `2b08` | 64 | 2 | OpenLinkHub |
| M75 AIR WIRELESS | `1bf2` | 64 | 1 | OpenLinkHub |
| NIGHTSABRE WIRELESS | `1bb8` | 64 | 15 | OpenLinkHub |
| SCIMITAR RGB ELITE WIRELESS | `1bdb` | 64 | 4 | OpenLinkHub |
| SCIMITAR ELITE WIRELESS SE | `2b22` | 64 | 4 | OpenLinkHub |
| SABRE RGB PRO WIRELESS | `1b98` | 64 | 4 | OpenLinkHub |
| DARKSTAR RGB WIRELESS | `1bb2` | 64 | 13 | OpenLinkHub |
| SABRE V2 PRO | `2b28` | 64 | 0 | OpenLinkHub (no lighting) |
| SABRE V2 PRO MG | `2b29` | 64 | 0 | OpenLinkHub (no lighting) |
| SABRE V2 PRO CF | `2b25` | 64 | 0 | OpenLinkHub (no lighting) |
| SCIMITAR RGB ELITE | `1be3` | 128 | 5 | both (ckb-next also gives the 128-byte packets) |
| M55 | `2b03` | 128 | 1 | OpenLinkHub |

With 128-byte packets the layout is the same: only the packet is longer, so more data fits in
each write. The Slipstream receiver and the keyboards are left for later.

## Keyboards PeriHub recognizes

On their USB cable, HID interface 1 (OpenLinkHub). Packets are larger on many keyboards: 128,
256, 512 or 1024 bytes (OpenLinkHub; ckb-next agrees on 1024 for the K100, K65 RGB MINI, K70 PRO
and K70 RGB TKL). Where the two sources give different LED counts PeriHub uses OpenLinkHub's,
the more recent one: since PeriHub only shows one colour on every key, a count that is a little
off leaves a few keys unlit at worst, or makes the keyboard refuse the write, which turns the
lighting off.

Key presses arrive on interface 0, and in software mode also on interface 2 (OpenLinkHub listens
there): PeriHub opens neither, only interface 1.

Not known: whether every key still types normally while a keyboard is in software mode.
OpenLinkHub handles some keys itself in software mode (the volume dial and FN layer on the
K70 CORE). The 15-second confirmation gives the keyboard back without any key or click.

| Keyboard | Product id | Packet | LEDs | Source of the LED count |
|---|---|---|---|---|
| K55 PRO RGB | `1ba4` | 64 | 5 | OpenLinkHub (ckb-next: 6) |
| K55 RGB PRO XT | `1ba1` | 64 | 137 | both |
| K55 CORE RGB | `1bfe` | 64 | 10 | OpenLinkHub |
| K55 CORE RGB | `2b05` | 64 | 10 | OpenLinkHub |
| K55 CORE TKL RGB | `2b20` | 64 | 8 | OpenLinkHub |
| K57 RGB WIRELESS | `1b6e` | 64 | 137 | both |
| K60 RGB PRO | `1ba0` | 64 | 123 | both |
| K60 RGB PRO LP | `1bad` | 64 | 123 | both |
| K65 PLUS WIRELESS | `2b10` | 64 | 123 | OpenLinkHub |
| K65 PLUS WIRELESS | `2b11` | 64 | 123 | OpenLinkHub |
| K65 PLUS WIRELESS | `2b23` | 64 | 123 | OpenLinkHub |
| K65 PRO MINI | `1bd7` | 128 | 130 | OpenLinkHub |
| K65 RGB MINI | `1baf` | 1024 | 124 | OpenLinkHub (ckb-next: 123) |
| K65 RGB MINI | `1bc3` | 1024 | 124 | OpenLinkHub |
| K65 RGB MINI | `1bcf` | 1024 | 124 | OpenLinkHub |
| K70 CORE RGB | `1bfd` | 64 | 123 | both |
| K70 CORE RGB | `1bff` | 64 | 123 | both |
| K70 CORE RGB | `2b0a` | 64 | 123 | both |
| K70 CORE TKL | `2b01` | 64 | 125 | OpenLinkHub |
| K70 CORE TKL WIRELESS | `2b02` | 64 | 123 | OpenLinkHub |
| K70 MAX | `1bc0` | 128 | 142 | OpenLinkHub |
| K70 PRO | `1bb3` | 1024 | 142 | OpenLinkHub (ckb-next: 193) |
| K70 PRO | `1bc4` | 1024 | 142 | OpenLinkHub |
| K70 PRO | `1bc6` | 1024 | 142 | OpenLinkHub |
| K70 PRO | `1bd4` | 1024 | 142 | OpenLinkHub (ckb-next: 193) |
| K70 PRO MINI WIRELESS | `1bb6` | 1024 | 130 | OpenLinkHub |
| K70 PRO TKL | `2b14` | 256 | 139 | OpenLinkHub |
| K70 RGB TKL | `1b73` | 1024 | 130 | OpenLinkHub (ckb-next: 193) |
| K70 RGB TKL CS | `1bb9` | 1024 | 130 | OpenLinkHub (ckb-next: 193) |
| K95 RGB PLATINUM XT | `1b89` | 64 | 156 | both |
| K100 RGB | `1b7c` | 1024 | 194 | OpenLinkHub (ckb-next: 193) |
| K100 RGB | `1b7d` | 1024 | 194 | OpenLinkHub (ckb-next: 193) |
| K100 RGB | `1bc5` | 1024 | 194 | OpenLinkHub (ckb-next: 193) |
| K100 AIR WIRELESS | `1bab` | 1024 | 137 | OpenLinkHub |
| MAKR 75 | `2b04` | 512 | 137 | OpenLinkHub |

Older Corsair keyboards (K70 RGB MK.2, K95 RGB PLATINUM, K65 RGB, K68 and others) speak an older protocol, not this one, and are not recognized.

## Slipstream wireless receivers

| Receiver | Product id | Source |
|---|---|---|
| SLIPSTREAM WIRELESS USB Receiver | `1ba6` | both (ckb-next calls it the generic Bragi dongle) |
| SLIPSTREAM WIRELESS USB Receiver | `1bdc` | OpenLinkHub |
| SLIPSTREAM WIRELESS USB Receiver | `2b00` | OpenLinkHub |
| HARPOON RGB WIRELESS receiver | `1b65` | both |
| DARK CORE RGB PRO SE receiver | `1b7f` | both |
| DARK CORE RGB PRO receiver | `1b81` | both |
| IRONCLAW RGB WIRELESS receiver | `1b66` | ckb-next |

The newer "Slipstream V2" receiver (`2b1b`) is handled by a separate driver in OpenLinkHub, with
differences PeriHub does not know: it is left out. So is the K57 receiver (`1b62`), which ckb-next talks to on other
endpoints.

- The receiver takes requests on **HID interface 1**, with 64-byte packets (OpenLinkHub).
- Property `0x36` of the receiver (marker `0x08`) is a bit field: bit *n*, from 1 to 7, is set
  while the device on channel *n* is connected (ckb-next).
- A request to the device on channel *n* is the same packet with the marker byte `0x08 | n`
  (ckb-next: "or" of the channel into byte 0; OpenLinkHub: base `0x08` plus the channel).
- Behind the receiver a device reports the same product id as on its cable (property `0x12`,
  ckb-next; OpenLinkHub matches the receiver's devices with the cable ids). `0xffff` means no
  device (ckb-next).
- PeriHub opens the first connected device of a receiver, with the packet size of the receiver,
  and checks every few seconds that it is still connected. A receiver with more than one device
  connected shows the first one only, for now.

## What PeriHub does with it

- **Always (read-only):** get the firmware version, the mode, the polling rate and the DPI.
  Nothing is set.
- **Experimental lighting, off by default and only for the current session:** switch to software
  mode, open the lighting handle, write one colour to every LED, send the keep-alive every 20
  seconds; switch back to hardware mode when it is turned off, when PeriHub quits, or when any
  command fails. Only commands `0x01` (property `0x03` only, values 1 and 2), `0x02`, `0x05`,
  `0x06` and `0x07` on handle 0, `0x0d` (handle 0, resource `0x01` only) and `0x12` can be sent:
  everything else is refused by the code before it reaches the mouse.
- Lighting is refused while another program that drives Corsair mice runs (iCUE, OpenRGB,
  ckb-next, OpenLinkHub), and given back if one starts. After turning it on, the user has 15
  seconds to confirm that the mouse still works, or it is given back (20 seconds in the core, if
  the window is gone).
