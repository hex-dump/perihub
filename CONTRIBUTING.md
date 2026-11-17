# Contributing to PeriHub

Thanks for helping! PeriHub is maintained by one person in their spare time, so the most useful
contributions are the ones that need a real device, which the maintainer does not have.

## Ways to help

- **Test your device.** On its page in PeriHub, **Open test form** fills in a test report. A report
  only reads the device, and one report moves the device up a level for everyone who has it.
- **Ask for a device.** A [device request](https://github.com/hex-dump/perihub/issues/new?template=device_request.yml)
  with the model, the USB ID, and everything the official software can change on it, not only what you use.
- **Ideas and suggestions.** Share them in [Discussions → Ideas](https://github.com/hex-dump/perihub/discussions/categories/ideas),
  where others can vote and add to them. Questions go in [Q&A](https://github.com/hex-dump/perihub/discussions/categories/q-a).
- **Bugs.** A [bug report](https://github.com/hex-dump/perihub/issues/new?template=bug_report.yml) with your system,
  the device, what PeriHub showed and the log file.
- **Code.** Pull requests are welcome. For anything larger than a fix, open an issue or an idea first,
  so we can agree on the approach before you spend time on it.
  Commits on a `test/*` branch can name the issue they are for with a `Test: #N` (or `Tests: #N, #M`)
  line; the test build then comments on it, and later builds edit that comment. A plain `#N` does nothing.

## Rules for code

- **PeriHub is MIT, and its code is its own.** A protocol is first described in our own words in
  `docs/protocols/`, with its sources, and the code is written from that page. Code from GPL projects
  (OpenRGB, OpenRazer, ckb-next…) is never copied or translated: only facts are taken from them.
  Code under permissive licenses (MIT, BSD…) may be used with credit in `THIRD-PARTY-NOTICES.md`.
- **Only observed facts.** Device answers, USB captures and published specifications. Never facts
  obtained by decompiling or reading the code of the makers' software, even when someone else did it.
- **Devices are never put at risk.** A command that is not in a driver's list is refused; untested
  devices are read-only or experimental, with a backup, a read-back and an undo before anything is
  saved. Bootloader, flash and pairing commands are never sent.
- **Dependencies** must have one of the licenses in `deny.toml`: CI checks every one.

## Before opening a pull request

`git config core.hooksPath .githooks` turns on quick checks before every commit. Then:

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace            # also fails if a file generated from the device tables is out of date
cd app && npm run check && npx vitest run
```

- After changing a device table, run `cargo xtask gen`: it rewrites the udev rule, the device list,
  the website and the AppStream metadata. See [docs/development.md](docs/development.md#adding-a-device).
- Add a line to `CHANGELOG.md` under `## Unreleased`, in the right section (New devices, Added,
  Performance, Changed, Fixed…). The release notes are made from it.

Building, the architecture and the release process are described in [docs/development.md](docs/development.md).
By contributing you agree that your contribution is licensed under the [MIT license](LICENSE), and
to follow the [code of conduct](CODE_OF_CONDUCT.md).
