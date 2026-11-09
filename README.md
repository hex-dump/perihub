<p align="center">
  <img src="app/src-tauri/icons/128x128.png" width="96" alt="">
</p>

<h1 align="center">PeriHub</h1>

<p align="center">
  <b>PeriHub for your peripherals</b>: one open-source app for your gaming keyboard and mouse<br>
  on <b>Windows and Linux</b>, without SteelSeries GG, Logitech G HUB or Corsair iCUE.
</p>

<p align="center">
  <a href="https://github.com/hex-dump/perihub/releases/latest"><img src="https://img.shields.io/github/v/release/hex-dump/perihub?label=release" alt="Release"></a>
  <a href="https://github.com/hex-dump/perihub/actions/workflows/ci.yml"><img src="https://github.com/hex-dump/perihub/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="LICENSE"><img src="https://img.shields.io/github/license/hex-dump/perihub" alt="MIT license"></a>
</p>

<p align="center">
  <a href="https://github.com/hex-dump/perihub/releases/latest"><b>Download</b></a> ·
  <a href="https://hex-dump.github.io/perihub/">Website</a> ·
  <a href="docs/devices.md">Devices</a> ·
  <a href="https://github.com/hex-dump/perihub/discussions">Discussions</a>
</p>

![Keyboard lighting](docs/screenshots/keyboard-lighting.png)

- **Free and open source**, no account, no ads, no telemetry.
- **Small**: a 4 MB installer, about 3 MB of memory while it waits in the tray.
- **Settings stay in your devices**: key mapping, mouse profiles, DPI and macros keep working with PeriHub closed, on any PC.

## Contents

- [Devices](#devices)
- [What PeriHub can do](#what-perihub-can-do)
- [Experimental features](#experimental-features)
- [Download and install](#download-and-install)
- [Using PeriHub](#using-perihub)
- [Safety](#safety)
- [Performance](#performance)
- [FAQ](#faq)
- [Known limitations](#known-limitations)
- [Help and contributing](#help-and-contributing)
- [Credits and license](#credits-and-license)

## Devices

| Device | Support |
|---|---|
| **SteelSeries Apex 3 TKL** keyboard | ✅ Fully supported, tested on real hardware |
| **Logitech G502 HERO** mouse | ✅ Fully supported, tested on real hardware |
| 29 more **Logitech G mice** (G502 X, G Pro, G403, G703, G903, G305…) | 🧪 Experimental: read, and editable at your own risk with the USB cable |
| 26 **Corsair mice** and 23 **Corsair keyboards** (KATAR, M65, M75, SCIMITAR, DARK CORE, K55, K70, K100…) | 🧪 Experimental: read, and one colour of lighting for the session, at your own risk |
| **Logitech G300** mouse | 🧪 Experimental: read, and editable at your own risk (from the next version; read-only in 0.6.1) |

The full list, with USB IDs, is in **[docs/devices.md](docs/devices.md)**. A device becomes fully supported once someone with it sends a test report: see [Help test your device](#help-test-your-device).

Not on the list? Open a [device request](https://github.com/hex-dump/perihub/issues/new?template=device_request.yml): any brand can be asked for.

## What PeriHub can do

### SteelSeries Apex 3 TKL

| Feature | Details | Saved in |
|---|---|---|
| Lighting | 8 zones: static, gradient, breathing, color cycle, wave, reactive (the zone of each key you press lights up), built-in rainbow, off | PC: PeriHub reapplies it when the keyboard is connected |
| Brightness | 0–100% in 17 steps, in sync with SteelSeries + F11/F12 | PC |
| Key remapping | Any key, on the normal layer and on the "SteelSeries key held" layer, with 10 keyboard layouts | **Keyboard**: works without PeriHub |
| Software macros | A key, such as SteelSeries + 1, starts a macro run by PeriHub | The key in the keyboard, the macro on the PC |

### Logitech G502 HERO

| Feature | Details | Saved in |
|---|---|---|
| Profiles | 5 onboard profiles: activate, enable or disable, restore factory settings | **Mouse** |
| DPI | Up to 5 levels from 100 to 25600, default and sniper level, live preview | Mouse |
| Polling rate | 125, 250, 500, 1000 Hz | Mouse |
| Buttons | 11 buttons plus the G-Shift layer: clicks, keys with modifiers, media keys, DPI and profile functions | Mouse |
| Lighting | Logo and DPI indicator: off, fixed color, breathing, color cycle, with live preview | Mouse |
| Macros | Recorded from the keyboard or built step by step: keys, clicks, media keys, scrolling, pauses | **Mouse**: works without PeriHub, on any PC |
| Software macros | What the mouse cannot do alone: repeat, type text, open a website or a program. Up to 8 | PC: PeriHub must be running |

### Everything else

- **App profiles**: while a game or an app is in focus, PeriHub can switch the keyboard lighting and the mouse profile, and switch back afterwards. On Windows and on Hyprland. PeriHub only reads the app's identifier (the executable name or window class), never window titles or contents.
- **Backups**: before every write to a device, PeriHub saves its memory, and **Backups…** puts it back.
- **Updates in one click** on Windows, signed, and only when you choose.

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

## Experimental features

Some devices have not been tested yet. PeriHub reads them and changes nothing, unless you turn on an experimental feature on the device's page.

> [!CAUTION]
> **No warranty.** Experimental features have never been tried on your device. PeriHub is provided "as is", without any warranty, as its [license](LICENSE) says. If you turn one on, you use it at your own risk: the author of PeriHub is not responsible for any damage, malfunction or loss. PeriHub asks you to confirm this before each one is turned on.

| | Logitech mice: **Edit anyway…** | Logitech G300: **Edit anyway…** (from the next version) | Corsair devices: **Try experimental lighting…** |
|---|---|---|---|
| What you can change | Profiles: DPI, polling rate, buttons, lighting, macros | Profiles: light colour, report rate, DPI levels and shift, buttons | One colour on every LED or key, for this session |
| When it is offered | USB cable, and a memory laid out like the G502 HERO's | Always | Always, except while iCUE, OpenRGB, ckb-next or OpenLinkHub runs |
| What stays off | Enabling or disabling profiles, editing through a receiver | The left and right buttons | Everything else: DPI, polling rate, buttons, the device's memory |
| Safety net | Only values the mouse accepts; a backup before every save; every write read back | Only values Logitech Gaming Software offers; only the edited fields change; a backup of the three profiles before every save; every write read back | Only the commands lighting needs; nothing is saved in the device |
| If something looks wrong | Each save is undone after 15 seconds unless you keep it, also after PeriHub closed or the mouse was unplugged | The same | The device gets its own settings back after 15 seconds unless you keep it, when you quit PeriHub, and as soon as anything fails |
| Turn it off | On the mouse page, or **Turn off experimental editing** in the tray menu | On the G300 page, or the tray menu | On the device page, or **Turn off experimental editing** in the tray menu |

### Help test your device

If you own a device marked 🧪 or 👁:

1. Install PeriHub and quit the official app (G HUB, Logitech Gaming Software, iCUE).
2. Open the device's page in PeriHub and press **Open test form**. It opens a report on GitHub with what PeriHub read already filled in: no key presses, apps or log lines.
3. Check that what PeriHub shows matches your device, and send it. You need a free GitHub account.

Once someone confirms a model, it becomes fully supported for everyone.

## Download and install

Get the file for your system from the [releases page](https://github.com/hex-dump/perihub/releases/latest).

### Windows

1. Download `PeriHub_…_x64-setup.exe` and run it.
2. The installer is not code-signed yet ([code signing policy](CODE_SIGNING.md)). If SmartScreen blocks it, click **More info → Run anyway**.
3. **Quit SteelSeries GG, Logitech G HUB and Corsair iCUE**, and turn off their autostart: two programs talking to the same device get in each other's way.

No drivers are needed. PeriHub tells you when a new version is out, in the tray menu and in **Settings → Updates**.

**With Scoop**: [@Kissshot-2003](https://github.com/Kissshot-2003) maintains a [bucket for PeriHub](https://github.com/Kissshot-2003/perihub-scoop). Update it with `scoop update perihub`.

```pwsh
scoop bucket add perihub-scoop https://github.com/Kissshot-2003/perihub-scoop
scoop install perihub-scoop/perihub
```

### Linux

| Distribution | File |
|---|---|
| Debian, Ubuntu | `PeriHub_…_amd64.deb` |
| Fedora, openSUSE | `PeriHub-…-1.x86_64.rpm` |
| Arch, CachyOS | the `perihub-bin` package, built from this repository (below) |
| Any other | `PeriHub_…_amd64.AppImage`: `chmod +x` it and run it |

On Arch-based distributions, until `perihub-bin` is on the AUR:

```sh
git clone https://github.com/hex-dump/perihub.git
cd perihub/packaging/aur/perihub-bin
makepkg -si
```

#### USB permissions

The `.deb`, `.rpm` and `perihub-bin` packages install a udev rule that lets PeriHub open the devices. With the **AppImage**, if PeriHub shows "permission denied", install it by hand, and again after an update that adds devices:

```sh
sudo curl -o /etc/udev/rules.d/70-perihub.rules \
  https://raw.githubusercontent.com/hex-dump/perihub/main/packaging/linux/70-perihub.rules
sudo udevadm control --reload && sudo udevadm trigger
```

The rule opens only the interfaces that carry settings, and only for the user at the screen. The ones that report what you type and how you move the mouse stay closed.

<details>
<summary><b>Optional rules</b>: reactive lighting and software macros</summary>

<a id="optional-reactive-lighting"></a>

#### Optional: reactive lighting, and keyboard keys that start software macros

Reactive lighting reads key presses, and so does a key of the Apex 3 TKL that starts a software macro. This needs a second rule, [`71-perihub-keys.rules`](packaging/linux/71-perihub-keys.rules). PeriHub offers to install it from **Lighting → Reactive** and from the **Keys** page, asking for your password, and can remove it from there.

> [!WARNING]
> Linux cannot give this permission to PeriHub alone. With the rule installed, **every program running under your user account can read everything you type on the Apex 3 TKL**, passwords included, even while PeriHub is closed. Install it only if you want these features and trust all the software you run.

PeriHub itself reads key presses only while reactive lighting is on or a keyboard key starts a macro, and never stores, logs or sends what you type.

#### Optional: software macros

Software macros type and click through a virtual keyboard and mouse (`/dev/uinput`). Many gaming systems already allow it, and PeriHub tells you so on the **Buttons** and **Keys** pages. Otherwise it offers to install [`72-perihub-uinput.rules`](packaging/linux/72-perihub-uinput.rules) from there.

> [!WARNING]
> With this rule, **every program running under your user account can simulate a keyboard and a mouse**, and type or click anything as if it were you. Install it only if you want software macros and trust all the software you run.

</details>

## Using PeriHub

- **Keyboard → Lighting**: pick an effect. It applies at once and is remembered.
- **Keyboard → Keys**: pick your layout, click a key, choose its new function and press **Save to keyboard**. **With SteelSeries key** changes what keys do while the SteelSeries key is held.
- **Mouse**: pick a profile at the top, then change DPI, buttons and lighting. Nothing is written until you press **Save to mouse**. DPI and lighting are previewed live; leaving the page brings the saved settings back.
- **G-Shift**: give "G-Shift" to a button. While you hold it, the other buttons do what the G-Shift tab says.

**Closing the window does not quit PeriHub.** It keeps running in the tray with much less memory, to animate the keyboard lighting and reapply it after a replug or a wake-up. To quit: **Quit** in the tray menu, or **Settings → Quit PeriHub**.

- **Windows 11** hides new tray icons behind the **^** arrow. To keep PeriHub visible: **Settings → Personalization → Taskbar → Other system tray icons → PeriHub**.
- **Autostart** can be turned on in Settings. On Hyprland it needs `xdg-desktop-autostart.target` (uwsm or systemd), or `exec-once = /path/to/perihub --hidden` in your configuration. For the tray icon you need a bar with a tray, such as Waybar or Noctalia.

## Safety

PeriHub never touches firmware. It uses the same settings commands as the official apps, with more safeguards:

- **Only known commands.** The code can send only commands that have been checked, and refuses the others before they reach the device. The command that restarts the Apex into its firmware updater cannot even be written. On Logitech mice only a short list of HID++ features is used, and a receiver's pairing commands are never sent. On Corsair devices only property reads, and with experimental lighting the commands lighting needs, can get through.
- **Untested devices are only read**, unless you turn on an [experimental feature](#experimental-features).
- **Every write is checked.** Mouse profiles are changed on top of their original bytes and read back byte by byte. The keyboard's key mapping is tried in RAM first and saved only if the read-back matches.
- **Backups** before every write: in `~/.config/perihub/backups/` on Linux and `%APPDATA%\perihub\backups\` on Windows. **Backups…** puts one back, writing only what differs.
- **Factory profiles are never written**: they are only read, to restore a profile.
- **Your key presses stay on your PC.** Reactive lighting and software macros read keys only while they need to, keep only which zone to light or which macro to start, and drop keys from other devices at once. Nothing you type is stored, logged or sent.
- **No data leaves your PC.** PeriHub works offline. The only connection: on Windows it asks GitHub for the latest version at start and once a day (you can turn this off), and installs an update only if you ask and its signature matches PeriHub's key.

More: [how the release files are signed](CODE_SIGNING.md) · [reporting a security issue](SECURITY.md).

## Performance

| PeriHub 0.5.5 on Windows | |
|---|---|
| Download | **3.9 MB** installer |
| On disk | **17 MB** |
| In the tray | **3 MB** of memory, **0.05%** of one CPU core |
| Window open | **39 MB** of memory, **0.2%** of one CPU core |
| Background services, account, ads, telemetry | none |

PeriHub is written in Rust and draws its window with WebView2, the web engine of Windows, instead of shipping its own browser. The engine runs only while the window is open. An animated lighting effect, drawn by PeriHub frame by frame, adds about 0.6% of one core. How these numbers were measured: [docs/development.md](docs/development.md#performance-measurements).

We do not publish numbers for G HUB or GG: we have not measured them on the same machine, and they support many more devices and features. To compare on your PC, run one app at a time and look at Task Manager → Processes. G HUB is a trademark of Logitech, SteelSeries GG of SteelSeries, iCUE of Corsair; PeriHub is not affiliated with them.

## FAQ

**Can I keep GG, G HUB or iCUE running?**
No: quit them and turn off their autostart. G HUB, for example, takes control of the mouse when it starts and after every wake-up, so the mouse stops using its onboard profiles. PeriHub shows a warning while G HUB runs, and gives the mouse its profiles back when G HUB is closed.

**Does this void my warranty?**
PeriHub does not touch the firmware. It writes only settings (profiles, key mapping) that the official apps write too. Experimental features come with [no warranty](#experimental-features).

**Why does the keyboard go back to the rainbow when PeriHub is closed?**
The Apex 3 TKL does not store colors, with GG too. PeriHub reapplies them as soon as it sees the keyboard.

**How do I undo a change?**
**Backups…** on the mouse page or the keyboard's Keys page, then **Restore** on the newest backup. For factory settings: **Keys → Reset all to factory** on the keyboard, **Factory settings** on each mouse profile.

**Something does not work: what should I attach to a bug report?**
The log file: **Settings → Problems → Open log folder**. It records devices connecting, errors and updates, never your key presses or the apps you use.

**Is PeriHub available in other languages?**
The interface is in English. More languages are planned.

## Known limitations

- **Windows**: tested on a real Windows 11 PC (lighting, key remapping, mouse profiles and macros, app profiles, tray, autostart, updates, software macros). Creating software macros has been tried on Linux only.
- **Logitech mice through a LIGHTSPEED receiver** are read-only and have not been tried on real hardware yet ([issue #4](https://github.com/hex-dump/perihub/issues/4)). Their buttons do not start software macros.
- **Corsair devices** have not been tried on real hardware yet. With more than one device on the same receiver, only the first one is shown, and the Slipstream V2 receiver is not recognized.
- **App profiles** work on Windows and Hyprland only. They switch lighting and mouse profiles, not key mappings.
- **Software macros** on Linux skip characters the keyboard layout cannot type. On Windows many games ignore typed text, but not keys.
- **Not available yet**: GameSense. **Out of scope**: SteelSeries Sonar and Moments, G HUB Lua scripts.

## Help and contributing

- **Questions and ideas**: [Discussions](https://github.com/hex-dump/perihub/discussions).
- **Bugs**: a [bug report](https://github.com/hex-dump/perihub/issues/new?template=bug_report.yml) with your system, the device, what PeriHub showed and the log file.
- **Test reports**: [Logitech mice](https://github.com/hex-dump/perihub/issues/new?template=mouse_test.yml), [Corsair devices and the G300](https://github.com/hex-dump/perihub/issues/new?template=device_test.yml), or **Open test form** in the app.
- **New devices**: a [device request](https://github.com/hex-dump/perihub/issues/new?template=device_request.yml) with the model and USB ID.
- **Code**: how to build, test and find your way around the code is in [docs/development.md](docs/development.md).

### Support the project

PeriHub is free. If it spared you an official app and you'd like to say thanks, you can [buy me a coffee](https://buymeacoffee.com/hexdump1). It is entirely optional and unlocks nothing: it helps buy new devices to add. Testing PeriHub on your hardware helps just as much.

## Credits and license

The protocols were worked out thanks to:

- [OpenRGB](https://gitlab.com/CalcProgrammer1/OpenRGB): the Apex 8-zone lighting protocol. No code was copied.
- [libratbag](https://github.com/libratbag/libratbag) (MIT): Logitech HID++ 2.0, onboard profiles, the list of Logitech mice and the G300's format.
- [gorkemgun/apex-3-tkl](https://github.com/gorkemgun/apex-3-tkl) (MIT): the Apex 3 TKL key mapping.
- [ckb-next](https://github.com/ckb-next/ckb-next) and [OpenLinkHub](https://github.com/jurkovic-nikola/OpenLinkHub): Corsair's protocol, described in our own words in [docs/protocols/corsair-bragi.md](docs/protocols/corsair-bragi.md). No code was copied.

PeriHub is released under the [MIT license](LICENSE). It is not affiliated with SteelSeries, Logitech or Corsair; product names belong to their owners.
