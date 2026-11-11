#!/bin/sh
# Builds and installs the PeriHub Flatpak from this working tree, to try it before a release.
#
# Needs flatpak and flatpak-builder, the Flathub remote (flatpak remote-add --user --if-not-exists
# flathub https://dl.flathub.org/repo/flathub.flatpakrepo), and cargo-sources.json and
# node-sources.json in this folder (see README.md). The manifest is the Flathub one, with the
# working tree as its source instead of the release tag.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
work="$here/.build"
mkdir -p "$work"

for generated in cargo-sources.json node-sources.json; do
    [ -f "$here/$generated" ] || { echo "missing $here/$generated: see README.md" >&2; exit 1; }
    cp "$here/$generated" "$work/"
done
[ -d "$work/shared-modules" ] || git clone --depth 1 https://github.com/flathub/shared-modules.git "$work/shared-modules"

# The git source (tag and commit of a release) becomes this folder.
python3 -I - "$here/io.github.hex_dump.perihub.yml" "$work/io.github.hex_dump.perihub.yml" "$root" <<'EOF'
import sys
source, target, root = sys.argv[1:]
lines = open(source).read().splitlines()
out, skipping = [], False
for line in lines:
    if line.strip() == "- type: git":
        indent = line[: len(line) - len(line.lstrip())]
        out += [
            f"{indent}- type: dir",
            f"{indent}  path: {root}",
            f"{indent}  skip: [target, app/node_modules, app/dist, packaging/flatpak/.build]",
        ]
        skipping = True
        continue
    if skipping and line.strip().startswith("- "):
        skipping = False
    if not skipping:
        out.append(line)
open(target, "w").write("\n".join(out) + "\n")
EOF

flatpak-builder --user --install --force-clean --install-deps-from=flathub --repo="$work/repo" "$work/build" "$work/io.github.hex_dump.perihub.yml"
echo "Installed: flatpak run io.github.hex_dump.perihub"
