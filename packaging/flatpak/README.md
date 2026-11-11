# PeriHub on Flathub

The Flatpak's application ID is `io.github.hex_dump.perihub`: Flathub requires the GitHub user in the ID,
with the dash written as an underscore. The Tauri app keeps `io.github.hexdump.perihub` everywhere else (the
Windows installer and its updates depend on it).

Files:

- `io.github.hex_dump.perihub.yml`: the manifest submitted to Flathub. It builds PeriHub from source,
  offline, at a release tag.
- `flathub.json`: x86_64 only, like the other packages.
- `build-local.sh`: builds and installs the Flatpak from the working tree, to try it before a release.
- `../linux/io.github.hex_dump.perihub.desktop` and `.metainfo.xml`: installed by the manifest. Flathub
  requires them in the project itself. Add each release to the metainfo's `<releases>`.

## The generated sources

Flathub builds without network access, so every Rust crate and npm package is listed with its checksum in
`cargo-sources.json` and `node-sources.json`. They are generated with the official
[flatpak-builder-tools](https://github.com/flatpak/flatpak-builder-tools) from `Cargo.lock` and
`app/package-lock.json`, and must be generated again whenever one of those changes:

```sh
git clone --depth 1 https://github.com/flatpak/flatpak-builder-tools.git /tmp/fbt
python3 -m venv /tmp/fbt-venv && /tmp/fbt-venv/bin/pip install aiohttp tomlkit /tmp/fbt/node
/tmp/fbt-venv/bin/python /tmp/fbt/cargo/flatpak-cargo-generator.py Cargo.lock -o packaging/flatpak/cargo-sources.json
/tmp/fbt-venv/bin/python -m flatpak_node_generator npm app/package-lock.json -o packaging/flatpak/node-sources.json
```

They are not committed here: they go to the Flathub repository with the manifest.

## Inside the sandbox

- **USB permissions**: a Flatpak cannot install udev rules. Users install `70-perihub.rules` on the host, with
  the command in the main README, as for OpenRGB or Solaar on Flathub. The optional rules (key presses,
  software macros, Unifying receivers) cannot be installed from PeriHub either: it shows the commands instead.
- **Autostart** goes through the Background portal, which writes the entry on the host.
- **Other programs** cannot be seen from the sandbox, so PeriHub cannot warn about Solaar or logiops, or keep
  experimental Corsair lighting off while OpenRGB runs: the interface asks to quit them.
- **Settings** live in `~/.var/app/io.github.hex_dump.perihub/config/perihub`, not in `~/.config/perihub`.

## Releasing

After a release: update the tag and commit in the manifest, generate the sources again if a lock file changed,
and open a pull request with them in the Flathub repository of the app.
