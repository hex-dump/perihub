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
python3 -I -m unittest discover -s tools -p "test_*.py"   # the scripts in tools/ (triage, notify)
cargo xtask check               # generated files up to date
python3 tools/release.py lint   # CHANGELOG sections
```

`cargo test` also replays the transcripts in `crates/perihub-core/tests/fixtures/transcripts`:
what real devices answered, played back to the drivers without the device. A test report of a
device of another brand ends with one; [replay.md](replay.md) says how to turn it into a test.

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

While the app is open, `perihub-cli app ...` asks it to switch a profile, a DPI level or a lighting
preset (see [quick-actions.md](quick-actions.md)).

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
  src/kblayout.rs       keyboard layouts: what keyboards report, and the system's (see docs/protocols/keyboard-layouts.md)
  src/effects.rs        lighting effects rendered on the PC
  src/manager.rs        device hotplug
  src/worker.rs         one thread per device, so requests never interleave
  tests/fixtures/       real G502 HERO sectors used by the tests
crates/perihub-cli      command-line tool
crates/xtask            generates the device lists from the driver tables (cargo xtask gen)
app/                    UI: Tauri 2 + Svelte 5
packaging/linux         udev rules
docs/protocols          protocols described in our own words, written before the code
docs/captures.md        how a tester records what the official software sends to a device
docs/device-request.md  what a device request carries, and why reading the feature reports is safe
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
| `site/index.html` | the same, with a link to each model's page, and the `data-models` the search uses after each `data-catalog` |
| `packaging/linux/…metainfo.xml` | `<modalias>` entries, so software centres suggest PeriHub for a supported device |
| `site/devices.json` | every device, its level, USB ids and page address, published with the website; `site/build.py` writes one page per model from it (`site/templates/device.html`) and `help-test.html`, then checks every link |
| `app/src/lib/devices.generated.json` | the brands and models Settings lists, imported by the app at build time |

The text around the generated blocks is written by hand. What a block lists is a selector, described in
`crates/xtask/src/select.rs`: a family (`razer`), a brand (`Cooler Master`), a level
(`experimental`), a kind (`mouse`), joined with `:` (all of them) and `+` (either). A new
family needs a header in `crates/xtask/src/udev.rs`, which says what the rule opens and why: the
generator stops until it is there. The level is worked out from the driver: read-only when it may
send only read commands, experimental otherwise.

## Confirming a device

A device becomes supported when a tester's report shows it works. The confirmations are data, in one
table: `crates/perihub-core/src/confirmed.rs`, which lists the USB ids of devices of other brands and
Corsair devices, and the Logitech memory formats (profile format and sector size), each with the
issue whose report confirmed it. The catalogue (`cargo xtask gen`) lists a confirmed id as supported,
the interface shows it as tested, and `mouse::support` reads the formats from the same table.

```sh
cargo xtask confirm 1532:00b7 --issue 40                 # a device of another brand, or a Corsair device
cargo xtask confirm --memory-format 4:255 --issue 40     # a Logitech memory format (profile format:sector size)
```

It refuses an id PeriHub does not know, a Logitech mouse by USB id (its memory format decides), a
memory format PeriHub does not write, and anything confirmed already. Otherwise it adds the entry
(sorted, one per line), runs `cargo xtask gen`, and adds the line under "New devices" in
`CHANGELOG.md`. A confirmed device keeps the safety checks of an experimental one (No warranty box,
backup, read-back, undo) for now. With `--report FILE` (the text of the issue), the transcript at the
end of its test report also becomes a replay fixture ([replay.md](replay.md)) when it is whole, read-only
and the device's own; its `expect` holds only what the confirmation is about, and the values the tester
confirmed are added by hand.

The same happens by itself for a clean report: `tools/triage.py` (run on every new issue) says a test
report confirms something only if it was made by the app (first line `PeriHub X.Y.Z on …`, from the
test report form, with the report the **Test my device** wizard makes counting the same) from the
latest version (or a test build of a branch, whose first line is `PeriHub X.Y.Z (test build test/x@abc1234) on …`
and which is never told to update), the device is in the latest release's `site/devices.json` and not supported yet, there
or on main (or, for a device that is in no release and not on main, only on a `test/*` branch: the branch is found by the USB id in the
branch's `site/devices.json`, preferring the one the report's first line names; for a Logitech mouse: the memory format is not tested yet, PeriHub can write it, and the
mouse was saved to over the cable), no line of the report is an error (a transcript is skipped, a
cut one confirms nothing), "Messages and errors" and "What is wrong or missing" are empty, and the
boxes that say what PeriHub shows matches are ticked (for a Logitech mouse also the two about saving).
Anything doubtful confirms nothing, and a person handles the issue as before. The `Confirm a device`
workflow (`.github/workflows/confirm.yml`) then runs `cargo xtask confirm`, the core and xtask tests
and `cargo xtask check`, pushes `confirm/<id>` and opens a pull request that says "Closes #N"; merging
it is the maintainer's one click. For a device that is only on a test branch it takes that branch as the
base (input `branch`): the confirmation is pushed to the branch itself and the pull request from it to
main is opened, or the one open already is reused (a new device is never merged before a tester has
confirmed it; `tools/test-branch-devices.sh` lists the devices of the test branches). A pull request opened with `GITHUB_TOKEN` does not start CI, which
main's ruleset requires: close and reopen it. If Actions may not create pull requests in the
repository, it pushes the branch and comments with the link to open it. To run it by hand:
**Actions → Confirm a device → Run workflow** with the issue number. `tools/test_triage.py` tests the
decision.

A device request gets its answer from the same script: the level when the latest release knows the
device, "can be added directly" (label `ready to add`: a test build will be posted on the issue to try with
**Test my device**, and a clean report puts the device into the next release) when the id is in
`tools/upstream-seen.json` for a vendor PeriHub has a driver for (libratbag for Logitech, OpenRazer for
Razer, rivalcfg for SteelSeries), and "needs USB captures" (label `needs captures`, linking [captures.md](captures.md))
otherwise.

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
the PKGBUILD and .SRCINFO for it to the `aur` branch. To publish on aur.archlinux.org, take those two
files from that branch (`git clone --branch aur https://github.com/hex-dump/perihub.git`) into the AUR
repository, check them with `makepkg`, and push by hand.

`CHANGELOG.md` sections, in this order: New devices, Added, Performance, Changed, Fixed, Security,
Removed, Known limitations. `python3 tools/release.py lint` checks them, and CI runs it.

## Automations

| Workflow | When | What it does |
|---|---|---|
| CI | every push and pull request | tests, formatting, clippy, generated files, CHANGELOG sections; dependency licenses and advisories (`deny.toml`, `tools/check_npm_licenses.py`) |
| Changelog | pull requests | a change to what PeriHub does needs a CHANGELOG line, or the `no changelog` label |
| Test build | every push to `test/…`, or by hand | Windows installer, AppImage, .deb and Flatpak as run artifacts, which know their branch and commit (`PERIHUB_BUILD`, shown in the test report's first line; the Flatpak build tries twice); then one comment with the run link on each open issue the branch's commits name (`#30`), unless one already has it |
| Tag a release | by hand, on main | tags the version once CI is green, starts Release |
| Release | the tag | draft release, signature and `latest.json` checks, `SHA256SUMS.txt`, notes from CHANGELOG.md; publishing opens an Announcements discussion |
| Release comment | a release is published | one comment on each open issue the notes or commits name, saying the version is out; nothing is closed |
| AUR package | a release is published | PKGBUILD and .SRCINFO for it, committed to the `aur` branch (not main, which takes no pushes from bots) |
| Website | main, a published release | builds and publishes `site/` |
| Triage | a new issue | answers requests (the device's level, "ready to add" or "needs captures"), asks old test reports to retry, labels brand and kind; a clean test report starts Confirm a device |
| Auto-add a device | triage when a request is "ready to add" with an id a generator can write, or by hand (`issue` and `usb`) | for OpenRazer's mice (`tools/triage.py` `MECHANICAL`, `tools/auto_add.py` `GENERATORS`): the id goes in `tools/auto-add.json`, `tools/openrazer_facts.py` writes the table from the source at the commit the table names, `cargo xtask gen` and the checks run, and the commit (`Test: #N`) goes on `test/auto-VVVV-PPPP`; the branch is pushed, Test build is started by hand (a push made with the workflow token starts nothing), and a draft pull request to main is opened. If the generator cannot produce the id or a check fails, nothing is commented and the issue gets the label `needs maintainer`. Nothing is pushed to main, and nothing is merged before a tester has confirmed it |
| Confirm a device | triage, or by hand | `cargo xtask confirm` for the report in an issue, checks, and a pull request that makes the device supported; for a device only on a `test/…` branch, the commit goes to that branch and the pull request is from it to main |
| Reminders | every day | one reminder when an author has not answered a question for 14 days (device requests, test reports and unlabeled issues) |
| Waiting for reporter | a new comment on an issue | the label `waiting for reporter` when the maintainer asks something, removed when the reporter answers |
| Upstream device ids | Mondays | ids other projects know and PeriHub has not looked at, in one issue |
| Links | Mondays | broken links in the README, docs and website, in one issue |
| Security advisories | Mondays | known vulnerabilities in the Rust crates and npm packages |
| Dependabot | weekly | one pull request per ecosystem (Cargo, npm, Actions) |
| Screenshots | by hand, a published release | `docs/screenshots` and the social preview from the interface with simulated devices (`npm run screenshots`, `app/preview`), committed to the `assets` branch (`screenshots/`, `social-preview.png`) when they change, then starts Website; the README and the website read them from there |

### Devices added without a person

Which ids can be added by the Auto-add workflow is decided in two places that agree (a test keeps them in step):
`tools/triage.py` (`MECHANICAL`, a pure function `auto_addable` that needs only the catalogue and the ids the
upstream projects list) and `tools/auto_add.py` (`GENERATORS`, the (project, vendor) pairs whose table a script
writes from the project's source). Only OpenRazer's mice (1532) are such a pair today: at the commit the table
names, every Razer mouse is in the table except the receivers, docks and wireless dongles the generator leaves out,
and `tools/auto-add.json` (`{"allow": ["1532:00c1"]}`) lets an id through. Not the others: libratbag's Logitech
devices and the other brands have no generator (their drivers and models are written by hand), rivalcfg's table
already has every id of its pinned commit (a newer one needs the pin bumped, which brings every new id and changed
fact, so a person reads it), the Razer keyboard table leaves out only laptops and ids another driver shares, and
OpenRGB is for lights only. The generator's output may only gain the one device (`auto_add.check_diff`), else a
person takes over.

To try it without GitHub, in a clean checkout: `python3 tools/auto_add.py 1532:00c1 85 --src /tmp/openrazer --dry-run`
(it fetches the pinned commit into `--src`, builds the branch in a throwaway worktree, runs the checks, prints
the commit and removes everything; `--keep` keeps the worktree, `--skip-checks` leaves out the cargo checks).

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
