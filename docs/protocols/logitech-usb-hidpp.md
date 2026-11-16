# Logitech HID++ devices on their own USB connection

How PeriHub finds and reads a Logitech HID++ 2.0 device that is not behind a receiver: a keyboard on its
USB cable, or a headset through a wireless dongle of its own. The code is
`crates/perihub-core/src/hidpp/direct.rs` (the table and the probe) and `direct_devices` / `poll_direct` in
`crates/perihub-core/src/manager.rs`. What is read once the device answers is the same as for a device
behind a receiver that PeriHub knows only by its features (`UnsupportedDevice` in
`crates/perihub-core/src/hidpp/receiver.rs`; lighting: [logitech-keyboard-lighting.md](logitech-keyboard-lighting.md)).

**Sources.** Facts only, no code copied. The HID collections each device reports, from its owner's device
request on PeriHub's GitHub ([issue #40](https://github.com/hex-dump/perihub/issues/40),
[issue #41](https://github.com/hex-dump/perihub/issues/41)). The Linux kernel's
`drivers/hid/hid-logitech-hidpp.c` (GPL-2.0, master of 2026-11): which device index it sends, how it picks the
report length, and the battery voltage feature 0x1F20 it reads on the G935 headset. Solaar (GPL-2.0, master of 2026-11, `lib/logitech_receiver/base.py` and `device.py`,
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
| G733 headset, through its LIGHTSPEED dongle | `046d:0ab5` | 3 | `000b/0001` (telephony), `000c/0001` (consumer keys), `ff00/0001`, `ff43/0202` |

No public source says which `ff43` usage holds which report, so PeriHub does not rely on the usage numbers.
On Windows each collection has a path of its own, and a write to a collection whose descriptor has no
report with that id fails at once (hidapi gives an error). PeriHub tries the HID++ collections of the
interface in turn, `ff43` before `ff00`, the higher usage first, and keeps the first where the device
answers. Solaar does something similar on Windows: it asks each collection for an input report `10` of
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
  This is also how it reaches the G935 headset (`046d:0a87`) through its USB dongle: the dongle is not
  handled as a receiver, and the headset answers at `ff`.
- Solaar uses `ff` for a device on its own connection whose description says HID++ 2.0 or newer, and
  asks again at `00` when `ff` gets no device.

PeriHub asks at `ff`, then at `00`, with the root's "get protocol version" (feature index 0, function 1, a read
that changes nothing, with `5a` as the ping byte the device sends back). A device that answers with
version 2 or newer is read; anything else (an error, a HID++ 1.0 answer, no answer within a second) goes
into the test report as it came, as "usage page ff43, usage 0604, index ff: HID++ 4.2", and the next
index, then the next collection, is tried. A headset's dongle may not answer while the headset is off:
whether it answers for itself, and at which index the headset answers, is what the first G733 test
report will tell. A device that has not answered is asked again every 30 seconds.

## What is sent

Only reads, as on any device PeriHub has not tested (`READ_ONLY_CALLS` and `READ_ONLY_GETS` in
`crates/perihub-core/src/hidpp/mod.rs`, enforced by `Access::ReadOnly` for the whole connection):

| Feature | Functions | What PeriHub reads |
|---|---|---|
| `0000` root | 0 get feature, 1 get protocol version | the feature indexes; the HID++ version |
| `0001` feature set | 0 count, 1 get feature id | every feature the device lists, with its flags and version |
| `0003` device info | 0 info, 1 firmware entity | entity count, transports and product ids (the unit id stays out of the report), firmware names and versions |
| `0005` device name | 0 length, 1 name chunk, 2 type | the name and the kind |
| `1004`, `1000`, `1001`, `1f20` | 1, 0, 0, 0 | the battery, whichever the device has (in that order), with its reply as it came |
| `8070` colour LED effects | 0 zone count, 1 zone info, 2 effect info | the zones, their location and the effects each offers |
| `8040`, `8071`, `8081`, `4540` | the reads of [logitech-keyboard-lighting.md](logitech-keyboard-lighting.md) | brightness, zones and effects, startup effect, light timers, who drives the lights, per-key bitmap, layout |
| `2201`, `8060`, `8100` | 1; 0; 0 | DPI range, polling rates, onboard memory description, if listed |

Some headsets have only long reports: the kernel sends a long report when the device describes no
short one. PeriHub sends long reports to every HID++ 2.0 device anyway.

## 0x1F20 battery voltage (headsets)

| Function | Parameters | Reply | Kind |
|---|---|---|---|
| 0 get measurement | none | bytes 0–1 the battery voltage in mV (big-endian), byte 2 flags: `01` discharging, `03` charging, `07` full, `0f` unknown | read |

The device also sends event 0 with the same three bytes when they change. The kernel turns the voltage into a
percentage with a table taken from HeadsetControl; PeriHub uses its own curve for a single lithium cell (the one
it uses for 0x1001), so the percentage is marked as an estimate, and shows `0f` as discharging, and the test report has the raw reply
(`1f20: 0f 3c 01 00` is 3900 mV, discharging). A reply of 0 mV is read as no battery reading (the headset off).

The device page never offers a DPI change or **Drive the lights** on such a device (`read_only` in
`UnsupportedDevice`), and the connection would refuse them anyway.
