#!/bin/sh
# Builds the PeriHub Flatpak from this working tree and writes the bundle each release attaches
# (.build/PeriHub_<version>_x86_64.flatpak). The release workflow runs it too.
#
# Needs flatpak, flatpak-builder and git. The GNOME runtime and the Rust and Node SDK extensions come
# from Flathub, installed without asking: the Flathub remote is added for the current user if it is
# missing.
#   --no-install  only build the bundle; by default it is also installed for the current user.
#   --system      use the system installation and its flathub remote (the release workflow's container
#                 has the GNOME runtime there); implies --no-install.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
work="$here/.build"
installation="--user"
install="--install"
for arg in "$@"; do
    case "$arg" in
        --no-install) install="" ;;
        --system) installation="--system"; install="" ;;
        *) echo "unknown option: $arg" >&2; exit 2 ;;
    esac
done

if [ "$installation" = "--user" ]; then
    flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
fi

# Flathub's shared modules, for the tray icon library, at the commit this was tested with.
modules="$here/shared-modules"
if [ ! -d "$modules" ]; then
    git clone --quiet https://github.com/flathub/shared-modules.git "$modules"
    git -C "$modules" checkout --quiet e69357f3b96c2572456b5a371e4b973d98fdd284
fi

version=$(sed -n 's/^version = "\(.*\)"/\1/p' "$here/../../Cargo.toml" | head -1)
mkdir -p "$work"
flatpak-builder "$installation" $install --force-clean --install-deps-from=flathub --assumeyes \
    --state-dir="$work/state" --repo="$work/repo" "$work/build" "$here/io.github.hex_dump.perihub.yml"
flatpak build-bundle --runtime-repo=https://dl.flathub.org/repo/flathub.flatpakrepo \
    "$work/repo" "$work/PeriHub_${version}_x86_64.flatpak" io.github.hex_dump.perihub
echo "Bundle: $work/PeriHub_${version}_x86_64.flatpak"
