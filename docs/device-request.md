# What a device request carries

The **Send a report** page of the app (shown in the sidebar when no supported device is connected, and
linked from Settings → Devices) lists the USB and Bluetooth devices PeriHub does not know and opens a
device request on GitHub with what it can read of the one you pick. **Copy report** copies the same text
whole. This page says what is in it and why none of it is risky. The code is in
`crates/perihub-core/src/report_devices.rs` (the list), `hid_descriptor.rs` (the descriptor summary) and
`feature_reports.rs` (the feature reports); the text is built in `app/src/lib/testReport.ts`.

## What is in it

- The name, USB id, bus, release number (bcdDevice) and the number of USB interfaces.
- The HID collections of each interface (usage page and usage).
- The **report descriptor** of each interface: the report ids with the length of their input, output
  and feature reports, and the raw descriptor in hex. On Linux the descriptor comes from `/sys`, which
  needs no rights; on Windows from hidapi, which opens the collection without read or write access when
  the system keeps it to itself.
- The **feature reports (read only)**: the current contents of the feature reports the device declares
  (next section).
- Only if you made one with **Record the official software**: a **USB recording** of what the device's
  official software sends it, under "USB recording of the official software" ([captures.md](captures.md)):
  the device's own transfers only, one line each, without key presses, other devices, sound and video, the
  string descriptors or the serial number. Nothing is recorded unless you start it.
- Never a serial number or a path.

A link longer than about 8000 characters is not opened by GitHub, so the app trims it in this order:
the USB recording (the biggest part, cut short and then left out), then the raw descriptors, then the
feature reports (the section stays, saying it was left out), then the end of the details. The text says so, and **Copy report** always has everything.

## Reading the feature reports

A feature report is a block of state a device keeps and hands over on request (HID specification,
Get_Report with the report type Feature): a firmware version, a chip id, a table of settings. A
maintainer who sees them can often recognize a known controller chip and its protocol without anyone
recording USB traffic. PeriHub reads them like this, and the rules are in the code (`feature_reports.rs`)
and tested:

- **Only GET_FEATURE** (hidapi's `get_feature_report`), once for each report id the descriptor declares,
  with the declared length. PeriHub never sends SET_FEATURE, an output report or anything else, and never
  reads an input report: what you type or how you move the mouse arrives in input reports, so none of it
  can be captured. The reader's interface (`Source`) has no way to write.
- **Only vendor-defined collections** (usage page 0xFF00 and above), which carry a manufacturer's own
  settings and information.
- **A node with a mouse, keyboard, pointer or keypad collection (Generic Desktop usages 1, 2, 6, 7) is
  never opened**, also when it has a vendor collection as well.
- **No extra rights.** The node is opened as hidapi opens any device. On Linux that works only when the
  hidraw node is already open to your user (a udev rule): when it is not, nothing is asked for and
  nothing is read; the text says "not opened". On Windows hidapi opens the collection.
- **Stop at the first error** of an interface, and when a reply does not come within 1.5 seconds (the
  reading runs on a thread of its own, so a device that never answers holds nothing up). The handle is
  closed when the call returns.
- **Bounded:** at most 32 ids per interface, each at most 512 bytes, 1024 bytes in all.
- **Only on that page:** the reading happens while the Send a report page is open (the manager notes
  that the page asked for the list, never in the background), once per interface until the device is
  unplugged.

What the section looks like:

```
Feature reports (read only):
  interface 1, id 20 (3 bytes): 200a0b
  interface 1, id 21 (9 bytes): no answer
  interface 1: stopped after the first error, the other ids were not asked for
  interface 2: not opened (it needs rights PeriHub does not ask for)
```

The bytes are what hidapi returns: the first one is the report id (0 for a device with no ids). The
length is the declared one, with the id byte when the id is not 0. A device whose descriptor declares no
vendor-defined feature report says "none declared by a vendor-defined collection", and "not read"
means the page was not open long enough, or the system gave no descriptor.

Not tested on real hardware yet (needs a device with an openable vendor collection, on Windows and on
Linux with a rule): the behaviour of the get_feature call itself on a real node. The parts around it
are covered by unit tests with a stand-in device.
