---
name: add-device
description: Add support for a new device or brand to PeriHub, from sources to main and the next release (with an optional test build). Use when the user asks to support a mouse, keyboard or receiver, or answers a device request issue.
---

# Adding a device to PeriHub

**First check whether it is done already.** A request for an id that a generator writes (today: a Razer mouse
OpenRazer lists; `tools/triage.py` `MECHANICAL`) is added by the Auto-add workflow with nobody doing anything: it
makes the branch `test/auto-VVVV-PPPP` (`tools/auto_add.py`: the id in `tools/auto-add.json`, the generator, `cargo xtask
gen`, the checks, a `Test: #N` commit), starts the test build and opens a normal pull request, not a draft:
the maintainer merges it after CI, and the device is in the next release as experimental. Look for that branch
and for the label `needs maintainer` on the issue: the label means the generator could not produce the id (a
laptop, a keyboard, a mouse with no command PeriHub knows, an id newer than the pinned commit) or a check failed,
so the rest of this page applies, by hand. Do not make a second branch for an id that has one. Never bump a
generator's pinned commit just to add one id: it brings every new id and changed fact.

Follow these steps in order. Ask the user before anything that publishes (push, issue comments,
releases); the test build reply on an issue is already agreed.

**The release rule** (maintainer rule): a device written from public documentation goes into main and the next
release as **experimental**, without waiting for a tester: read-only where the documentation does not allow
checking what the device does, and its changes only behind the No warranty box, the read-back and the undo (step
3). A clean test report from **Test my device** later makes it **supported** (step 7). A test build stays
available for early testing, but nothing waits for it.

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
- Until someone tests it (it ships like this, in the next release): reads first; changes are experimental. Saving to memory needs the No
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

## 6. CHANGELOG, main and the test build

**The support goes into main, and so into the next release, as experimental** (see the release rule above;
also a device with a known source, such as one the triage answered "ready to add"). Work on a branch of its
own (`devices/<name>`, or `test/<name>` when a test build is wanted from the first push), open a pull request to
main with the user's OK, and the maintainer merges it after CI. Nothing waits for a tester.

```
device request -> (documented upstream / has a driver: "ready to add") -> support written on a branch,
pull request to main -> CI -> the maintainer merges it -> it ships in the next release as experimental
(earlier, if wanted: a test build, below -> the tester runs Test my device)
-> a clean report -> the device is confirmed (cargo xtask confirm, confirm.yml) -> supported
```

- Under `## Unreleased`, in `### New devices` (sections and order: `tools/release.py`): what is
  supported, at which level (experimental or read-only, never "supported" before a clean report), from which
  source. One entry per device.
- **A test build is optional**, for a tester who can try the device before the release. A push (with the user's
  OK) to a branch named `test/<name>` starts it.
  **The push already starts test-build.yml: never also run `gh workflow run test-build.yml`**, its concurrency
  cancels the build that is running (and cancelling the new one stops them all). The build says which branch and
  commit it is (`PERIHUB_BUILD`): the test report starts `PeriHub 0.9.5 (test build test/<name>@abc1234) on ...`,
  which triage reads (a test build is never told to update). The build still shows the previous release's
  number, that is expected.
- Put a line `Test: #N` (or `Tests: #N, #M`) in the body of a commit of the branch: that is what makes the
  build announce itself on the device request (a plain `#N` does nothing). A build with new commits
  posts a new comment on the open issue and deletes the earlier test build comments of the branch.
  The triage answer to a "ready to add" request tells the requester the device goes into the next release as
  experimental, and that a test build's link comes on the issue if one is made (for an id the Auto-add workflow
  writes, it says the support is being made automatically).
  The Auto-add workflow starts the build itself with `gh workflow run test-build.yml --ref test/auto-…`, because
  its push is made with the workflow token and starts nothing; a push made by you does start it, so never do both.
- The tester installs the test build, connects the device and runs **Test my device**. Whether or not anyone
  does, the device ships as experimental with the next release.
- A device that "needs captures" waits for them (`docs/captures.md`); once a driver is written from
  them, it goes the same way.

## 7. When the tester confirms

A clean report (made by the app from the latest version, or from a test build; no error
line, nothing under "Messages and errors" or "What is wrong or missing", the boxes ticked; the one
from **Test my device** counts the same) is picked up by the triage workflow, which starts
confirm.yml for a device the latest release has: `cargo xtask confirm` on a branch of its own, and a pull
request to main that the maintainer merges (CI does not start on a pull request a workflow opens: close and
reopen it), or a comment with the link when Actions may not open pull requests. A device that only a `test/<name>`
branch has (it is not on main yet) is confirmed on that branch, which the workflow finds by the device's USB id
in the branch's `site/devices.json`, preferring the one the report's first line names; the commit goes to the
branch and the pull request from it to main is opened or reused. A device that is on main but in no release yet
is left to the maintainer until a release has it.
By hand, for a device of another brand or a Corsair one, or a Logitech memory format:

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
