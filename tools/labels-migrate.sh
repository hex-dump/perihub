#!/usr/bin/env bash
# One-time label migration for the repository's issues (run by the maintainer, after the change that
# adds .github/ISSUE_TEMPLATE/01_test_report.yml is merged):
#
#   tools/labels-migrate.sh --dry-run   shows what it would do, changes nothing
#   tools/labels-migrate.sh             does it
#
# - makes (or updates, if they exist) the "test report" label and the brand labels, with their
#   descriptions, and fixes the descriptions of "device request", "mouse" and "keyboard";
# - puts "test report" on every open and closed issue that has "mouse test" or "device test";
# - deletes "mouse test" and "device test", only when every issue of that label has been moved.
# Safe to run again. Needs the gh command line, logged in with write access to the repository.
set -euo pipefail

REPO="${REPO:-hex-dump/perihub}"
DRY=0
case "${1:-}" in
  "") ;;
  --dry-run) DRY=1 ;;
  *) echo "usage: $0 [--dry-run]" >&2; exit 2 ;;
esac

run() {
  if [ "$DRY" = 1 ]; then echo "[dry run] $*"; else "$@"; fi
}

# "gh label create --force" makes the label, or changes the colour and description of an existing one.
label() { # name colour description
  run gh label create "$1" --repo "$REPO" --color "$2" --description "$3" --force
}

label "test report" 0e8a16 "A device's test report, from the test form"
label "device request" 1d76db "Support for a new device"
label "mouse" c5def5 "Mice"
label "keyboard" c5def5 "Keyboards"
# Same colour as "brand: Logitech".
label "brand: Logitech" d4c5f9 "Logitech devices"
label "brand: SteelSeries" d4c5f9 "SteelSeries devices"
label "brand: Corsair" d4c5f9 "Corsair devices"
label "brand: Razer" d4c5f9 "Razer devices"
label "brand: ASUS" d4c5f9 "ASUS devices"
label "brand: Other" d4c5f9 "Devices of other brands"

for old in "mouse test" "device test"; do
  # Open and closed. Pull requests are not listed by "gh issue list".
  numbers=$(gh issue list --repo "$REPO" --label "$old" --state all --limit 1000 --json number --jq '.[].number')
  failed=0
  count=0
  for n in $numbers; do
    count=$((count + 1))
    if ! run gh issue edit "$n" --repo "$REPO" --add-label "test report" --remove-label "$old"; then
      echo "#$n: could not be moved from \"$old\"" >&2
      failed=1
    fi
  done
  echo "\"$old\": $count issues"
  if [ "$count" -ge 1000 ]; then
    echo "\"$old\" has 1000 issues or more: run again before deleting it" >&2
    failed=1
  fi
  if [ "$failed" = 0 ]; then
    # A label that is already gone is not an error.
    if gh label list --repo "$REPO" --limit 200 --json name --jq '.[].name' | grep -qxF "$old"; then
      run gh label delete "$old" --repo "$REPO" --yes
    fi
  else
    echo "\"$old\" kept" >&2
    exit 1
  fi
done
echo "Done."
