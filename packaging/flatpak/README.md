# PeriHub as a Flatpak

Each release has a Flatpak bundle, `PeriHub_X.Y.Z_x86_64.flatpak`, built from this repository by the
release workflow. PeriHub is not on Flathub: Flathub does not accept manifests or submissions written with
AI tools, and much of PeriHub is. Flathub only provides the GNOME runtime the bundle runs on.

The Flatpak's application ID is `io.github.hex_dump.perihub`, in the form Flathub uses for GitHub projects
(the user `hex-dump`, with the dash written as an underscore). The Tauri app keeps
`io.github.hexdump.perihub` everywhere else: the Windows installer and its updates depend on it.

Files:

- `io.github.hex_dump.perihub.yml`: the manifest. It builds PeriHub from the repository it sits in, with
  network access: npm and cargo download the packages pinned in `app/package-lock.json` and `Cargo.lock`.
- `build-local.sh`: builds the bundle, `.build/PeriHub_X.Y.Z_x86_64.flatpak`, and installs it for the
  current user (`--no-install` only builds it). The release workflow runs it too. It copies the working
  tree, untracked files included: to rebuild a release by hand, run it in a clean checkout of the tag.
- `../linux/io.github.hex_dump.perihub.desktop` and `.metainfo.xml`: installed by the manifest. Add each
  release to the metainfo's `<releases>`.
- `shared-modules/`: [Flathub's shared modules](https://github.com/flathub/shared-modules), cloned by
  `build-local.sh` at a fixed commit, for the tray icon library the GNOME runtime does not have.

## Inside the sandbox

- **USB permissions**: a Flatpak cannot install udev rules. Users install `70-perihub.rules` on the host, with
  the command in the main README, as with the AppImage. The optional rules (key presses, software macros,
  Unifying receivers) cannot be installed from PeriHub either: it shows the commands instead.
- **Autostart** goes through the Background portal, which writes the entry on the host. Desktops without
  that portal, such as Hyprland, need `flatpak run io.github.hex_dump.perihub --hidden` in their own
  autostart: PeriHub says so.
- **Other programs** cannot be seen from the sandbox, so PeriHub cannot warn about Solaar or logiops, or keep
  experimental Corsair lighting off while OpenRGB runs: the interface asks to quit them.
- **Settings** live in `~/.var/app/io.github.hex_dump.perihub/config/perihub`, not in `~/.config/perihub`.
- **Updates**: the bundle does not update by itself. Installing the next release's bundle replaces it.
