# Logitech HID++ devices on their own USB connection

How PeriHub finds and reads a Logitech HID++ 2.0 device that is not behind a receiver: a keyboard on its
USB cable. PeriHub is for mice and keyboards only; headsets are out of scope. The code is
`crates/perihub-core/src/hidpp/direct.rs` (the table and the probe) and `direct_devices` / `poll_direct` in
`crates/perihub-core/src/manager.rs`. What is read once the device answers is the same as for a device
behind a receiver that PeriHub knows only by its features (`UnsupportedDevice` in
`crates/perihub-core/src/hidpp/receiver.rs`; lighting: [logitech-keyboard-lighting.md](logitech-keyboard-lighting.md)).

**Sources.** Facts only, no code copied. The HID collections each device reports, from its owner's device
request on PeriHub's GitHub ([issue #40](https://github.com/hex-dump/perihub/issues/40)). The Linux kernel's
`drivers/hid/hid-logitech-hidpp.c` (GPL-2.0, master of 2026-11): which device index it sends, and how it picks the
report length. Solaar (GPL-2.0, master of 2026-11, `lib/logitech_receiver/base.py` and `device.py`,
`lib/hidapi/hidapi_impl.py`): the device index it uses for devices without a receiver, the USB id range it
treats as receiverless HID++ 2.0 keyboards, and how it tells a HID++ collection on Windows. No decompiled
Logitech software was used.

## The HID collections

Older Logitech devices and the receivers have their HID++ reports in collections of the vendor usage page
`ff00`: usage `0001` holds the short report (id `10`, 7 bytes) and usage `0002` the long one (id `11`,
20 bytes). The newer devices use the vendor usage page **`ff43`** instead. What PeriHub saw so far:

| Device | USB ID | Interface | Collections |
|---|---|---|---|
| PRO K/DA keyboard, on its cable | `046d:c339` | 0 | `0001/0006` (keyboard) |
| | | 1 | `0001/0006` (keyboard), `000c/0001` (consumer keys), `ff43/0602`, `ff43/0604` |

On Windows each collection has a path of its own, and a write to a collection whose descriptor has no
report with that id fails at once (hidapi gives an error), before anything reaches the device. On the
PRO K/DA (issue #40, Windows) `ff43/0604` refused the long report with "WriteFile: Incorrect function" and
`ff43/0602` answered "HID++ 4.2": as on the G213, whose descriptor gives `0602` the long report (id `11`,
20 bytes) and `0604` the 64-byte one (id `12`, see logitech-g213.md). PeriHub tries the HID++ collections
of the interface in turn, those known to carry the long report first (`ff43/0602`, `ff00/0002`), then
`ff43` before `ff00`, the higher usage first, and keeps the first where the device answers. Solaar does something similar on Windows: it asks each collection for an input report `10` of
7 bytes and `11` of 20 bytes, and takes those that have them as short and long HID++ collections.

On Linux one hidraw node holds every collection of an interface, so the HID++ collections of interface 1
of the PRO K/DA come with what is typed on that keyboard. PeriHub never opens a node that carries a
keyboard's typing: a collection whose path is shared with a keyboard collection (`0001/0006`) is
skipped, and on a model marked as such in the table a collection whose usage hidapi could not read
(usage page 0) is skipped too. The udev rule leaves that keyboard closed. On Windows the HID++
collection is opened on its own, and the keyboard's collections are never opened.

## The device index

A request is `11`, the device index, the feature index, `function << 4 | software id`, then up to 16
bytes of parameters (the long report, as PeriHub sends to every HID++ 2.0 device). Behind a receiver
the index is the receiver's slot. Without a receiver:

- Linux's `hid-logitech-hidpp` sets the index to `ff` in every request it sends; for a device behind a
  receiver `hid-logitech-dj` rewrites it on the way.
- Solaar uses `ff` for a device on its own connection whose description says HID++ 2.0 or newer.

PeriHub asks at `ff`, with the root's "get protocol version" (feature index 0, function 1, a read
that changes nothing, with `5a` as the ping byte the device sends back). A device that answers with
version 2 or newer is read; anything else (an error, a HID++ 1.0 answer, no answer within a second) goes
into the test report as it came, as "usage page ff43, usage 0602, index ff: HID++ 4.2", and the next
collection is tried. A device that has not answered is asked again every 30 seconds.

## What is sent

Only reads, as on any device PeriHub has not tested (`READ_ONLY_CALLS` and `READ_ONLY_GETS` in
`crates/perihub-core/src/hidpp/mod.rs`, enforced by `Access::ReadOnly` for the whole connection):

| Feature | Functions | What PeriHub reads |
|---|---|---|
| `0000` root | 0 get feature, 1 get protocol version | the feature indexes; the HID++ version |
| `0001` feature set | 0 count, 1 get feature id | every feature the device lists, with its flags and version |
| `0003` device info | 0 info, 1 firmware entity | entity count, transports and product ids (the unit id stays out of the report), firmware names and versions |
| `0005` device name | 0 length, 1 name chunk, 2 type | the name and the kind |
| `1004`, `1000`, `1001` | 1, 0, 0 | the battery, whichever the device has, with its reply as it came |
| `8070` colour LED effects | 0 zone count, 1 zone info, 2 effect info | the zones, their location and the effects each offers |
| `8040`, `8071`, `8081`, `4540` | the reads of [logitech-keyboard-lighting.md](logitech-keyboard-lighting.md) | brightness, zones and effects, startup effect, light timers, who drives the lights, per-key bitmap, layout |
| `2201`, `8060`, `8100` | 1; 0; 0 | DPI range, polling rates, onboard memory description, if listed |

The device page never offers a DPI change or **Drive the lights** on such a device (`read_only` in
`UnsupportedDevice`), and the connection would refuse them anyway.
