# Recording what the official software sends to a device

PeriHub can only change a setting on a device when it knows the exact messages the official software
sends for it. When there is no public documentation, those messages have to be recorded from a real
device, by its owner, while the official software changes one setting at a time. This guide shows how.
It is the same method on every system; only the tool changes.

**Captures are optional.** A device request counts without them: a maintainer first looks for public
documentation of the device. This guide is for people comfortable with it who want to speed things up.

You do not need to understand the recording. You need to change one thing at a time and write down what
you changed and when.

What a recording is **not** used for: PeriHub does not read, copy or decompile the official software.
Only the messages that travel between it and your device are used.

## The method (all systems)

1. Close PeriHub, so that only the official software talks to the device.
2. Start the recording.
3. Do **one** change in the official software (for example "DPI 800 to 1600"). Wait about 3 seconds.
4. Write down what you changed and the time (or press a marker, see the web recorder below).
5. Repeat for each setting the software has: DPI levels, polling rate, lighting (each effect, colour,
   speed, brightness), button or key assignments, macros, profiles, battery or sleep settings,
   sensitivity, and so on. Do each one a few times with different values: two different values show
   which bytes carry the number.
6. At the end put every setting back as it was (take screenshots of the software's pages first).
7. Stop and save the recording, then attach it to the issue (see the end of this page).

Tips: a short recording per setting is easier to read than one long recording. Do not unplug the
device during a recording, and do not use the device for anything else (do not move the mouse a lot,
do not type on a keyboard) except what the step needs.

## Windows: USBPcap and Wireshark

1. Install [Wireshark](https://www.wireshark.org/download.html). In its installer, tick **USBPcap**
   (or install [USBPcap](https://desowin.org/usbpcap/) separately) and restart Windows if it asks.
2. Find the USB bus of the device. Plug the device in, open Wireshark as administrator and look at the
   USBPcap interfaces (USBPcap1, USBPcap2, ...). Wireshark shows a small activity graph next to each:
   move the mouse or press the software's buttons and see which one moves. A USB hub or receiver is on
   the same bus as what is plugged next to it.
3. Capture **only that bus**: double-click its USBPcap interface. A capture of every bus is much
   bigger and can contain other devices.
4. Find the device address, so that you can filter. Replug the device while the
   capture runs, then search the packets for its vendor and product id (in the filter bar:
   `usb.idVendor == 0x046d`, with your vendor id). The "Source" column of that "GET DESCRIPTOR" answer is
   the address, for example `1.7.0`: the middle number (7) is the device address.
5. In the Wireshark filter bar, type `usb.device_address == 7` (use your address) and press Enter. The
   filter only changes what you see: the saved file can still hold the whole bus. To save only the
   device's packets, use **File, Export Specified Packets** with "Displayed" selected.
6. Start the capture, do one change in the official software, write it down with the time (the **Time**
   column of Wireshark, or your clock: start a stopwatch when you start the capture).
7. Stop the capture (red square) and save with **File, Save As** as `.pcapng`.

## Linux: usbmon and Wireshark

1. Install Wireshark (`sudo apt install wireshark`, `sudo pacman -S wireshark-qt`, or your system's
   package).
2. Load the kernel module: `sudo modprobe usbmon`.
3. Let your user read it. Either run Wireshark with `sudo` for this recording, or give your user access
   (a udev rule or the `wireshark` group, depending on the distribution: the package usually explains it).
   The devices appear as `/dev/usbmon1`, `/dev/usbmon2`, ...
4. Find the bus of the device: `lsusb` shows `Bus 003 Device 007: ID 046d:c08b ...`. Capture on the
   interface `usbmon3` (the bus number) and not on `usbmon0`, which holds every bus.
5. Filter by the device number, `usb.device_address == 7` in the Wireshark filter bar, and continue with
   the method above. `File, Export Specified Packets` with "Displayed" saves only the device.
6. Save as `.pcapng`.

## Web configurators (WebHID pages in Chrome or Edge)

Some makers use a web page to configure the device (the page asks you to "connect" the device in the
browser). USB capture tools see this too, but a recorder in the page's own console is easier: it
writes down every message the page sends to the device and receives from it, as text, with the time.
It needs Chrome or Edge (they have WebHID); Firefox and Safari do not.

1. Close PeriHub. Open the configurator page in **one** tab.
2. Do **not** connect the device on the page yet.
3. Press **F12**, click the **Console** tab. If the browser says you must type `allow pasting` before
   pasting, type that and press Enter.
4. Paste the script below, press Enter, and check that the console says `Recorder ready`.
5. Connect the device on the page as usual. Do not reload the page afterwards: a reload loses the
   recording (if it happens, paste the script again and go on from the step you were on).
6. Before **each** change, type `mark('what you are going to change')` in the console and press Enter,
   **then** make the change on the page, then wait about 3 seconds. Write names and numbers exactly
   as the page shows them, for example `mark('lighting effect: Wave')` or
   `mark('polling rate 500 Hz')`. One change per note. If you click something by mistake,
   `mark('oops, clicked X by mistake')` and carry on. The up arrow repeats the last command.
7. At the end run `save('my-device')`: a file `my-device.txt` is downloaded. You can run it halfway
   as a backup (the second file then holds everything from the start).

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

The script does not change anything on the device by itself, and sends nothing anywhere: the recording
stays in the page until you run `save`. It only works for devices the page opens through WebHID. If the
page uses another way (WebUSB, or a program on your computer), use the USB capture above.

## What is recorded and what is not

- **A USB capture** holds every packet of the bus you captured: the device's messages and also the
  descriptors it announces. It can hold the device's **serial number** (in its string descriptors) and,
  on the same bus, other devices. Capture only the device's bus, export only its packets, and if you
  want, look for your serial number in the file (Wireshark's **Edit, Find Packet** in strings) and tell us
  rather than sending it. We do not need it.
- **A keyboard** sends a message for each key you press. A capture or recording made while you type
  holds what you typed, including passwords. **Do not type** while recording (use the mouse for the
  official software's pages), and do not log in to anything. Do not press the keys of a keyboard that
  is on the bus during a capture, except for a step that asks for it.
- **The web recorder** holds only the hex messages with their times and your notes: not the page, not
  cookies, not your computer. The messages of a keyboard's key presses do show as received reports,
  so the same advice applies.
- Open the file in a text editor or Wireshark before sending it: you should recognise your own notes.

## Send the recording

1. Name the file after the device (for example `my-mouse-dpi.pcapng`). A `.pcapng` or `.txt` file can be
   attached to a GitHub comment by dragging it into the comment box. If GitHub refuses a large file,
   compress it first (a `.zip` attaches fine).
2. In the same comment write: the device and its USB id, the version of the official software, what you
   changed and in which order (the list you wrote down, with the times), and the settings you put back
   at the end.
3. A part is enough. Say which settings you did not manage to record.
4. Attach screenshots of the official software's pages, so the names of the modes and options can be
   matched to the numbers in the recording.
