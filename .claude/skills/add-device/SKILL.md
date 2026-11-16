---
name: add-device
description: Add support for a new device or brand to PeriHub, from sources to test build. Use when the user asks to support a mouse, keyboard or receiver, or answers a device request issue.
---

# Adding a device to PeriHub

Follow these steps in order. Ask the user before anything that publishes (push, issue comments,
releases); the test build reply on an issue is already agreed.

## 1. What the device is and what it can do

- USB id (`vvvv:pppp`), interfaces and HID collections: from the device request ("What PeriHub
  saw"), or ask for them. `site/devices.json` says whether PeriHub already knows it.
- **Everything** the official software can change on it, not only what the requester uses. Plan
  support for all of it; parts without a source wait for captures from a tester.

## 2. Sources, and how to use them

- Allowed: answers from the device, USB captures (USBPcap/Wireshark) by testers or published by other
  projects, projects that tried the commands on hardware, the maker's published specifications.
- Never: facts from decompiling or reading the official software or its web drivers, even when a
  third party did it.
- GPL sources give **facts only**: describe the protocol in our own words in `docs/protocols/<name>.md`
  (commands, bytes, answers, sources cited), then write the Rust from that page alone, never with their
  code open, never translated line by line, no GPL dependencies. MIT/BSD/WTFPL sources may be used
  with attribution.
- Credit every source in `THIRD-PARTY-NOTICES.md` and in the credits of `app/src/pages/Settings.svelte`
  ("Protocol documented by X. No code was copied." for facts-only sources).

## 3. The driver

- A module in `crates/perihub-core/src/peripheral/` implementing `task::Driver`, its `MODELS` table
  added to `peripheral::models()` and a `Protocol` variant to `task::driver`. Logitech HID++ mice go in
  `hidpp::models::MODELS` instead.
- `commands()` lists every command the driver may send, with its `Effect`: `Io` refuses anything
  else, and a read-only connection sends only reads. Never list bootloader, flash, ISP, reset, pairing
  or factory-default commands.
- Until someone tests it: reads first; changes are experimental. Saving to memory needs the No
  warranty box, a backup, a read-back and the 15-second undo (`PeripheralTask` does this). Changes
  that last until unplugged show a warning, with no "Edit anyway". A device that cannot read its
  settings back is never saved to.
- Never open an interface that carries a keyboard's typing, and never a receiver shared with other
  devices. The table's `interface` is the one a source names; `None` opens the whole device.
- Answers that do not look as documented make the page read-only with the reason.
- Tests with `FakeHid` (`peripheral::fake`) and `task::start_for_test`: reads as documented, only
  listed commands, read-only stays read-only, wrong answers refused. Mutate the parser once to see a
  test fail.

## 4. Everything generated from the table

```sh
cargo xtask gen        # udev rule, devices.md tables and counts, website, AppStream modalias, devices.json
```

A new family needs a header in `crates/xtask/src/udev.rs` (what the rule opens, and why) and, for
devices.md and the website, a generated block where the family is described:
`<!-- generated: table SELECTOR -->` … `<!-- end generated -->` (selectors: `crates/xtask/src/select.rs`).
Write the text around it by hand: level, what PeriHub reads and changes, what is left out.
udev files that use `uaccess` must sort before `73-seat-late.rules`.

Also by hand: the brand in `.github/ISSUE_TEMPLATE/01_test_report.yml` (description and checkboxes; the brand dropdown there and in `device_request.yml` has the five named brands and Other, add a brand to it, and to `tools/labels-migrate.sh`, only when it has a label of its own), and
README.md where it lists brands. `mouse_test.yml` and `device_test.yml` are near-copies of the test report form
for installed PeriHub 0.9.0 and older (their links open those files): change the three together, and delete the
two once those versions are no longer in use.

## 5. Checks

```sh
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
cd app && npm run check && npx vitest run
python3 tools/release.py lint
```

## 6. CHANGELOG and the test build

- Under `## Unreleased`, in `### New devices` (sections and order: `tools/release.py`): what is
  supported, at which level, from which source.
- Commit on a local branch. With the user's OK: push it as `test/<name>`, run
  `gh workflow run test-build.yml --ref test/<name>`, and once it is green answer the issue with the
  link to the run (artifacts need a GitHub account; the build still says the previous version).
- No release until the tester confirms, or the user asks for one.
