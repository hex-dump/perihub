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

- Allowed: answers from the device, USB captures (USBPcap/Wireshark; testers follow `docs/captures.md`,
  link it to them) by testers or published by other
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
cargo xtask check
python3 -I -m unittest discover -s tools -p "test_*.py"
```

## 6. CHANGELOG, the test branch and the test build

**A new device is never merged into main, and never shipped in a release, before a tester has
confirmed it on real hardware** (maintainer rule, always: also a device with a known source, such as
one the triage answered "ready to add"). So the support always goes on a `test/<name>` branch, never
straight to main:

```
device request -> (documented upstream / has a driver: "ready to add") -> support written on test/<name>,
a commit with a "Test: #N" line -> test-build.yml builds it, tools/notify.py comments the link on the issue
-> the tester installs that test build and runs Test my device -> a clean report -> the device is
confirmed ON THAT BRANCH (cargo xtask confirm) and a pull request from the branch to main is opened
-> the maintainer merges it -> it ships in the next release
```

- Under `## Unreleased`, in `### New devices` (sections and order: `tools/release.py`): what is
  supported, at which level, from which source.
- Commit on a local branch and, with the user's OK, push it as `test/<name>`. **The push already starts
  test-build.yml: never also run `gh workflow run test-build.yml`**, its concurrency cancels the build
  that is running (and cancelling the new one stops them all). The build says which branch and commit
  it is (`PERIHUB_BUILD`): the test report starts `PeriHub 0.9.5 (test build test/<name>@abc1234) on ...`,
  which triage reads (a test build is never told to update, and its report confirms the device on that
  branch). The build still shows the previous release's number, that is expected.
- Put a line `Test: #N` (or `Tests: #N, #M`) in the body of a commit of the branch: that is what makes the
  build announce itself on the device request (a plain `#N` does nothing). A build with new commits
  posts a new comment on the open issue and deletes the earlier test build comments of the branch.
  The triage answer to a "ready to add" request already tells the requester that this link will come.
- The tester installs the test build, connects the device and runs **Test my device**.
- No pull request to main is merged, and no release made, until the confirmation (step 7) is on the branch.
  A device that "needs captures" waits for them (`docs/captures.md`); once a driver is written from
  them, it goes the same way.

## 7. When the tester confirms

A clean report (made by the app from the latest version, or from a test build of the branch; no error
line, nothing under "Messages and errors" or "What is wrong or missing", the boxes ticked; the one
from **Test my device** counts the same) is picked up by the triage workflow, which starts
confirm.yml with the test branch as the base (it finds the branch by the device's USB id in the
branch's `site/devices.json`, preferring the one the report's first line names). The workflow runs
`cargo xtask confirm` on the branch, pushes the commit **to the branch** and opens (or reuses) the pull
request from it to main, or comments the link when Actions may not open pull requests: the maintainer
only merges it (CI does not start on a pull request a workflow opens: close and reopen it).
By hand, on the `test/<name>` branch, for a device of another brand or a Corsair one, or a Logitech
memory format:

```sh
cargo xtask confirm 1532:00b7 --issue 40
cargo xtask confirm --memory-format 4:255 --issue 40
cargo xtask confirm 1532:00b7 --issue 40 --report ISSUE_BODY.md   # also the replay fixture, if the report has a transcript
```

then push the branch and open the pull request to main. It adds the entry to
`crates/perihub-core/src/confirmed.rs`, runs `cargo xtask gen` and writes the
CHANGELOG line under "New devices". Do not edit the device tables or the tested memory formats by hand
(`mouse::support` reads `confirmed::MEMORY`). Then run the checks of step 5. A fixture made from a
report only expects what the confirmation is about: add the values the tester confirmed
(`docs/replay.md`).
