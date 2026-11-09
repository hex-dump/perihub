# Changelog

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
