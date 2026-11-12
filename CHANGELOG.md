# Changelog

## Unreleased

### Added
- **3 Glorious mice (experimental)**, with a cable: Model O/O- (`258a:0036`), Model D (`258a:0033`) and Model O Eternal (`3794:a000`), from the published documentation of their SinoWealth protocol (libratbag, MIT). Nobody has tried them with PeriHub yet. Their page shows the firmware, the DPI of each slot that is on, the slot in use and the polling rate. Every change rewrites their settings in memory, so, as on Razer mice, only **Save to memory** is offered, after the No warranty box, with backup, read-back and the 15-second undo. PeriHub reads the settings again just before, changes only the DPI of the slots that are on and the polling rate, writes every other byte back as it read it, and checks every byte after. A mouse whose firmware is not the one libratbag knows is read for its firmware only.
- PeriHub never writes these mice on top of a reading that differs from the one taken when the mouse was connected anywhere but in the DPI, the polling rate and the slot in use, and reads the block back up to five times after a write: OpenRGB found that a reading right after a write is incomplete, and its zeros must never reach the mouse's memory.
- **On Windows these mice are read-only for now**: there their settings may arrive padded to 520 bytes, and then the length a write must give is unknown.
- The generic SinoWealth ids libratbag also lists (`258a:0027`, `0029`, `0051`, `1007`) are left out on purpose: mice of other brands share them, some with a different protocol, and OpenRGB turned off its SinoWealth keyboards after reused ids bricked Redragon keyboards. Firmware update mode and profile switching are never sent.
- On Linux the udev rule opens these three mice whole, since no source names the interface of their configuration: it carries what the mouse does and the keys its own buttons send, never a keyboard's typing. With the AppImage or the Flatpak, install the rule again.
- **Logitech G600 (experimental)**, from libratbag (MIT), with its interface from OpenRGB. Nobody has tried it with PeriHub yet. Its page shows the profile in use: DPI of each slot, slot in use, polling rate, lighting and buttons. Writing a profile stores it, so only **Save to memory** is offered, after the No warranty box, with backup, read-back and the 15-second undo; PeriHub changes the DPI, polling rate and lighting, writes everything else back as read, never writes on top of a profile that changed since the mouse was connected, and puts back the DPI slot in use. On Linux the udev rule opens its interface 1, which also carries the keys its own side buttons send.
- **Roccat Kone Pure, Kone XTD and Kone EMP, read only**, from libratbag (MIT): the profile in use, its DPI levels, the slot in use and the polling rate. PeriHub writes nothing to them, since the sources disagree on what a write needs; the only report it sends chooses which profile the next read returns. On Linux the udev rule opens their boot mouse interface only.
- **7 Pulsar mice on their USB cable (experimental)**: Xlite Wired (`3710:1401`), X2 Wired (`1402`), X2H Wired Medium (`1403`), X2A Medium Wired (`1404`), Xlite v4 (`3401`), X3 (`3409`) and X3 LHD (`3410`), from pulsar-mouse-linux (MIT), a recent project built from USB captures. Nobody has tried them with PeriHub yet. They store every change by themselves, so only **Save to memory** is offered, after the No warranty box, with backup, read-back and the 15-second undo: polling rate, debounce time, angle snapping, ripple control, motion sync and the DPI of each stage of the profile in use. PeriHub takes an answer only when it repeats the request's category and register, sends a dropped read again only while a freshly plugged mouse has not answered yet, never sends a write twice, and never switches or resets a profile (a reset also resets every global setting). On Linux the udev rule opens their interface 3 only: interface 1 carries the keys their buttons can send. The wireless dongles, the CrazyLight and Feinmann mice and the X2 Wireless are left out.
- **Pulsar X2 CrazyLight (`3710:3414`) and X3 LHD CrazyLight (`3527`) on their cable (experimental)**, and the **Lamzu Atlantis (`3554:f50f`) read only**: one 17-byte protocol, from pulsar-mouse-linux (whose author confirmed reads and writes on both CrazyLight mice) and lamzu-cfg (MIT). Nobody has tried them with PeriHub yet. PeriHub checks every value against the check byte the mouse stores beside it, writes only listed addresses (one command each), and never sends the factory reset or a profile switch. Only **Save to memory**: DPI of each stage, polling rate, lift-off, debounce, motion sync, angle snapping and ripple control; a profile holding a faster rate for the dongle keeps it. On Linux the udev rule opens their interface 1.
- **Lighting of 4 Cooler Master mice (experimental)**: MM530 (`2516:0065`), MM711 (`0101`), MM720 (`0141`) and MM730 (`0165`), from the USB captures of Cooler Master's app attached to OpenRGB's device requests (facts only, no code). Nobody has tried them with PeriHub yet. Colour and brightness of the mode in use, and steady, breathing, colour cycle or off, through the app's own save sequence: only **Save to memory**, after the No warranty box, with backup, read-back and the 15-second undo. An answer is taken only when it repeats the request, and a write only when the mouse echoes it. On Linux the udev rule opens their interface 1, a vendor collection only; interface 2, with the keys their buttons can type, is never opened.
- **Lighting of the EVGA X12 (experimental) and X20 (read only)** on their cable (`3842:2422`, `3842:2420`), from facts in OpenRGB's EVGA drivers (no code copied). Nobody has tried them with PeriHub yet. On the X12, each zone (front, wheel, logo) can be turned off or steady and get a new colour and brightness, only with **Save to memory**, after the No warranty box, with backup, read-back and the 15-second undo; a write the mouse does not read back the same stops there. The X20 shows its lighting and is never written: nothing it answers would confirm a write. The Z15 and Z20 keyboards are left out, since their lighting shares an interface that may carry what is typed. On Linux the udev rule opens the X12's interface 1 and the X20's interface 2.
- **Keyboards with QMK firmware (experimental)**, from facts found in the QMK and Vial sources (no code copied). They have no USB ids of their own: PeriHub finds them by the raw HID channel VIA and Vial use, and lists them on a new **QMK keyboards** page. **Vial** keyboards (by their serial number) are read by themselves; any other keyboard with that channel only after you turn it on as a VIA keyboard. PeriHub shows the protocol versions, layers and macros, and the lighting, which it changes, with Apply or Save to memory, backup, read-back and the 15-second undo, only where the protocol says exactly how: VialRGB, and VIA protocol 12 and 13 (VIA firmware from 2023 on). Keymaps, macros, encoders, the bootloader jump, the memory and keymap resets, the state of the key matrix and Vial's unlock are never sent.
- On Linux, an optional rule per QMK keyboard, `73-perihub-qmk.rules`, written from the QMK keyboards page with your password: it opens only the interface whose descriptor, read in `/sys` first, holds the raw HID channel and nothing else, so what you type stays closed.
- The credits in **Settings** name every source.

### Changed
- **The sidebar lists only the devices that are there**: connected, asleep, or there but not opened (a missing permission, for example), each with its own page. The Apex 3 TKL and the Logitech mouse no longer stay listed as "Not connected", and PeriHub opens on the first device it finds.
- **Send a report**: with no supported device connected, the sidebar offers it instead. It lists the USB and Bluetooth devices PeriHub does not know, with their name, USB id and HID collections, and opens a device request on GitHub already filled in for the one you pick (no serial numbers). Settings → Devices links to it too.
- **Mouse lighting: a "Same effect on all zones" switch.** On, one card sets every zone at once, so breathing and color cycle pulse together (what PeriHub always did). Off, each zone keeps its own effect: choosing breathing or color cycle on the logo no longer changes the DPI indicator too. A profile opens in sync when its zones already show the same effect. Not tried on a mouse yet.

- **Faster updates on Windows**: as soon as PeriHub finds a new version it downloads the installer into its cache folder and checks its signature, so **Update and restart** starts it at once, without asking GitHub again; the button shows the download while it runs. After the update PeriHub comes back with its window, also when it had been started hidden at sign-in. The update runs in the version you have installed, so this speeds up the updates after this version, not the one to it. Not timed yet.

- Dependencies: Vite 8.3.8 and the latest compatible Rust crates (among them wry 0.57.2 and rustls-webpki 0.103.17); the builds run on Node.js 26, the current LTS. Tauri, its plugins, Svelte and the other direct dependencies were already at their latest versions. TypeScript stays on 6.0: svelte-check does not support TypeScript 7 yet.

### Fixed
- Other brands: the profile line said "PeriHub readsand changes only this one".
- Mouse lighting: two quick changes on different zones both reach the mouse; before, only the last one was previewed.
- Other brands: the undo of a change, when its 15 seconds run out or when the device is connected again, never puts one profile's settings onto another. If the device switched profile since the change, nothing is written: PeriHub says which profile to switch back to, and undoes the change when the device is plugged in again on that profile. Until then no other change is made on that device, so the waiting backup is never replaced.

## 0.7.0 — 2026-11-11

129 mice of three more brands, ASUS ROG and TUF, SteelSeries and Razer, from the published documentation of their protocols. Nobody has tried them with PeriHub yet.

### Added
- **21 ASUS ROG and TUF mice (experimental)**, from the published documentation of their protocol (libratbag, MIT), on their cable and through their own wireless receiver: Chakram, Chakram Core, Chakram X, Gladius II (and Origin, PNK LTD), GX860 Buzzard, Harpe Wireless, Keris (and Wireless, Wireless AimPoint), Pugio, Spatha X, Strix Carry, Strix Impact (I, II, II Wireless, III), TUF Gaming M3 Gen II, M4 Air and Mini WL Mouse Miku. Nobody has tried them with PeriHub yet. Each gets a page with what PeriHub read: firmware, active profile, DPI levels, polling rate, button response, angle snapping, lighting and buttons, and a test report with every request and answer.
- On those pages **Apply** changes the active profile's DPI levels, polling rate, button response, angle snapping and lighting without saving them to the mouse's memory, always under a warning, and **Save to memory** stores them, after the No warranty box the first time. Every change is checked against what the mouse accepts, backed up first, read back after and undone after 15 seconds unless kept, also when the mouse is connected again after PeriHub closed. DPI is not changed on mice with separate X and Y DPI, nor brightness on those whose scale is unknown.
- **75 Razer mice (experimental)**, on their USB cable, from facts found in OpenRazer (GPL-2.0; no code copied): the report layout and commands, and per mouse its interface, transaction ids and maximum DPI, extracted by `tools/openrazer_facts.py`. Their page shows the firmware, DPI and polling rate. Since OpenRazer always sends the DPI to the mouse's memory, every change is treated as one that stays: only **Save to memory** is offered, after the No warranty box, with backup, read-back and the 15-second undo. Every answer must match its request and report success. Receivers and docks are left out, since some can be shared with a keyboard.
- **33 SteelSeries mice (experimental)**, from rivalcfg (WTFPL): Rival 3 (and Gen 2, Wireless, Wireless Gen 2), 5, 95, 100, 110, 300, 300S, 310, 500, 600, 650, 700, Sensei 310, RAW and Ten, Aerox 3 and 5 (and Wireless), Aerox 9 Wireless, Prime (Mini, Plus, Wireless), Kana v2 and Kinzu v2, on their cable and through their own receiver. These mice cannot report their settings: their page shows the firmware, the battery of wireless ones, and for each setting what PeriHub last sent. **Send** changes one setting (DPI presets, polling rate, colours, effects, brightness, timers) until the mouse is unplugged, under a warning always on screen. The save command is not even in the list of commands PeriHub may send to them, since nothing could back it up or undo it. The table is generated from rivalcfg's profiles by `tools/rivalcfg_to_rust.py`, and 616 reports are checked byte for byte against rivalcfg's own tests; values between two steps, which rivalcfg rounds, are refused.
- A new layer for devices of other brands: every command goes through a list per protocol, which refuses anything else (firmware, profile switching, button bindings for now) and, on a connection that may not change anything, everything but reads. The command line only reads them. An answer that does not look as documented leaves the device read-only.
- On Linux the udev rule opens only the configuration interface of each new mouse where a source names it: every SteelSeries and Razer mouse, and 22 of the 28 ASUS ids. The other 6 ASUS ids are opened whole. The ROG Omni receiver is never opened, since it is shared with other devices.
- `THIRD-PARTY-NOTICES.md`, with the notices of libratbag and apex-3-tkl (MIT), now in every package with `LICENSE`.

## 0.6.8 — 2026-11-11

PeriHub comes as a Flatpak too.

### Added
- **A Flatpak**, `PeriHub_0.6.8_x86_64.flatpak`, attached to the release like the other packages and built from source by the release workflow (`packaging/flatpak`). It works on any distribution with Flatpak, on the GNOME 51 runtime, which Flatpak downloads from Flathub when PeriHub is installed. PeriHub itself is not on Flathub, so the Flatpak does not update by itself: install the next release's file the same way. Like the AppImage, it needs the udev rule installed by hand (README, USB permissions).
- Inside the Flatpak, autostart goes through the desktop's Background portal; the optional udev rules, which a sandbox cannot install, come with the commands to run instead; and since the sandbox cannot see the other programs running, the Corsair lighting and Logitech DPI pages ask to quit OpenRGB, ckb-next, OpenLinkHub, Solaar and logiops. On a desktop without the Background portal, such as Hyprland, the autostart setting says which command to add to the desktop's own autostart.
- A desktop file and AppStream metainfo for Linux (`packaging/linux`), used by the Flatpak.

## 0.6.7 — 2026-11-11

Logitech mice are supported by the format of their memory, not by model, and the devices on a Bolt or Unifying receiver get a page of their own, with their DPI as a first experimental setting.

### Added
- **Unifying receivers (`046d:c52b`, `c532`) are recognized**, like the Logi Bolt one. Nothing paired with them is supported yet: PeriHub reads them with read commands only, and never sends the receiver's own commands, which pair and unpair devices.
- **Every device that answers behind a Bolt or Unifying receiver gets a page of its own** in the sidebar, mouse or keyboard, built from the HID++ features it reports: what kind of device it is (HID++ 0x0005), what it can do (battery, DPI, SmartShift wheel, high-resolution and thumb wheels, programmable buttons, Easy-Switch, lighting, onboard profiles…), the DPI and polling rates it accepts, its onboard memory format if it has one, and a test report. Nothing on it is changed: this is the first step towards changing their settings, starting with the DPI (issue #17).
- **The DPI of a device on a Bolt or Unifying receiver can be changed (experimental)**, if it has adjustable DPI (HID++ 0x2201) and no onboard profiles, whose active profile would set the DPI instead: on its page, after the **No warranty** box the first time. PeriHub sets it in the device's RAM, never in its memory, and only to values the device says it accepts. Such devices forget it when they are turned off or fall asleep, so PeriHub sets it again when the device connects or wakes up (it checks every 10 seconds), while PeriHub is running, after checking that the device in that receiver slot is still the same one; **Back to the device's own DPI** stops that. The page also shows the device's battery and DPI as they are now, and the test report and the device test form now ask about the DPI. Nobody has tried it on a real device yet (issue #17).
- Each of these devices has its own thread, which notices within 10 seconds when the device falls asleep, and the receivers are asked for them every 10 seconds also while a mouse is open: before, PeriHub looked behind the receivers only while no supported mouse was connected with its cable.
- PeriHub warns, on the device's page, when **Solaar** or **logiops** runs: they set their own DPI and may undo PeriHub's.
- On Linux the udev rule does **not** open the Unifying receivers, on purpose: Linux puts them in DJ mode, where what is typed on the keyboards paired with them arrives on the same interface as HID++, so opening it would let any program read that typing. When PeriHub finds a Unifying receiver it cannot open, the mouse page explains this and offers an **optional rule**, `73-perihub-unifying.rules`, installed through the system's password window only after a warning and a box to tick, and removable from the same place, as for the Apex's key presses. With Solaar installed its own rule already opens the receiver.

### Changed
- **Logitech mice with onboard profiles are supported by memory format.** What decides how a save is laid out is the format of the mouse's memory, which many models share, not the model. PeriHub reads it from the mouse every time it connects (a firmware update can change it), and:
  - a **tested format** makes the mouse supported, whatever the model: format 2 in 256-byte sectors (G502 HERO) and format 3 in 255-byte sectors (G502 X Wireless, issue #15). Profiles can be saved, enabled and disabled, and switched by app profiles, with no warning and no 15-second undo, as on the G502 HERO;
  - a format PeriHub can write but **nobody has tried yet** (format 2 or 3, in 255- or 256-byte sectors) can be edited straight away, with the USB cable: the mouse page says it is experimental and asks for a test report. Before the first save to that mouse PeriHub asks to tick the **No warranty** box, once; every save is still backed up, read back, and undone after 15 seconds unless it is kept;
  - **any other format** stays read-only.
- **Edit anyway…** and **Turn off experimental editing** are gone: there is nothing to turn on any more. The tray menu entry that remains is **Turn off experimental lighting**, for Corsair devices. The list of mice with experimental editing in `config.json` is ignored.
- **Nothing is saved through a LIGHTSPEED receiver**, on any mouse: a write over the radio has never been tried, and the documentation already promised it. Since 0.6.5 the G502 X Wireless could be saved to through its receiver too; now it needs its USB cable, like every other mouse. Through the receiver PeriHub still switches profiles and DPI levels of a mouse with a tested memory format.
- When another program such as G HUB controls the mouse, the mouse page says so on every mouse, experimental ones included. Only on a model tested as a whole (G502 HERO, G502 X Wireless) does PeriHub give the mouse its profiles back by itself, and only there does the notice promise it.
- The HID++ connection to a mouse is always opened read-only, and gets more only after PeriHub has read the mouse's memory format: RAM changes (active profile, DPI level, previews) for a tested format through a receiver, saves to the memory with the USB cable. A memory format PeriHub cannot write is refused by the HID++ layer itself, not only by the mouse page.

## 0.6.6 — 2026-11-10

The Logi Bolt receiver is recognized, a first step for the MX Master 3S.

### Added
- **The Logi Bolt receiver (`046d:c548`) is recognized**, as a first step towards the MX Master 3S and other mice paired with it (issue #17). PeriHub cannot change those mice yet: it reads what the paired device is (name, firmware, the HID++ features it has), with read commands only, and the mouse page offers a test report with it. When PeriHub cannot reach a receiver at all, the report lists the receiver's HID collections, to find out why. As for the LIGHTSPEED receivers, the receiver's own commands, which pair and unpair devices, are never sent. On Linux the udev rule opens only the receiver's HID++ interface.

## 0.6.5 — 2026-11-09

The Logitech G502 X Wireless is supported.

### Added
- **The Logitech G502 X Wireless is supported.** Its profiles, DPI levels, buttons and macros can be edited straight away with **Edit**, without turning on experimental editing or accepting a warning. A user confirmed it on a real G502 X Wireless (format 3, 255-byte sectors), on Windows: the changes took effect, persisted across unplugging and over wireless, the 15-second undo restored the backup when not kept, macros worked, and backups restored cleanly. Issue #15, thanks to Fomius.
- Labels for the 11 buttons of the G502 family on the buttons page (Left/Right/Wheel click, Back, Forward, Sniper / DPI Shift, DPI –/+, Profile switch, Wheel tilt left/right) now apply to the G502 X Wireless as well.

## 0.6.4 — 2026-11-09

The Logitech G300 is supported, and a warning for Logitech Gaming Software.

### Changed
- **The Logitech G300 is supported.** Its profiles can be edited straight away with **Edit**, without turning on experimental editing or accepting its warning. A user confirmed it on a real G300, on Windows: the changes took effect, they were still there after the mouse was unplugged and plugged back in, and a save that was not kept was undone. The safety net stays: a backup of the three profiles before every save, the save read back, and **Keep?** after each one, with the backup going back after 15 seconds without an answer. No G300 has been tried on Linux yet. Issue #21, thanks to omeggga.
- PeriHub warns when **Logitech Gaming Software** is running, as it does for G HUB: it keeps its own copy of a mouse's profiles and may write it back, or take control of the mouse.

### Fixed
- **Unsaved changes on the mouse page were lost when something else changed.** Plugging in the keyboard or another device, or a program such as G HUB starting or quitting, made the mouse page read the mouse again and drop the changes you had not saved yet. It now reads the mouse again only when the mouse itself was opened again or taken by another program.
- A Logitech G300 or a Corsair device that cannot be opened for lack of the udev rule was reported every two seconds, to the window and in the log file, where it pushed everything else out within hours. It is reported once.

## 0.6.3 — 2026-11-09

The first save to a mouse nothing was ever saved to, and experimental editing for mice like the G502 X Wireless.

### Added
- **Experimental editing for mice like the G502 X Wireless.** **Edit anyway…** was offered only when the mouse's memory matched the G502 HERO's exactly (profile format 2, 256-byte sectors). It is now also offered for profile format 3 and for 255-byte sectors, which is what the G502 X Wireless reports: libratbag and Solaar write these with the same layout. As for every untested mouse, it needs the USB cable, every save is backed up and read back, and it is undone after 15 seconds unless you keep it. Saving has not been tried on such a mouse yet. Issue #15, thanks to Fomius.
- The mouse test report says whether the mouse has a profile list of its own, and **Open test form** fills in the title too. The form has three boxes for those who tried saving with **Edit anyway…**.
- `docs/protocols/logitech-onboard-profiles.md` describes the profile list and the factory profiles.

### Changed
- **Saving to the Logitech G300 has worked on a real mouse** (issue #21, thanks to omeggga): the light colour, the report rate and the buttons were saved. It stays experimental until someone confirms that a save is still there after the mouse is unplugged; the notices in the app say so.

### Fixed
- **The first save to a mouse nothing was ever saved to.** Such a mouse has no profile list in its memory and uses its factory profiles. PeriHub wrote the profile but not the list, and showed the profile as saved; going by what libratbag and Solaar do with these mice, the mouse goes on using its factory profiles until it has a list. Now the first save to a G502 HERO also creates the list, with a copy of the factory profiles, so everything you did not change stays as it is, and the mouse switches to its own copy of the profile it was on. Enabling or disabling a profile does the same. While there is no list PeriHub shows the factory profiles and which of them are enabled, says so on the mouse page, and selects profiles among the factory ones. On an untested mouse with **Edit anyway…** PeriHub does not create the list (the save could not be undone) and says how to get one. None of this has been tried on a real mouse with an empty memory yet: the mouse PeriHub is tested on has a list.
- A profile that was never saved gets its factory copy before it is enabled, so the profile list never points to an empty sector.
- A mouse that refuses to read its missing profile list (the G305 does, according to libratbag) is no longer reported as an error.
- The **Enabled** box of a profile goes back when the mouse refuses the change (the last enabled profile, for example): it stayed unticked while the profile was still enabled.
- **Reset all to factory** on a keyboard whose keys are already the factory ones no longer saves a backup of nothing.
- A profile list whose checksum does not match is never overwritten: PeriHub says that the list is damaged and saves nothing, instead of writing a profile the mouse may not use.

## 0.6.2 — 2026-11-09

The Logitech G300 can be edited at your own risk, and G502 HERO mice with 255-byte memory sectors can save.

### Added
- **Experimental editing for the Logitech G300.** **Edit anyway…** on the G300 page lets you change and save its three profiles at your own risk: light colour, report rate, the four DPI levels and the default one, the DPI shift, and what the wheel button and G4 to G9 do (mouse clicks, keyboard keys with Ctrl, Shift, Alt or Win, DPI functions, next profile). The left and right buttons stay as they are. PeriHub writes a profile the way libratbag does, changes only the fields you edited, accepts only values Logitech Gaming Software offers, backs up the three profiles before every save, reads each save back, and undoes it after 15 seconds unless you keep it. Saving has not been tried on a real G300 yet. The G300 page also shows the DPI shift. How the G300 works is described in `docs/protocols/logitech-g300.md`. Issue #16.

### Fixed
- **Some G502 HERO mice could not save.** Some G502 HERO mice (firmware U1 74.00.0005, for example) report 255-byte memory sectors instead of 256, and PeriHub refused every save with "unsupported profile format: 255-byte sectors". The profile has the same layout, only the checksum is one byte earlier: PeriHub now writes these sectors too. Reading was not affected. Issue #20, thanks to danhetrick.

## 0.6.1 — 2026-11-08

A test form for the new devices.

### Added
- **Open test form** and **Copy test report** on the pages of Corsair devices and the Logitech G300. The form opens on GitHub already filled in with what PeriHub read (no key presses, apps or log lines), like the one for Logitech mice: it is how a device becomes fully supported.

### Changed
- The README is shorter and in a clearer order: devices, what PeriHub can do, experimental features with their risks, install, use, safety. The full device list is in `docs/devices.md`, building and the code in `docs/development.md`.

## 0.6.0 — 2026-11-08

More devices: the Logitech G300 read-only, and Corsair mice, keyboards and wireless receivers, experimental. Untested Logitech mice can be edited at your own risk.

### Added
- **Logitech G300 (read-only).** PeriHub shows the three profiles stored in the G300: light colour, report rate, DPI levels with the default one, and what each button does, G4 to G9 as printed on the mouse, plus the active profile and DPI level. Nothing is changed: the G300 code can only read. Checked against a real G300 and Logitech Gaming Software in issue #16, thanks to omeggga.
- **Corsair mice and keyboards (experimental).** PeriHub recognizes 26 Corsair mice that speak Corsair's newer protocol, on their USB cable: KATAR PRO, KATAR PRO XT, M55 RGB PRO, M65 RGB ULTRA, M75, SABRE RGB PRO and SABRE PRO CS, the wireless IRONCLAW, DARK CORE RGB PRO, HARPOON, M65 RGB ULTRA, M75, M75 AIR, NIGHTSABRE, SCIMITAR, SABRE RGB PRO, DARKSTAR and SABRE V2 PRO models connected with their cable, the SCIMITAR RGB ELITE and the M55. Also 23 Corsair keyboards of the same generation (K55 PRO, K55 CORE, K57, K60 PRO, K65 PLUS, K65 MINI, K70 CORE, K70 MAX, K70 PRO, K70 RGB TKL, K95 PLATINUM XT, K100, MAKR 75 and variants), the same way. Several Corsair devices can be connected at once: each gets its own page. Wireless Corsair mice and keyboards are also recognized through their receiver (Slipstream `1ba6`, `1bdc`, `2b00` and the older HARPOON, DARK CORE RGB PRO and IRONCLAW receivers): the first device connected to a receiver. Not tested on any Corsair device yet: PeriHub reads the firmware, polling rate and DPI, and changes nothing. Experimental lighting (one colour on every LED) can be turned on for the session; the device goes back to its own settings when it is turned off, when PeriHub quits or as soon as a command fails, and nothing is written to its memory. On Linux the udev rule opens only the control interface. Reports from Corsair owners are very welcome.
- **Experimental editing for untested Logitech mice.** **Edit anyway…** on the mouse page lets you change and save the profiles of a mouse nobody has tested yet, at your own risk. It is off by default and needs the USB cable and a memory laid out like the G502 HERO's. PeriHub accepts only the DPI and polling values the mouse reports, backs up and reads back every write, and after each save asks whether to keep the changes: without an answer the backup goes back after 15 seconds. Enabling or disabling profiles stays off. The tray menu has **Turn off experimental editing**. The test report says whether it was on.

## 0.5.5 — 2026-11-07

Backups can now be restored from the app.

### Added
- **Restore a backup.** PeriHub has always saved a backup before writing to a device; now you can put one back. **Backups…** next to **Factory settings** on the mouse page, and next to **Reset all to factory** on the keyboard's Keys page, lists them with the date and what was about to change. Restoring writes back only what differs, reads it back to check it, and backs up the current state first, so a restore can be undone too. G502 HERO (profiles, macros, enabled profiles) and Apex 3 TKL (key mapping). Suggested on Reddit.
- `perihub-cli mouse backups`, `mouse restore <id>`, `keyboard backups` and `keyboard restore <id>`.

### Fixed
- perihub-cli and the app could talk to the same device at once, which mixes their requests and can make a save fail halfway. perihub-cli now refuses to start while the app is open, and the app waits for a running perihub-cli to finish before opening the devices.

## 0.5.4 — 2026-11-06

A small update: every dependency is at its latest version, and PeriHub has a website, https://hex-dump.github.io/perihub/.

### Fixed
- Windows: starting PeriHub while another copy was still starting could leave the second copy open instead of handing over to the first (tauri-plugin-single-instance 2.5.5).

### Changed
- All Rust and npm dependencies updated, among them Tauri 2.12.5, tauri-plugin-updater 2.14, windows-registry 0.100 and dirs 7. Settings, backups and logs stay in the same folders. TypeScript stays at 6, which svelte-check still needs.
- Mice with a single onboard profile, like the G403 HERO (#14), go back to their saved settings after a preview through host mode, since they have no other profile to switch to. Nothing changes for users yet: those mice are still read-only.

## 0.5.3 — 2026-11-05

Fixes from the first test reports of other mice, the G502 X and the G403 HERO. They have tests with bytes like the ones these mice use, but have not been tried on the mice themselves yet: their owners are asked in #14 and #15. On a G502 HERO and an Apex 3 TKL, PeriHub reads exactly what 0.5.2 read.

### Fixed
- Mice whose onboard memory has sectors smaller than 256 bytes, like the G502 X, showed only "unsupported profile format" (#15). PeriHub now reads their profiles when the sectors hold every field it shows, and they stay read-only. Otherwise the message gives the sector size.
- Special button functions, such as DPI cycle, showed as "Custom value" when the mouse stores them with other values in their last two bytes. This is most likely the DPI button of the G403 HERO in #14. When a profile is saved, an unchanged special function keeps its bytes as they were.
- The firmware line no longer lists the empty entries that some mice report (00.00.0000).

### Changed
- The test report also gives the format of the onboard memory: profile and macro format, number and size of its sectors, factory profiles.

## 0.5.2 — 2026-11-01

An easier way to report how a mouse works, and two small changes to the interface. The test report buttons and Settings were tried on Linux and on Windows 11 in a virtual machine, with a G502 HERO. The messages next to Save have automated tests only.

### Added
- **Copy test report** and **Open test form**, in Settings → Problems and in the notice of the experimental mice. The report has the version, the system, the model, the USB ID, the connection, the firmware, the profiles, the DPI range and the LED zones, with no key presses, apps or log lines. Through a LIGHTSPEED receiver it also names the receiver, the slot and the wireless ID, which until now had to be looked up in the log.

### Changed
- The mouse page: "Profile saved" and the other messages appear next to the Save button and go away after a few seconds, so the page does not move under the pointer.

### Fixed
- Settings reads the autostart state again when the window comes back, so a change made in Task Manager shows at once.

## 0.5.1 — 2026-10-31

Where Windows keeps the autostart entry, thanks to @Kissshot-2003, and the Discard button on the mouse page. Both tested on a real Windows 11 PC.

### Fixed
- Windows: the autostart entry goes to `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` instead of `HKLM`, where 0.4.2 and earlier could write it machine-wide: it would also start PeriHub for the other accounts on the PC with a path to this user's folder. Entries left in HKLM are moved to HKCU when PeriHub starts, or deleted when Task Manager had turned them off. When Windows does not let us delete the HKLM entry, it stays and still starts PeriHub, so Settings shows autostart as on, and turning it off says to quit PeriHub from the tray and start it once as administrator, or to disable it in Task Manager. Disabled there, Settings shows autostart as off.
- The mouse page: **Discard** goes back to the saved values at once, without waiting for the mouse, and a change made just before Discard, or before choosing or activating another profile, saving or restoring factory settings, no longer stays on the mouse as a preview.

## 0.5.0 — 2026-10-24

Wireless Logitech mice through their LIGHTSPEED receiver, read-only until someone tests them, and their battery. Not tried on a real receiver yet: reports are welcome in issue #4.

Tested on a real Windows PC, where the sign-in fix of 0.4.2 is now verified too: with PeriHub open at sign-out, it comes back in the tray without its window.

### Added
- Logitech wireless mice through their LIGHTSPEED receiver, experimental and read-only like the other mice not tested yet: the 12 wireless models already recognized with their cable, plus the G305, G603 and G604. PeriHub only talks to the mouse through the receiver, and never sends the receiver's own commands, which pair and unpair devices. A mouse that is turned off or asleep is shown as such, and PeriHub finds it again when it wakes up.
- The battery of wireless mice, on the mouse page: charge, charging and charged.
- The SteelSeries Apex 3 (full size) is recognized: PeriHub says that it is not supported yet and sends nothing to it.
- Linux: the udev rule includes the LIGHTSPEED receivers, only their HID++ interface. With the AppImage, install the rule again.

### Changed
- Windows: installed with Scoop, PeriHub leaves updates to Scoop (`scoop update perihub`). It does not look for new versions or offer to install them: its installer would add a second copy outside Scoop.

## 0.4.2 — 2026-10-21

From the test of 0.4.1 on a real Windows PC, where the update from 0.3.0, autostart with a real login, the G HUB warning and software macros started by mouse buttons and keyboard keys worked.

### Changed
- A clearer interface: larger, more readable descriptions; icons in the sidebar, which marks the page on screen; orange only for what matters (the main action, the selection, the value in use); "Unsaved changes" next to the button that saves them; a disabled Save button that no longer looks enabled; profile cards that fit; and the key map that fits the window down to its minimum size.
- Reactive lighting lights the zone of the key pressed, not of the key it sends: SteelSeries + 1 sending F24 lights the zone of 1, and two swapped keys light their own zones. A key that two keys send lights its own zone.
- The mouse page follows the mouse's buttons whenever it is visible, also without the focus. The level in use is marked in the DPI levels, and the current DPI is shown at the top right.
- Windows: the log file moved to `%LOCALAPPDATA%\io.github.hexdump.perihub\logs`. Before it was in the install folder, which then stayed behind after uninstalling. The old log is moved there.

### Fixed
- Windows: with PeriHub open at sign-out, Windows opened it again at sign-in with its window, and autostart then brought the window to the front. Now autostart's launch never shows the window, and PeriHub asks Windows to reopen it in the tray. Not verified on Windows yet.
- While another program, such as G HUB, controlled the mouse, the mouse page showed "Unsaved changes" with no profile selected. Saving was refused, so nothing was written.

## 0.4.1 — 2026-10-20

### Added
- Keys of the Apex 3 TKL can start software macros (issue #6): on the **Keys** page choose "Macro run by PeriHub" for a key, for example SteelSeries + 1, and pick a macro or create one. The key sends the macro's key from the keyboard's memory. PeriHub reads the keyboard's keys only while one of them starts a macro, acts only on the macro keys and drops every other key as it arrives. On Linux this needs the optional rule for the keyboard's keys, the same as reactive lighting, with the same warning. Tested on Linux with the real keyboard; not tried on a real Windows PC yet.
- The software macros card is also on the **Keys** page.

## 0.4.0 — 2026-10-20

This version fixes what the Windows test of 0.3.0 found. The new features were tested on Linux with the real hardware; on Windows the software macros, the G HUB warning and the new autostart entry have not been tried on a real PC yet: reports are welcome in issue #3. On Windows, PeriHub 0.3.0 finds this version when it starts and installs it from Settings; on Linux, install it by hand.

### Added
- PeriHub notices when another program puts the mouse under software control, as G HUB does when it starts and after every wake-up. If no such program is running, the mouse gets its onboard profiles back within a few seconds (with its active profile, which it can lose after a wake-up); if G HUB is running, PeriHub does not fight it and explains what is happening. On Windows it warns while G HUB is running.
- Log file (`perihub.log`) to attach to bug reports, with an "Open log folder" button in Settings. It records devices connecting, errors and updates, never key presses or the apps in focus.
- Updates on Windows are also checked once a day while PeriHub runs, and a new version shows in the tray menu.
- Software macros, run by PeriHub (issue #6): once, a number of times, or until the button is pressed again, with keys, mouse buttons, media keys, scrolling, pauses, typed text (accents included) and opening a website or a program. A mouse button starts one by sending F19, F24 or F13–F18 from the mouse's memory; PeriHub reacts only to keys that come from the mouse. On Linux they need access to /dev/uinput: PeriHub offers an optional rule, with a warning. "Stop software macros" in the tray menu stops them all.
- The mouse page follows profile and DPI changes made with the mouse's buttons.
- Macro editor: copy a macro from another button or profile (a macro already in the mouse is shared, not written twice), see how much memory it takes and how much is free in the mouse, and import or export macros as files.

### Changed
- Closing the window frees the web view that draws it: in the tray PeriHub uses a fraction of the memory it used before, and started hidden it creates no window at all.
- The keyboard lighting preview animates only while the window is in front, and static effects are drawn once: an open window in the background no longer uses CPU.
- The Windows autostart entry puts PeriHub's path in quotes, so it works with a space in the user folder name.

### Fixed
- After a wake-up with G HUB running, PeriHub kept saying the mouse used its onboard profiles, and app profiles failed to switch it with "invalid argument".
- Opening the keyboard lighting page sent the keyboard the lighting it already showed: the effect restarted with a short glitch, and a lighting set by an app profile was replaced by the normal one.

## 0.3.0 — 2026-10-19

The new features were tested on Linux with the real hardware. On Windows they have not been tried on a real PC yet: reports are welcome in issue #3.

### Added
- App profiles: while a game or an app is in focus, the keyboard shows a saved lighting and the mouse switches to one of its onboard profiles; both go back when it loses focus. Works on Windows and on Hyprland.
- Saved lightings on the keyboard lighting page, used by app profiles.
- Reactive keyboard lighting: the zone of each pressed key lights up and fades back. On Windows the keys come from Raw Input (not tested on a real PC yet). Key presses are read only while the effect is on and are never stored. It needs an optional udev rule, which PeriHub can install and remove after an explicit warning: with it, every program running as the user can read what is typed on the Apex.
- `perihub-cli focus` prints the app in focus, to find the name to use in a rule.
- German, French, Spanish, Portuguese, Swedish/Finnish, Norwegian and Danish layouts in the key remapping view, chosen automatically from the system language.
- Mouse macros stored in the mouse's memory, so they work without PeriHub on any PC. Record them from the keyboard or build them step by step with keys, mouse clicks, media keys, scrolling and pauses. A macro runs once, repeats while the button is held, or runs one part when the button is pressed and another when it is released. Macros are saved on the G502 HERO; other models show theirs read-only.
- Updates on Windows: PeriHub looks for a new version on GitHub when it starts and installs it with one click, after checking its signature. The check can be turned off in Settings, which also shows the version. Install 0.3.0 by hand: updates start from the next version.

### Changed
- Mouse backups include every sector of the memory, macros too.
- **Security:** the Linux udev rule now opens only the Apex 3 TKL's configuration interface. Before, it also opened the interfaces that report key presses, so programs running as the user could read what was typed on it. Install the new rule to apply the change.
- The experimental mouse notice points to the new mouse test report form on GitHub.

## 0.2.0 — 2026-10-18

First version tried on a real Windows PC: keyboard lighting, key remapping and mouse profiles work.

### Added
- Experimental, read-only support for 27 more Logitech G mice connected with a USB cable, from libratbag's device list. PeriHub shows their profiles, DPI, buttons and lighting but changes nothing until a model is confirmed to work.
- The Linux udev rule covers the new mice.
- Brightness for the mouse's fixed colors.

### Changed
- The mouse page, sidebar and settings show the connected model instead of assuming the G502 HERO.
- A mouse that fails to open is no longer retried every 2 seconds unless it is a tested model; plug it in again to retry.
- Mouse breathing and color cycle apply to all LED zones at once, so the DPI indicator and the logo stay in sync.

### Fixed
- The keyboard brightness slider now follows SteelSeries + F11/F12 while the lighting page is open, instead of only when the page is opened. The new level is also saved, so it is kept when the keyboard is plugged in again.
- The brightness slider moves smoothly, both when dragged and when the level changes from the keyboard.
- The key remapping view matches the real Apex 3 TKL: the bottom row ends with Alt, Win, SteelSeries and Ctrl (there is no Menu key), and there are no Print Screen, Scroll Lock and Pause keys.
- SteelSeries + F10 is labeled as the microphone mute key; SteelSeries + F9, which does nothing, shows no label.

## 0.1.0 — 2026-10-17

First release.

### SteelSeries Apex 3 TKL
- 8-zone lighting: static, gradient, breathing, color cycle, wave, built-in rainbow, off.
- Brightness 0–16, kept in sync with SteelSeries + F11/F12.
- Key remapping on the normal and SteelSeries layers, stored in the keyboard's memory.
- ANSI (US), ISO (UK) and ISO (Italian) layouts in the key remapping view.
- Reset all keys to factory settings.

### Logitech G502 HERO
- 5 onboard profiles: activate, enable/disable, restore factory settings.
- Up to 5 DPI levels (100–25600), default and sniper level, with live preview.
- Polling rate 125/250/500/1000 Hz.
- 11 buttons plus the G-Shift layer: clicks, keys with modifiers, media keys, DPI and profile functions.
- Logo and DPI indicator LEDs: off, fixed, breathing, color cycle, with live preview.

### App
- Automatic detection when devices are plugged in or unplugged.
- System tray icon, autostart on login, single instance.
- Automatic backup before every write to a device's memory.

### Known limitations
- The Windows build is produced and tested by CI but has not been tried on a real Windows PC yet.
- Not yet available: per-game profile switching, software macros, reactive lighting, GameSense.
