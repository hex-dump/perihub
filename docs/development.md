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

Quick checks before every commit (formatting, generated files, CHANGELOG sections, workflow YAML):

```sh
git config core.hooksPath .githooks   # once per clone; git commit --no-verify skips them
```

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

The command-line tool never saves to a mouse whose memory format is untested, and never uses
experimental lighting.

## Architecture

```
crates/perihub-core     Rust library: protocols, effects, device detection, configuration
  src/apex/             SteelSeries Apex 3 TKL: RGB (output reports) and key mapping (feature reports)
  src/hidpp/            Logitech HID++ 2.0, LIGHTSPEED receivers, the onboard profile format and the list of mice
                        (see docs/protocols/logitech-onboard-profiles.md)
  src/g300.rs           Logitech G300 (feature reports; see docs/protocols/logitech-g300.md)
  src/corsair/          Corsair mice, keyboards and receivers (see docs/protocols/corsair-bragi.md)
  src/effects.rs        lighting effects rendered on the PC
  src/manager.rs        device hotplug
  src/worker.rs         one thread per device, so requests never interleave
  tests/fixtures/       real G502 HERO sectors used by the tests
crates/perihub-cli      command-line tool
crates/xtask            generates the device lists from the driver tables (cargo xtask gen)
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

## Adding a device

The driver tables in `perihub-core` are the only list of devices: `peripheral::models()` for the
other brands, `hidpp::models::MODELS` and `hidpp::receiver::RECEIVERS` for Logitech,
`corsair::models` for Corsair. Everything else that names devices is generated from them:

```sh
cargo xtask gen     # writes the generated files
cargo xtask check   # fails if one is out of date (cargo test runs it too, so CI does)
```

| File | What is generated |
|---|---|
| `packaging/linux/70-perihub.rules` | the whole file: one rule per USB id, with the interface the table names |
| `docs/devices.md`, `README.md` | the tables, lists and counts between `<!-- generated: … -->` and `<!-- end generated -->` |
| `site/index.html` | the same, and the `data-models` the search uses after each `data-catalog` |
| `packaging/linux/…metainfo.xml` | `<modalias>` entries, so software centres suggest PeriHub for a supported device |
| `site/devices.json` | every device, its level and USB ids, published with the website |
| `app/src/lib/devices.generated.json` | the brands and models Settings lists, imported by the app at build time |

The text around the generated blocks is written by hand. What a block lists is a selector, described in
`crates/xtask/src/select.rs`: a family (`razer`), a brand (`Cooler Master`), a level
(`experimental`), a kind (`mouse`), joined with `:` (all of them) and `+` (either). A new
family needs a header in `crates/xtask/src/udev.rs`, which says what the rule opens and why: the
generator stops until it is there. The level is worked out from the driver: read-only when it may
send only read commands, experimental otherwise.

## Releasing

```sh
python3 tools/release.py bump 0.8.5     # needs a summary paragraph under "## Unreleased" in CHANGELOG.md
git commit -am "Version 0.8.5" && git push
```

Then, once CI is green on that commit, **Actions → Tag a release → Run workflow** on main. It tags the
version and starts the release workflow, which builds the installers and the Flatpak into a draft
release, checks every updater signature against the key in `tauri.conf.json` and `latest.json`
against the files, attaches `SHA256SUMS.txt`, and writes the notes from the version's section of
`CHANGELOG.md` (`python3 tools/release.py notes 0.8.5` shows them). Publishing the draft is up to
you; once it is published the website shows the new downloads, and the AUR workflow commits
`packaging/aur/perihub-bin` for it to main.

`CHANGELOG.md` sections, in this order: New devices, Added, Performance, Changed, Fixed, Security,
Removed, Known limitations. `python3 tools/release.py lint` checks them, and CI runs it.

## Automations

| Workflow | When | What it does |
|---|---|---|
| CI | every push and pull request | tests, formatting, clippy, generated files, CHANGELOG sections; dependency licenses and advisories (`deny.toml`, `tools/check_npm_licenses.py`) |
| Changelog | pull requests | a change to what PeriHub does needs a CHANGELOG line, or the `no changelog` label |
| Test build | every push to `test/…`, or by hand | Windows installer, AppImage, .deb and Flatpak as run artifacts |
| Tag a release | by hand, on main | tags the version once CI is green, starts Release |
| Release | the tag | draft release, signature and `latest.json` checks, `SHA256SUMS.txt`, notes from CHANGELOG.md; publishing opens an Announcements discussion |
| AUR package | a release is published | `packaging/aur/perihub-bin` for it, committed to main |
| Website | main, a published release | builds and publishes `site/` |
| Triage | a new issue | answers requests for devices PeriHub knows, asks old test reports to retry, labels brand and kind |
| Reminders | every day | one reminder when an author has not answered for 14 days |
| Upstream device ids | Mondays | ids other projects know and PeriHub has not looked at, in one issue |
| Links | Mondays | broken links in the README, docs and website, in one issue |
| Security advisories | Mondays | known vulnerabilities in the Rust crates and npm packages |
| Dependabot | weekly | one pull request per ecosystem (Cargo, npm, Actions) |
| Screenshots | by hand, a published release | `docs/screenshots` and the social preview from the interface with simulated devices (`npm run screenshots`, `app/preview`), committed when they change |

## Performance measurements

The [Measure on Windows](../.github/workflows/measure-windows.yml) workflow installs a release (or
builds a branch) on a fresh Windows Server 2025 virtual machine on GitHub Actions, with no devices
connected, and measures memory and CPU. Memory is the private working set, which is what Task
Manager shows in the Memory column, of PeriHub and, with the window open, the WebView2 processes it
starts. CPU is their processor time over 2 minutes.

| Version | Date | Machine | Tray | Window open |
|---|---|---|---|---|
| 0.8.2 | 13 November 2026 | AMD EPYC 7763, WebView2 156 | 3 MB, 0.08% of a core | 39 MB, 0.35% |
| 0.5.5 | 8 November 2026 | AMD EPYC 9V45, WebView2 156 | 3 MB, 0.05% of a core | 39 MB, 0.2% |
| 0.4.0 | 20 October 2026 | Intel Xeon Platinum 8573C, WebView2 154 | 2 MB | 42 MB |
| 0.3.0 | 19 October 2026 | AMD EPYC 7763 | 49 MB, about 2% | |

On a real PC (Windows 11, 16 logical processors) with both devices connected and the wave effect,
PeriHub 0.4.1 used 0.57–0.68% of one core in the tray, almost all of it to animate the wave.

On a real PC (Windows 11) with both devices connected, PeriHub 0.8.2 had been in the tray for 30
minutes and used 4.1 MB of memory and 0.37 to 0.68% of one core over 30 to 60 seconds
(13 November 2026). Whether an animated effect was on was not noted.
