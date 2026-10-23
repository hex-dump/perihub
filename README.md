<p align="center">
  <img src="app/src-tauri/icons/128x128.png" width="96" alt="">
</p>

<h1 align="center">PeriHub</h1>

<p align="center">
  An open-source app to configure gaming peripherals on <b>Windows and Linux</b>,<br>
  without SteelSeries GG or Logitech G HUB.
</p>

<p align="center">
  <a href="https://github.com/hex-dump/perihub/releases/latest"><img src="https://img.shields.io/github/v/release/hex-dump/perihub?label=release" alt="Release"></a>
  <a href="https://github.com/hex-dump/perihub/actions/workflows/ci.yml"><img src="https://github.com/hex-dump/perihub/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="LICENSE"><img src="https://img.shields.io/github/license/hex-dump/perihub" alt="MIT license"></a>
</p>

![Keyboard lighting](docs/screenshots/keyboard-lighting.png)

PeriHub currently supports the **SteelSeries Apex 3 TKL** keyboard and the **Logitech G502 HERO** mouse. The goal is to grow into a single, lightweight, vendor-neutral app for many more devices: see [Hardware roadmap](#hardware-roadmap).

## Contents

- [Features](#features)
- [Supported hardware](#supported-hardware)
- [Hardware roadmap](#hardware-roadmap)
- [Download and install](#download-and-install)
- [Usage](#usage)
- [Safety](#safety)
- [Performance](#performance)
- [FAQ](#faq)
- [Known limitations](#known-limitations)
- [Building from source](#building-from-source)
- [Architecture](#architecture)
- [Contributing](#contributing)
- [Support the project](#support-the-project)
- [Credits and license](#credits-and-license)

## Features

### SteelSeries Apex 3 TKL

| Feature | Details | Stored in |
|---|---|---|
| 8-zone lighting | Static (one color per zone), gradient, breathing, color cycle, wave, reactive (the zone of each pressed key lights up), built-in rainbow, off | PC (PeriHub reapplies it whenever the keyboard is connected) |
| Brightness | 0–100% in 17 steps, kept in sync with SteelSeries + F11/F12 | PC |
| Key remapping | Any key, on both the normal layer and the "SteelSeries key held" layer, with the US, UK, Italian, German, French, Spanish, Portuguese, Swedish/Finnish, Norwegian and Danish layouts | **Keyboard memory**: works without PeriHub |
| Software macros | A key, for example SteelSeries + 1, can start a macro run by PeriHub (see the G502 HERO below). On Linux this needs the optional rule for reactive lighting | The key in keyboard memory, the macro on the PC: PeriHub must be running |

### Logitech G502 HERO

| Feature | Details | Stored in |
|---|---|---|
| Profiles | 5 onboard profiles: activate, enable/disable, restore factory settings | **Mouse memory** |
| DPI | Up to 5 levels from 100 to 25600 (step 50), default and sniper level, live preview | Mouse memory |
| Polling rate | 125, 250, 500, 1000 Hz | Mouse memory |
| Buttons | 11 buttons plus the G-Shift layer: mouse clicks, keys with Ctrl/Shift/Alt/Win/Alt Gr, media keys, DPI up/down/cycle/sniper, profile switching | Mouse memory |
| Lighting | Logo and DPI indicator: off, fixed color with brightness, breathing, color cycle (breathing and color cycle stay in sync on both zones), with live preview | Mouse memory |
| Macros | Recorded from the keyboard or built step by step: keys, mouse clicks, media keys, scrolling, pauses. Run once, repeat while the button is held, or one part on press and another on release | **Mouse memory**: works without PeriHub, on any PC |
| Software macros | Run by PeriHub, for what the mouse cannot do alone: repeat a number of times or until you press the button again, type text (accents included), open a website or a program. Up to 8, started by mouse buttons or keyboard keys | PC: PeriHub must be running, also in the tray |

### App profiles

While a game or an app is in focus, PeriHub can show one of your saved keyboard lightings and switch the mouse to one of its onboard profiles. When you switch to an app without a rule, both go back to how they were. It works on Windows and, on Linux, on Hyprland: other Linux desktops do not tell apps which window is in focus.

PeriHub only reads the app's identifier (the executable name on Windows, the window class on Linux), never window titles or contents, and keeps nothing but your rules.

### Other Logitech G mice (experimental)

27 more Logitech mice are recognized when connected with a USB cable (see [Supported hardware](#supported-hardware)). They have not been tested with PeriHub yet, so it only reads them: you can see their profiles, DPI levels, buttons and lighting, but nothing can be changed until someone confirms that the model works.

From version 0.5.0, not released yet, the 15 wireless models are also recognized through their LIGHTSPEED receiver, read-only too: three of them, the G305, G603 and G604, have no data cable. PeriHub also shows their battery, and when the mouse is turned off or asleep it says so and finds it again once it wakes up.

<table>
  <tr>
    <td><img src="docs/screenshots/keyboard-keys.png" alt="Key remapping"></td>
    <td><img src="docs/screenshots/mouse-dpi.png" alt="Mouse DPI and polling rate"></td>
  </tr>
  <tr>
    <td><img src="docs/screenshots/mouse-buttons.png" alt="Mouse buttons"></td>
    <td><img src="docs/screenshots/mouse-lighting.png" alt="Mouse lighting"></td>
  </tr>
</table>

## Supported hardware

| Device | USB ID | Status |
|---|---|---|
| SteelSeries Apex 3 TKL | `1038:1622` (firmware 3.0.4) | ✅ Supported, tested on real hardware |
| Logitech G502 HERO | `046d:c08b` | ✅ Supported, tested on real hardware |
| Other Logitech G mice, with a USB cable (from 0.5.0 also with a LIGHTSPEED receiver) | listed below | 🧪 Experimental, read-only |
| SteelSeries Apex 3 | `1038:161a` | From 0.5.0 recognized, not supported yet: PeriHub sends nothing to it ([issue #9](https://github.com/hex-dump/perihub/issues/9)) |

<details>
<summary>Experimental Logitech mice</summary>

The list comes from [libratbag](https://github.com/libratbag/libratbag). Wireless models are recognized with their USB cable. From version 0.5.0, not released yet, they are also recognized through a LIGHTSPEED receiver (`046d:c539`, `c53a`, `c53d`, `c53f`, `c541`, `c543`, `c545`, `c547` or `c54d`), where they report the wireless ID in the last column. The receiver support has not been tried on real hardware yet: reports are welcome in [issue #4](https://github.com/hex-dump/perihub/issues/4).

| Mouse | USB ID | Through a receiver |
|---|---|---|
| Logitech G502 Proteus Core | `046d:c07d` | |
| Logitech G502 Proteus Spectrum | `046d:c332` | |
| Logitech G502 HERO Wireless | `046d:c08d` | `407f` |
| Logitech G502 X | `046d:c099` | |
| Logitech G502 X Wireless | `046d:c098` | `409f` |
| Logitech G502 X PLUS | `046d:c095` | `4099` |
| Logitech G Pro | `046d:c085` | |
| Logitech G Pro | `046d:c08c` | |
| Logitech G Pro Wireless | `046d:c088` | `4079` |
| Logitech G Pro X Superlight | `046d:c094` | `4093` |
| Logitech G102/G103/G203 | `046d:c084` | |
| Logitech G102/G103/G203 | `046d:c092` | |
| Logitech G102/G103/G203 | `046d:c09d` | |
| Logitech G302 | `046d:c07f` | |
| Logitech G303 | `046d:c080` | |
| Logitech G303 Shroud Edition | `046d:c097` | |
| Logitech G402 | `046d:c07e` | |
| Logitech G403 | `046d:c083` | |
| Logitech G403 HERO | `046d:c08f` | |
| Logitech G403 Wireless | `046d:c082` | `405d` |
| Logitech G703 | `046d:c087` | `4070` |
| Logitech G703 HERO | `046d:c090` | `4086` |
| Logitech G705 | `046d:c096` | `409d` |
| Logitech G900 | `046d:c081` | `4053` |
| Logitech G903 | `046d:c086` | `4067` |
| Logitech G903 HERO | `046d:c091` | `4087` |
| Logitech MX518 | `046d:c08e` | |
| Logitech G305 (from 0.5.0) | wireless only | `4074` |
| Logitech G603 (from 0.5.0) | wireless only | `406c` |
| Logitech G604 (from 0.5.0) | wireless only | `4085` |

If you own one of these, please fill in a [mouse test report](https://github.com/hex-dump/perihub/issues/new?template=mouse_test.yml) with what PeriHub shows: it takes a few minutes, and a model becomes fully supported once someone confirms it works.

</details>

PeriHub only talks to the devices listed here. It never sends commands to anything it does not recognize. From 0.5.0, behind a LIGHTSPEED receiver it first asks each device which model it is, with read commands only.

## Hardware roadmap

PeriHub is meant to support much more than these two devices. The plan is to add hardware over time, starting with devices that share protocols PeriHub already speaks:

- **Logitech G mice using HID++ 2.0.** 27 more models are already recognized in read-only mode. Version 0.5.0 adds LIGHTSPEED receivers, read-only too. Next: enable editing as owners confirm them.
- **Other SteelSeries keyboards**, such as the rest of the Apex range.
- **SteelSeries mice.**
- **Headsets and other peripherals**, where the hardware exposes useful settings.
- **More languages** for the UI, and more layouts for the key remapping view.

There is no fixed timeline: devices are added once their protocol can be verified on real hardware. If you own a device you would like to see supported, open a [device request](https://github.com/hex-dump/perihub/issues/new?template=device_request.yml): read-only dumps and testing help are the fastest way to get a device added.

## Download and install

Get the file for your system from the [releases page](https://github.com/hex-dump/perihub/releases/latest).

### Windows

1. Download `PeriHub_…_x64-setup.exe` (or the `.msi`) and run it.
2. The installer is not code-signed. If SmartScreen blocks it, click **More info → Run anyway**.
3. **Quit SteelSeries GG and Logitech G HUB** and turn off their autostart: two programs talking to the same device get in each other's way.

No drivers are needed: PeriHub uses the standard Windows HID interfaces.

From version 0.3.0, PeriHub tells you when a new version is out and installs it with one click (**Settings → Updates**). From 0.4.0 it also checks once a day while it runs in the tray, and shows the new version in the tray menu.

**With Scoop**: [@Kissshot-2003](https://github.com/Kissshot-2003) maintains a [bucket for PeriHub](https://github.com/Kissshot-2003/perihub-scoop).

```pwsh
scoop bucket add perihub-scoop https://github.com/Kissshot-2003/perihub-scoop
scoop install perihub-scoop/perihub
```

Update it with `scoop update perihub`, not from **Settings → Updates**: PeriHub's installer would add a second copy outside Scoop. From 0.5.0, PeriHub recognizes a Scoop install and leaves updates to Scoop.

### Linux

**AppImage** (any distribution, including Arch and CachyOS):

```sh
chmod +x PeriHub_*_amd64.AppImage
./PeriHub_*_amd64.AppImage
```

**Arch, CachyOS and other Arch-based distributions**: the `perihub-bin` package will be published on the AUR soon. Until then, you can build it from the repository; it installs the udev rule too:

```sh
git clone https://github.com/hex-dump/perihub.git
cd perihub/packaging/aur/perihub-bin
makepkg -si
```

**Debian/Ubuntu** (`.deb`) and **Fedora/openSUSE** (`.rpm`): install the package with your distribution's package manager.

#### USB permissions

The `.deb` and `.rpm` packages install a udev rule automatically. With the AppImage, if PeriHub shows "permission denied", install the rule manually:

```sh
sudo curl -o /etc/udev/rules.d/70-perihub.rules \
  https://raw.githubusercontent.com/hex-dump/perihub/main/packaging/linux/70-perihub.rules
sudo udevadm control --reload && sudo udevadm trigger
```

The rule grants access only to the HID interfaces of the supported devices, and only to the user with the active session. On the Apex 3 TKL it opens only the configuration interface: the interfaces that report what you type stay closed. On a LIGHTSPEED receiver it opens only the HID++ interface, which carries settings: mouse movements and typing arrive on the receiver's other interfaces, which stay closed. If you already use OpenRGB with its udev rules, you probably don't need it.

From version 0.5.0 the rule includes the LIGHTSPEED receivers: if you installed it by hand for the AppImage, install it again with the commands above. The packages update it on their own.

<a id="optional-reactive-lighting"></a>

#### Optional: reactive lighting, and keyboard keys that start software macros

Reactive lighting needs a second rule, [`71-perihub-keys.rules`](packaging/linux/71-perihub-keys.rules), because it reads key presses. So does a key of the Apex 3 TKL that starts a software macro: PeriHub has to read the keyboard to see the macro's key. PeriHub offers to install the rule from **Lighting → Reactive** and from the **Keys** page, asking for your password, and can remove it from the same places.

> [!WARNING]
> Linux cannot give this permission to PeriHub alone. With the rule installed, **every program running under your user account can read everything you type on the Apex 3 TKL**, including passwords and private messages, in any app and even while PeriHub is closed. A malicious or compromised program could use it as a keylogger. It turns off, for this keyboard, the protection Wayland normally gives. Install it only if you want reactive lighting or keyboard keys that start software macros, and trust all the software you run.

PeriHub itself reads key presses only while the reactive effect is on, or while a key of the keyboard starts a software macro. It keeps only which lighting zone to light or which macro to start: it never stores, logs or sends what you type.

#### Optional: software macros

Software macros press keys and click through a virtual keyboard and mouse, made with `/dev/uinput`. Many gaming systems already let your user open it, for game controllers: PeriHub says so on the mouse's **Buttons** page and on the keyboard's **Keys** page. Otherwise it offers to install [`72-perihub-uinput.rules`](packaging/linux/72-perihub-uinput.rules) from the same pages, asking for your password, and can remove it there. Keys of the Apex 3 TKL that start a macro also need [the rule for its keys](#optional-reactive-lighting-and-keyboard-keys-that-start-software-macros).

> [!WARNING]
> With this rule, **every program running under your user account can simulate a keyboard and a mouse**, and use them to type and click anything as if it were you: for example, type commands into a terminal. Without it a program needs administrator rights for that. Install it only if you want software macros and trust all the software you run.

## Usage

- **Keyboard → Lighting**: pick an effect. Changes apply immediately and are remembered.
- **Keyboard → Keys**: choose your layout (ANSI US, ISO UK, ISO Italian), click a key, pick its new function and press **Save to keyboard**. Switch to **With SteelSeries key** to change what keys do while the SteelSeries key is held.
- **Mouse**: pick a profile at the top, then edit DPI, buttons and lighting. Nothing is written to the mouse until you press **Save to mouse**. DPI and LED changes are previewed live on the mouse before saving; leaving the page restores the saved settings.
- **G-Shift**: assign "G-Shift" to a button. While you hold it, the other buttons use the functions from the G-Shift tab.

### Running in the background

Closing the window does **not** quit PeriHub. It keeps running in the tray, without the window and with much less memory, to animate the keyboard lighting and to reapply it when the keyboard is plugged back in or the PC wakes up. Reopen it from the tray icon or by launching it again. To quit for real: **Quit** in the tray menu or **Settings → Quit PeriHub**.

On **Windows 11** new tray icons start among the hidden ones (the **^** arrow). To keep PeriHub in sight: **Settings → Personalization → Taskbar → Other system tray icons → PeriHub**.

**Autostart** can be enabled in Settings. On Hyprland, XDG autostart entries only run when `xdg-desktop-autostart.target` is active (uwsm or systemd). Otherwise, add this to your configuration:

```
exec-once = /path/to/perihub --hidden
```

To see the tray icon on Hyprland or Sway you need a bar with a tray module, such as Waybar or the Noctalia bar.

## Safety

PeriHub does not modify firmware. It uses the same configuration commands as the official apps, with extra safeguards:

- **Restricted commands.** The code can only send commands that have been verified.
  - The command that reboots the Apex into its bootloader cannot even be expressed.
  - On Logitech mice, only HID++ features `0x0000`, `0x0001`, `0x0003`, `0x0005`, `0x2201`, `0x8060`, `0x8070` and `0x8100` are used. From 0.5.0 also `0x1000`, `0x1001` and `0x1004`, only to read the battery.
  - From 0.5.0, through a LIGHTSPEED receiver PeriHub only talks to the mouse, with the same rules. It never sends the receiver's own commands, which pair and unpair devices.
- **Verified writes.**
  - Mouse: profiles are edited on top of the original sector bytes, so fields PeriHub does not manage stay intact. Every written sector is read back and compared byte by byte.
  - Keyboard: the key mapping is written to RAM first and only saved to memory if the read-back matches; otherwise it is rolled back.
- **Untested mice are read-only.** For a mouse that has not been verified, the core only allows HID++ functions that read state, and it does not even take the mouse out of software mode.
- **Factory profiles are read-only.** The mouse's ROM profiles are never written and are used for restoring.
- **No data leaves your PC.** PeriHub works offline. The only connection is on Windows: when it starts and then once a day it asks GitHub for the latest release (you can turn this off in **Settings → Updates**). An update is downloaded only when you click **Update and restart**, and installed only if its signature matches PeriHub's key.
- **Software macros start only from the devices.** A mouse button or a keyboard key starts one by sending an unused key from the device's memory (F19, F24, then F13–F18; never F20–F23, which mute the microphone or switch the touchpad on Linux). PeriHub reacts only to those keys, and only when they come from the mouse or from the Apex 3 TKL. On Linux it reads the mouse's own keyboard reports, which carry only what the mouse sends, never what you type; it reads the keyboard only while one of its keys starts a macro, and drops every other key as it arrives. On Windows it uses Raw Input, which gives an app the keys of every keyboard: while a software macro is saved, PeriHub drops each key that does not come from the mouse, or from the keyboard while one of its keys starts a macro, as soon as it arrives. Keys simulated by programs, PeriHub's own included, cannot start a macro. Every key a macro holds down is released when it stops. A macro opens only a website (http or https) or a file or program given by its full path, never through a command line.
- **Key presses stay on your PC.** Two features read key presses: reactive lighting, only while it is on and only from the Apex 3 TKL, and software macros, from the mouse and, only while one of its keys starts a macro, from the Apex 3 TKL. On Windows both use Raw Input, which any app can use there without a permission and which delivers the keys of all keyboards: PeriHub drops those of other devices right away. Reactive lighting keeps nothing but the time of the last press in each lighting zone, and software macros only check whether a key starts one: which keys you press is never stored, logged or sent anywhere. On Linux, the keyboard's interfaces that report key presses are closed by default and opened only if you install the optional rule for its keys (see [the warning](#optional-reactive-lighting-and-keyboard-keys-that-start-software-macros)).
- **Backups.** Before every write to a device's memory, PeriHub saves the previous state in
  - Linux: `~/.config/perihub/backups/`
  - Windows: `%APPDATA%\perihub\backups\`

To report a security issue, see [SECURITY.md](SECURITY.md).

## Performance

**Light on Windows.** PeriHub is a small app written in Rust. It draws its window with WebView2, the web engine that comes with Windows 11 and up-to-date Windows 10 (the installer adds it if it is missing), instead of shipping a browser of its own.

| PeriHub 0.4.0 on Windows | |
|---|---|
| Download | **3.8 MB** installer |
| On disk | **16 MB** |
| In the system tray | **2 MB** of memory, **0.05%** of one CPU core |
| Window open | **42 MB** of memory, **0.3%** of one CPU core |
| Background services | none |
| Account, ads, telemetry | none |

What this means for you:

- **Light in the background.** In the tray PeriHub is a single small process. The web engine that draws the window starts when you open it and closes with it. (0.3.0 kept it running: 49 MB and about 2% of one core in the tray.) An animated lighting effect, which PeriHub draws frame by frame, takes about 0.6% of one core: measured on a real PC with both devices connected and the wave effect.
- **Quit it and your setup stays.** Key mapping, mouse profiles, DPI, buttons and macros are stored in the devices, so they keep working with PeriHub closed, and nothing of PeriHub keeps running. PeriHub has to stay open only for the keyboard lighting (the Apex does not store it, except the built-in rainbow) and for app profiles.
- **One app instead of two** if you use a SteelSeries keyboard and a Logitech mouse.
- **Updates in one click**, signed, and only when you choose.
- **Also on Linux**, where the official apps are not available: Logitech and SteelSeries offer [G HUB](https://www.logitechg.com/en-us/innovation/g-hub.html) and [GG](https://steelseries.com/gg) for Windows and macOS (checked in October 2026).

<details>
<summary>How these numbers were measured</summary>

On 20 October 2026 the [Measure on Windows](.github/workflows/measure-windows.yml) workflow built PeriHub 0.4.0 from its source code and installed it on a fresh Windows Server 2025 virtual machine on GitHub Actions (Intel Xeon Platinum 8573C, 4 logical processors, WebView2 154), with no devices connected. Memory is the private working set, which is what Task Manager shows in the Memory column, of PeriHub and, with the window open, the 6 WebView2 processes it starts. CPU is their processor time over 2 minutes. PeriHub 0.3.0 was measured the same way on 19 October 2026, installed from its release on a machine with an AMD EPYC 7763. On your PC the numbers will differ, for example with devices connected and an animated lighting effect, which PeriHub draws frame by frame. On 21 October 2026 PeriHub 0.4.1 was measured on a real PC (Windows 11, 16 logical processors) with both devices connected and the wave effect: in the tray it used 0.57–0.68% of one core over 60 seconds, almost all of it to animate the wave. The workflow is in the repository, so the measurement can be repeated, also on a fork.

</details>

**How does it compare with G HUB or SteelSeries GG?** We do not publish numbers for other apps: we have not measured them on the same machine, and their use of resources changes with the version, the devices and the features in use. They also support many more devices and features than PeriHub. To compare on your own PC, run one app at a time and look at Task Manager → Processes, where each app's processes are grouped under its name. If you do, share your numbers and the versions in [Discussions](https://github.com/hex-dump/perihub/discussions).

G HUB is a trademark of Logitech; SteelSeries GG is a trademark of SteelSeries. PeriHub is not affiliated with either company.

## FAQ

**Can I keep GG or G HUB running?**
No, quit them and turn off their autostart. On Windows, G HUB takes control of the mouse when it starts and after every wake-up, so the mouse stops using its onboard profiles; PeriHub shows a warning while G HUB is running. When G HUB is closed, PeriHub gives the mouse back its profiles within a few seconds.

**Does this void my warranty?**
PeriHub does not touch the firmware. It only writes settings (profiles, key mapping) that the official apps write too.

**Why does the keyboard go back to the rainbow when PeriHub is closed?**
The Apex 3 TKL does not store colors, and it behaves the same way with GG. PeriHub reapplies them as soon as it detects the keyboard. "Built-in rainbow" is the only effect generated by the keyboard itself.

**Something does not work: what should I attach to a bug report?**
The log file: **Settings → Problems → Open log folder** (`~/.local/state/perihub/perihub.log` on Linux, `%LOCALAPPDATA%\io.github.hexdump.perihub\logs\perihub.log` on Windows). It records devices connecting, errors and updates, never your key presses or the apps you use, and it stays under 1 MB.

**How do I go back to factory settings?**
Keyboard: **Keys → Reset all to factory**. Mouse: **Factory settings** on each profile. Backups are kept as well.

**Is the UI available in other languages?**
The UI is in English. More languages are on the roadmap.

## Known limitations

- **Windows**: tested on a real Windows 11 PC: keyboard lighting, reactive lighting, key remapping, mouse profiles and their macros, app profiles, tray icon, autostart with a real login, the update from 0.3.0 to 0.4.1, the G HUB warning, software macros started by mouse buttons and by keyboard keys, and the log file. Creating software macros with the editor and from the Keys page has been tried on Linux only. Reports are welcome in [issue #3](https://github.com/hex-dump/perihub/issues/3).
- **Logitech wireless mice** work only over their USB cable. From 0.5.0, not released yet, also through their LIGHTSPEED receiver: read-only, and not tried on real hardware yet, so reports are welcome in [issue #4](https://github.com/hex-dump/perihub/issues/4). A button of a mouse behind a receiver does not start software macros.
- **App profiles** detect the app in focus on Windows and on Hyprland only. They switch lighting and mouse profiles, not key mappings, because changing those writes the keyboard's memory.
- **Not yet available**: GameSense compatibility.
- **Software macros** type text through the keyboard layout on Linux, so characters the layout cannot type (with Shift, AltGr or a dead key) are skipped. On Windows many games ignore typed text, but not keys.
- **Out of scope**: SteelSeries Sonar and Moments, G HUB Lua scripts.

## Building from source

You need stable [Rust](https://rustup.rs) (≥ 1.88) and [Node.js](https://nodejs.org) ≥ 20.19. On Linux you also need the development libraries for WebKitGTK 4.1, libayatana-appindicator, librsvg and libudev:

```sh
# Debian/Ubuntu
sudo apt install libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libudev-dev
# Arch/CachyOS
sudo pacman -S webkit2gtk-4.1 libayatana-appindicator librsvg
```

```sh
git clone https://github.com/hex-dump/perihub.git
cd perihub/app
npm install
npm run tauri dev                     # development, with hot reload
npm run tauri build -- --no-bundle    # standalone binary in target/release/perihub
npm run tauri build                   # installers for the current platform
```

Don't build the app with plain `cargo build`: that binary loads the UI from the development server (`localhost:1420`).

Tests and checks:

```sh
cargo fmt --all                 # formatting (CI checks it)
cargo test --workspace          # includes tests on sectors read from a real G502 HERO
cargo clippy --workspace --all-targets
cd app && npm run check         # frontend type check
```

### Command-line tool

Useful for diagnostics and scripting. Don't run it while the app is open, since both would compete for the same devices.

```sh
cargo run -p perihub-cli -- status
cargo run -p perihub-cli -- mouse state
cargo run -p perihub-cli -- keyboard keymap
cargo run -p perihub-cli -- keyboard light '{"type":"wave","period_ms":3000,"reverse":false}' 10
```

## Architecture

```
crates/perihub-core     Rust library: protocols, effects, device detection, configuration
  src/apex/             SteelSeries Apex 3 TKL: RGB (output reports) and key mapping (feature reports)
  src/hidpp/            Logitech HID++ 2.0, LIGHTSPEED receivers, the onboard profile format (CRC-CCITT) and the list of mice
  src/effects.rs        lighting effects rendered on the PC
  src/manager.rs        device hotplug
  src/worker.rs         one thread per device, so requests never interleave
  tests/fixtures/       real G502 HERO sectors used by the tests
crates/perihub-cli      command-line tool
app/                    UI: Tauri 2 + Svelte 5
packaging/linux         udev rule
```

- Device communication goes through [hidapi](https://github.com/libusb/hidapi). On Linux the hidraw backend is used, so the keyboard and mouse keep working normally while PeriHub is open.
- On Windows the mouse's short and long HID++ collections are separate devices; PeriHub uses the long one (report `0x11`). Behind a LIGHTSPEED receiver it also uses the short one, where the receiver answers for a mouse that is asleep.
- Each device family is a separate module in `perihub-core`, which is where new hardware will be added.

## Contributing

Bug reports, device requests and pull requests are welcome.

- **Questions and ideas**: use [Discussions](https://github.com/hex-dump/perihub/discussions).
- **Bugs**: include your OS, the device, and what the app showed.
- **New devices**: open a [device request](https://github.com/hex-dump/perihub/issues/new?template=device_request.yml) with the model and USB ID.
- **Experimental Logitech mice**: fill in a [mouse test report](https://github.com/hex-dump/perihub/issues/new?template=mouse_test.yml).
- **Code**: run `cargo fmt`, `cargo test`, `cargo clippy` and `npm run check` before opening a pull request.

## Support the project

PeriHub is free and open source. If it spared you from installing G HUB or SteelSeries GG and you'd like to say thanks, you can [buy me a coffee](https://buymeacoffee.com/hexdump1).

Donations are entirely optional and don't unlock anything. They support the development of PeriHub, including buying new devices to add. You can help just as much without money: test PeriHub on your hardware, report bugs, or open a device request with read-only dumps.

## Credits and license

The protocols were reconstructed thanks to the work of:

- [OpenRGB](https://gitlab.com/CalcProgrammer1/OpenRGB): documentation of the 8-zone Apex lighting protocol. No code was copied.
- [libratbag](https://github.com/libratbag/libratbag): Logitech HID++ 2.0, onboard profiles and the list of Logitech mice (MIT).
- [gorkemgun/apex-3-tkl](https://github.com/gorkemgun/apex-3-tkl): Apex 3 TKL key mapping (MIT).

PeriHub is released under the [MIT license](LICENSE). It is not affiliated with SteelSeries or Logitech; product names belong to their respective owners.
