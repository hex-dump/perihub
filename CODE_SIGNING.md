# Code signing policy

## Windows

The Windows installers (`PeriHub_…_x64-setup.exe` and `PeriHub_…_x64_en-US.msi`) are not code-signed yet, so SmartScreen may warn when you run them (**More info → Run anyway**). PeriHub is applying to [SignPath Foundation](https://signpath.org) for its free code signing for open source projects. Once the project is accepted, the installers and `perihub.exe` will be signed, and this page will say:

> Free code signing provided by [SignPath.io](https://about.signpath.io), certificate by [SignPath Foundation](https://signpath.org).

Only files built from this repository by the [release workflow](.github/workflows/release.yml) on GitHub Actions will be signed, and every signing request is approved by hand. A signature also does not make the warning go away at once: SmartScreen trusts a publisher after its signed releases have been downloaded for a while.

## Linux

The AppImage, `.deb` and `.rpm` packages are not signed with a distribution key. Every release lists the SHA-256 checksum of each file in `SHA256SUMS.txt`.

## Update files

The `.sig` files and `latest.json` in each release are for PeriHub's own updater on Windows: it installs a new version only if its signature matches the public key in [tauri.conf.json](app/src-tauri/tauri.conf.json). They are separate from Windows code signing.

## Team roles

- Committers and reviewers: [hex-dump](https://github.com/hex-dump), the maintainer
- Approvers: [hex-dump](https://github.com/hex-dump)

Pull requests from other people are reviewed by the maintainer before they are merged.

## Privacy

PeriHub does not transfer any information to other networked systems unless the user asks for it, with one exception on Windows: when it starts and then once a day, PeriHub asks GitHub for the latest release, and it can be turned off in **Settings → Updates**. An update is downloaded only when the user clicks **Update and restart**. On Linux, and on Windows when Scoop installed PeriHub, nothing is checked.

There is no account, no ads and no telemetry. Opening the test form or a macro's website starts the user's browser, which connects on its own. See [Safety](README.md#safety) for what PeriHub reads from the devices.
