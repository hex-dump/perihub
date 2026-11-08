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
  `2b03`) and some keyboards 1024 bytes **(both)**. PeriHub starts with 64-byte mice only.
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

## Wired Bragi mice (64-byte packets)

| Mouse | Product id | LEDs | Source of the LED count |
|---|---|---|---|
| KATAR PRO | `1b93` | 1 | both |
| KATAR PRO XT | `1bac` | 1 | both |
| M55 RGB PRO | `1b70` | 2 | both |
| M65 RGB ULTRA | `1b9e` | 3 | OpenLinkHub |
| M75 | `1bf0` | 2 | OpenLinkHub |
| SABRE RGB PRO | `1b79` | 5 | OpenLinkHub |
| SABRE PRO CS | `1b7a` | 1 | OpenLinkHub |

Wireless Corsair mice connected with their cable, and receivers (Slipstream), are left for later.

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
