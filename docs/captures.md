# Recording what the official software sends to a device

PeriHub can only change a setting on a device when it knows the exact messages the official software
sends for it. When there is no public documentation, those messages have to be recorded from a real
device, by its owner, while the official software changes one setting at a time. This page shows how.

> [!TIP]
> **Captures are optional.** A device request counts without them: a maintainer first looks for public
> documentation of the device. You do not need to understand the recording either: you only change one
> thing at a time and say what you changed.

PeriHub never reads, copies or decompiles the official software. Only the messages that travel between it
and your device are used.

## Which way to use

| Your situation | Use | What you need |
|---|---|---|
| The official software runs on Windows, or on Linux (also in a Windows virtual machine or in Wine) | **[Let PeriHub record it](#let-perihub-record-it)** (easiest) | PeriHub, not the Flatpak |
| PeriHub cannot record, on Windows | [USBPcap and Wireshark](#windows-usbpcap-and-wireshark) | Wireshark, administrator rights |
| PeriHub cannot record, on Linux | [usbmon and Wireshark](#linux-usbmon-and-wireshark) | Wireshark, `sudo` |
| The device is configured from a web page | [Web recorder](#web-configurators-webhid-pages-in-chrome-or-edge) | Chrome or Edge |

Whichever way you pick, read [what is recorded and what is not](#what-is-recorded-and-what-is-not)
first, and finish with [send the recording](#send-the-recording).

## The rules for every recording

- ☝️ **One change at a time**, then wait about 3 seconds before the next one.
- 📝 **Say what you changed**, with the values the software shows (for example "DPI 800 to 1600").
- ⌨️ **Do not type** while recording, and do not use the device for anything else than the step.
- 🚪 **Close PeriHub** (when you record by hand), so that only the official software talks to the device.
- ↩️ **Put everything back** at the end. Take screenshots of the software's pages before you start.

## Let PeriHub record it

PeriHub makes the recording itself, on Windows and on Linux. Open **Send a report**, find the device and
press **Record the official software**.

1. **Choose the steps.** Tick what the official software can change (DPI, polling rate, lighting,
   buttons, macros, profiles, battery or sleep) and write anything else, one per line. Each one becomes a
   step; the last step is putting back what you changed.
2. **Start.** Read what will happen, tick that you understand, and press **Start recording**.
3. **Go through the steps.** Open the official software yourself. For each step, make the change, wait
   about 3 seconds and press **Done, next step**. You can note what you changed, such as "800 to 1600".
   **Cancel** stops at any time.
4. **Check and send.** The recording goes into the request as text you can read before you send it.
   **Request support** opens the form with it. If it is too long for the link, PeriHub copies the whole
   report and asks you to paste it into **What PeriHub saw**. PeriHub never sends anything itself.

### On Windows: USBPcap, for the recording only

PeriHub uses [USBPcap](https://github.com/desowin/usbpcap), a free USB capture driver by Tomasz Moń
(GPL-2.0, with its BSD-2-Clause USBPcapCMD). It is never shipped with PeriHub: it is installed for the
recording and removed after it.

> [!NOTE]
> Windows asks once for administrator rights, and **USB devices disconnect for a couple of seconds** when
> the recording starts and when it ends. A keyboard or mouse on the same port works again right after.
> No restart of Windows is needed.

<details>
<summary>Exactly what PeriHub does on Windows</summary>

- **Download.** An elevated copy of PeriHub downloads `USBPcapSetup-1.5.4.0.exe` from the project's
  GitHub release into a folder only administrators can write to, and runs it only if its SHA-256 is
  `87a7edf9bbbcf07b5f4373d9a192a6770d2ff3add7aa1e276e82e38582ccb622`.
- **Install.** It installs it, registers the filter driver (`USBPcap.inf`, DefaultInstall) and restarts
  the USB host controllers, so no restart of Windows is needed.
- **Record.** It records the root hubs that have the device with USBPcapCMD, reading each capture through a
  pipe, and keeps only the device.
- **Remove.** At the end it removes USBPcap completely, whatever happened (also on an error or a cancel,
  or when PeriHub closes): USBPcap out of the USB class's UpperFilters (other filters stay), the
  controllers restarted again, its service deleted, `USBPcap.sys`, its Program Files folder and its Apps
  entry. USBPcap's own uninstaller is not enough: it leaves the driver. If the computer was turned off
  during a recording, PeriHub offers to finish the removal at its next start.
- **Your own USBPcap.** A USBPcap you installed yourself (with Wireshark, say) is used and left installed.

</details>

### On Linux: usbmon, with your password

PeriHub reads the device's bus with the kernel's usbmon, through `pkexec`, which asks for your password
once. Nothing is installed: a short script run as root loads usbmon if needed, copies `/dev/usbmonN` to
PeriHub until the recording stops, and unloads usbmon only if it loaded it.

This also works when the official software runs in a Windows virtual machine (with the device passed to
it) or in Wine.

> [!IMPORTANT]
> Not available in the **Flatpak**, whose sandbox cannot read USB traffic. Use the AppImage or the
> `.deb` for the recording.

### What goes into the request

Only the transfers between this device and the official software, one line each (`→` to the device,
`←` from it), step by step, with repeats folded. The raw capture is never written to disk.

| Kept | Always left out |
|---|---|
| The control requests the software sends: SET_REPORT, GET_REPORT, vendor requests | Every other device on the bus |
| Reports from the device's vendor-specific interfaces | Sound and video |
| Reports whose id belongs to a vendor-defined collection of the interface's HID report descriptor | The string descriptors that can hold the serial number |
| | Key presses, mouse movements and media keys, and any report that cannot be told apart from them |

## Doing it by hand: the method (all systems)

1. Close PeriHub, so that only the official software talks to the device.
2. Start the recording.
3. Do **one** change in the official software (for example "DPI 800 to 1600"). Wait about 3 seconds.
4. Write down what you changed and the time (or type a note, see the [web recorder](#web-configurators-webhid-pages-in-chrome-or-edge)).
5. Repeat for each setting the software has: DPI levels, polling rate, lighting (each effect, colour,
   speed, brightness), button or key assignments, macros, profiles, battery or sleep settings,
   sensitivity, and so on. Do each one a few times with different values: two different values show
   which bytes carry the number.
6. At the end put every setting back as it was.
7. Stop and save the recording, then [send it](#send-the-recording).

> [!TIP]
> A short recording per setting is easier to read than one long recording. Do not unplug the device
> during a recording, and do not use it for anything else (do not move the mouse a lot, do not type on
> a keyboard) except what the step needs.

## Windows: USBPcap and Wireshark

**Before you start:** install [Wireshark](https://www.wireshark.org/download.html). In its installer,
tick **USBPcap** (or install [USBPcap](https://desowin.org/usbpcap/) separately) and restart Windows if
it asks.

1. **Find the device's USB bus.** Plug the device in, open Wireshark as administrator and look at the
   USBPcap interfaces (USBPcap1, USBPcap2, ...). Wireshark shows a small activity graph next to each:
   move the mouse or press the software's buttons and see which one moves. A USB hub or receiver is on
   the same bus as what is plugged next to it.
2. **Capture only that bus**: double-click its USBPcap interface. A capture of every bus is much bigger
   and can contain other devices.
3. **Find the device address.** Replug the device while the capture runs, then search the packets for
   its vendor id in the filter bar: `usb.idVendor == 0x046d` (with your vendor id). The **Source** column
   of that "GET DESCRIPTOR" answer is the address, for example `1.7.0`: the middle number (7) is the
   device address.
4. **Filter on it**: type `usb.device_address == 7` (with your address) in the filter bar and press
   Enter.
5. **Record**: do one change in the official software and write it down with the time (the **Time**
   column of Wireshark, or a stopwatch started with the capture). Repeat as in [the method](#doing-it-by-hand-the-method-all-systems).
6. **Save**: stop the capture (red square). To save only the device's packets, use **File → Export
   Specified Packets** with **Displayed** selected, as `.pcapng`.

> [!WARNING]
> The filter only changes what you see. **File → Save As** saves the whole bus, other devices included:
> use **Export Specified Packets** instead.

## Linux: usbmon and Wireshark

**Before you start:** install Wireshark (`sudo apt install wireshark`, `sudo pacman -S wireshark-qt`,
or your system's package).

1. **Load usbmon**: `sudo modprobe usbmon`. The buses appear as `/dev/usbmon1`, `/dev/usbmon2`, ...
2. **Let your user read it.** Either run Wireshark with `sudo` for this recording, or give your user
   access (a udev rule or the `wireshark` group, depending on the distribution: the package usually
   explains it).
3. **Find the device's bus**: `lsusb` shows `Bus 003 Device 007: ID 046d:c08b ...`. Capture on
   `usbmon3` (the bus number), not on `usbmon0`, which holds every bus.
4. **Filter on the device number**: `usb.device_address == 7` in the filter bar, and record as in
   [the method](#doing-it-by-hand-the-method-all-systems).
5. **Save** with **File → Export Specified Packets**, **Displayed** selected, as `.pcapng`.

## Web configurators (WebHID pages in Chrome or Edge)

Some makers configure the device from a web page (the page asks you to "connect" the device in the
browser). USB capture tools see this too, but a recorder in the page's own console is easier: it writes
down every message the page sends to the device and receives from it, as text, with the time. It needs
Chrome or Edge (they have WebHID); Firefox and Safari do not.

1. Close PeriHub. Open the configurator page in **one** tab, and do **not** connect the device yet.
2. Press **F12** and click the **Console** tab. If the browser says you must type `allow pasting`
   before pasting, type that and press Enter.
3. Open **The recorder script** below, copy it (the copy button is at the top right of the box), paste
   it into the console and press Enter. The console says `Recorder ready`.
4. Connect the device on the page as usual. Do not reload the page afterwards: a reload loses the
   recording (if it happens, paste the script again and go on from the step you were on).
5. Before **each** change, write a note in the console, **then** make the change on the page, then wait
   about 3 seconds.
6. At the end save the recording: a text file is downloaded.

| Type in the console | What it does |
|---|---|
| `mark('lighting effect: Wave')` | A note before a change. Write names and numbers exactly as the page shows them, one change per note. |
| `mark('oops, clicked X by mistake')` | A note for a mistake: write it and carry on. |
| `save('my-device')` | Downloads `my-device.txt`. You can also run it halfway as a backup: the second file holds everything from the start. |
| ↑ (up arrow) | Repeats the last command. |

<details>
<summary><b>The recorder script</b></summary>

```js
// HID recorder (PeriHub). Records the reports sent to and received from a WebHID device,
// as hex text, and nothing else. Based on re/capture-instrumentation.js of AulaControl
// (https://github.com/sgtflixy/AulaControl, commit 9a526ee, Apache-2.0, (c) its authors),
// changed to log text lines and to take notes with mark().
(function () {
  if (!navigator.hid) { console.warn('This browser has no WebHID: use Chrome or Edge.'); return; }
  if (window.__rec) { console.log('Recorder already installed.'); return; }
  const lines = [];
  const t0 = performance.now();
  const hex = (buf) => {
    let a;
    if (buf instanceof ArrayBuffer) a = new Uint8Array(buf);
    else if (ArrayBuffer.isView(buf)) a = new Uint8Array(buf.buffer, buf.byteOffset, buf.byteLength);
    else a = new Uint8Array(0);
    return Array.from(a, (b) => b.toString(16).padStart(2, '0')).join(' ');
  };
  const add = (kind, id, text) => {
    const s = ((performance.now() - t0) / 1000).toFixed(3).padStart(9);
    lines.push(`${s}  ${kind}  id=${id}  ${text}`);
  };
  const proto = window.HIDDevice && HIDDevice.prototype;
  if (!proto) { console.warn('No HIDDevice in this browser.'); return; }
  for (const [name, kind] of [['sendReport', 'SEND'], ['sendFeatureReport', 'SENDFEATURE']]) {
    const orig = proto[name];
    proto[name] = function (id, data) {
      add(kind, id, hex(data));
      return orig.call(this, id, data);
    };
  }
  const origRecv = proto.receiveFeatureReport;
  proto.receiveFeatureReport = async function (id) {
    const r = await origRecv.call(this, id);
    add('RECVFEATURE', id, hex(r));
    return r;
  };
  const seen = new WeakSet();
  const watch = (dev) => {
    if (seen.has(dev)) return;
    seen.add(dev);
    dev.addEventListener('inputreport', (e) => add('RECV', e.reportId, hex(e.data)));
  };
  const origGet = navigator.hid.getDevices.bind(navigator.hid);
  navigator.hid.getDevices = async () => { const d = await origGet(); d.forEach(watch); return d; };
  const origReq = navigator.hid.requestDevice.bind(navigator.hid);
  navigator.hid.requestDevice = async (opts) => { const d = await origReq(opts); d.forEach(watch); return d; };
  navigator.hid.getDevices();
  navigator.hid.addEventListener('connect', (e) => watch(e.device));
  window.mark = (text) => { lines.push(`\n=== ${text} ===`); console.log('MARK:', text); };
  window.save = (name) => {
    const body = ['PeriHub HID recording, ' + new Date().toISOString(), ...lines].join('\n') + '\n';
    const a = document.createElement('a');
    a.href = URL.createObjectURL(new Blob([body], { type: 'text/plain' }));
    a.download = (name || 'hid-recording') + '.txt';
    document.body.appendChild(a);
    a.click();
    a.remove();
    console.log('Saved ' + lines.length + ' lines.');
  };
  window.__rec = true;
  console.log('Recorder ready. Use mark("text") before each change and save("my-device") at the end.');
})();
```

</details>

The script does not change anything on the device by itself, and sends nothing anywhere: the recording
stays in the page until you run `save`. It only works for devices the page opens through WebHID. If the
page uses another way (WebUSB, or a program on your computer), use a USB capture instead.

## What is recorded and what is not

> [!WARNING]
> **A keyboard sends a message for each key you press.** A capture or recording made while you type holds
> what you typed, including passwords. **Do not type** while recording (use the mouse for the official
> software's pages), do not log in to anything, and do not press the keys of a keyboard on the same bus
> except for a step that asks for it.

- **A USB capture** holds every packet of the bus you captured: the device's messages and also the
  descriptors it announces. It can hold the device's **serial number** (in its string descriptors) and,
  on the same bus, other devices. Capture only the device's bus and export only its packets. If you want,
  look for your serial number in the file (Wireshark's **Edit → Find Packet**, in strings) and tell us
  rather than sending it: we do not need it.
- **The web recorder** holds only the hex messages with their times and your notes: not the page, not
  cookies, not your computer. A keyboard's key presses do show as received reports, so the warning above
  applies.
- **PeriHub's own recording** leaves out key presses, other devices and the serial number by itself (see
  [what goes into the request](#what-goes-into-the-request)), and you read it before you send it.

Open the file in a text editor or Wireshark before sending it: you should recognise your own notes.

## Send the recording

Drag the file (`.pcapng` or `.txt`, named after the device, for example `my-mouse-dpi.pcapng`) into a
comment on the issue. If GitHub refuses a large file, compress it first: a `.zip` attaches fine.

In the same comment, write:

- [ ] the device and its USB id;
- [ ] the version of the official software;
- [ ] what you changed and in which order (the list you wrote down, with the times);
- [ ] the settings you put back at the end;
- [ ] the settings you did not manage to record (a part is enough);
- [ ] screenshots of the official software's pages, so the names of the modes and options can be matched
      to the numbers in the recording.
