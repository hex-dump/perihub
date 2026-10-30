# Changelog

## Unreleased

### Fixed
- Windows: the autostart entry goes to `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` instead of `HKLM`, where 0.4.2 and earlier could write it machine-wide: it would also start PeriHub for the other accounts on the PC with a path to this user's folder. Entries left in HKLM are moved to HKCU when PeriHub starts, or deleted when Task Manager had turned them off. When Windows does not let us delete the HKLM entry, it stays and still starts PeriHub, so Settings shows autostart as on, and turning it off says to quit PeriHub from the tray and start it once as administrator.
- The mouse page: **Discard** goes back to the saved values at once, without waiting for the mouse, and a change made just before Discard, or before choosing another profile, no longer stays on the mouse as a preview.

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
