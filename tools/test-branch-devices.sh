#!/usr/bin/env bash
# The devices of every test/* branch: {"test/x": [the "devices" of site/devices.json on that branch], ...}
# written to the file given. tools/triage.py reads it (BRANCHES.json) to find the branch a device that no
# release and not main has is on. Needs gh and jq, and GH_TOKEN as in a workflow (triage.yml sets GITHUB_REPOSITORY;
# without it the repository of 'gh repo view' is used).
# A branch without a readable site/devices.json is left out.
set -euo pipefail
out=${1:?usage: test-branch-devices.sh OUT.json}
repo=${GITHUB_REPOSITORY:-$(gh repo view --json nameWithOwner --jq .nameWithOwner)}
printf '{}' > "$out"
tmp=$(mktemp)
trap 'rm -f "$tmp" "$out.new"' EXIT
while IFS= read -r branch; do
  case "$branch" in test/*) ;; *) continue ;; esac
  if gh api -X GET "repos/$repo/contents/site/devices.json" -f ref="$branch" -H "Accept: application/vnd.github.raw" > "$tmp" 2> /dev/null \
    && jq -e '.devices | type == "array"' "$tmp" > /dev/null 2>&1; then
    jq --arg b "$branch" --slurpfile d "$tmp" '. + {($b): $d[0].devices}' "$out" > "$out.new"
    mv "$out.new" "$out"
  else
    echo "$branch: no site/devices.json, left out" >&2
  fi
done < <(gh api --paginate "repos/$repo/git/matching-refs/heads/test/" --jq '.[].ref | ltrimstr("refs/heads/")')
