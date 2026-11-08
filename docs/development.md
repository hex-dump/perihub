# Building and developing PeriHub

## Build from source

You need stable [Rust](https://rustup.rs) (≥ 1.88) and [Node.js](https://nodejs.org) ≥ 20.19.
On Linux you also need the development libraries for WebKitGTK 4.1, libayatana-appindicator,
librsvg and libudev:

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

Don't build the app with plain `cargo build`: that binary loads the UI from the development
server (`localhost:1420`).

## Tests and checks

Run them before opening a pull request (CI runs them too, on Linux and Windows):

```sh
cargo fmt --all                 # formatting
cargo test --workspace          # includes tests on sectors read from a real G502 HERO
cargo clippy --workspace --all-targets
cd app && npm run check         # frontend type check
cd app && npx vitest run        # frontend tests
```

## Command-line tool

Useful for diagnostics and scripting. It refuses to start while the app is open, since both would
compete for the same devices, and the app waits for it to finish.

```sh
cargo run -p perihub-cli -- status
cargo run -p perihub-cli -- mouse state
cargo run -p perihub-cli -- keyboard keymap
cargo run -p perihub-cli -- mouse backups
cargo run -p perihub-cli -- keyboard light '{"type":"wave","period_ms":3000,"reverse":false}' 10
```

The command-line tool never uses experimental editing or experimental lighting.

## Architecture

```
crates/perihub-core     Rust library: protocols, effects, device detection, configuration
  src/apex/             SteelSeries Apex 3 TKL: RGB (output reports) and key mapping (feature reports)
  src/hidpp/            Logitech HID++ 2.0, LIGHTSPEED receivers, the onboard profile format and the list of mice
  src/g300.rs           Logitech G300 (feature reports, read only)
  src/corsair/          Corsair mice, keyboards and receivers (see docs/protocols/corsair-bragi.md)
  src/effects.rs        lighting effects rendered on the PC
  src/manager.rs        device hotplug
  src/worker.rs         one thread per device, so requests never interleave
  tests/fixtures/       real G502 HERO sectors used by the tests
crates/perihub-cli      command-line tool
app/                    UI: Tauri 2 + Svelte 5
packaging/linux         udev rules
packaging/aur           perihub-bin PKGBUILD
docs/protocols          protocols described in our own words, written before the code
```

- Device communication goes through [hidapi](https://github.com/libusb/hidapi). On Linux the
  hidraw backend is used, so the keyboard and mouse keep working normally while PeriHub is open.
- On Windows the mouse's short and long HID++ collections are separate devices; PeriHub uses the
  long one (report `0x11`). Behind a LIGHTSPEED receiver it also uses the short one, where the
  receiver answers for a mouse that is asleep.
- Each device family is a separate module in `perihub-core`, which is where new hardware is added.
- New protocols are first described in `docs/protocols/`, in our own words and with their sources,
  and the code is written from that page. No code from other projects is copied.

## Performance measurements

The [Measure on Windows](../.github/workflows/measure-windows.yml) workflow installs a release (or
builds a branch) on a fresh Windows Server 2025 virtual machine on GitHub Actions, with no devices
connected, and measures memory and CPU. Memory is the private working set, which is what Task
Manager shows in the Memory column, of PeriHub and, with the window open, the WebView2 processes it
starts. CPU is their processor time over 2 minutes.

| Version | Date | Machine | Tray | Window open |
|---|---|---|---|---|
| 0.5.5 | 8 November 2026 | AMD EPYC 9V45, WebView2 156 | 3 MB, 0.05% of a core | 39 MB, 0.2% |
| 0.4.0 | 20 October 2026 | Intel Xeon Platinum 8573C, WebView2 154 | 2 MB | 42 MB |
| 0.3.0 | 19 October 2026 | AMD EPYC 7763 | 49 MB, about 2% | |

On a real PC (Windows 11, 16 logical processors) with both devices connected and the wave effect,
PeriHub 0.4.1 used 0.57–0.68% of one core in the tray, almost all of it to animate the wave.
