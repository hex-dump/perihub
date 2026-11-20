#!/usr/bin/env bash
# Brings main into the test/* branches a tester is still waiting on, so that their test builds have every fix
# of the latest release (a test build made before 0.9.6 lacked, for one, report links that fit through GitHub's
# login page). Run by refresh-test-branches.yml after a release is published, or by hand:
#
#   tools/refresh-test-branches.sh [--push] [BRANCH]
#
# A branch is refreshed when main is not in it yet and a commit of its own (not on main) has a "Test: #N" line for
# an issue that is still open. Conflicts are solved only where there is one right answer: the generated files are
# taken from main and written again by cargo xtask gen, the files with generated blocks keep the lines of both
# sides (the blocks are then written again), and CHANGELOG.md is main's with the branch's own entries moved to
# "## Unreleased" (tools/changelog_carry.py: a device on a test branch is in no release yet). Any other conflict, or a gen that fails, leaves the
# branch alone and is listed at the end (exit 1), for a person.
# With --push the merge is pushed and the branch's test build started (a push made with GITHUB_TOKEN does not start
# it by itself; with a personal token it would, so this is only for the workflow). Without it nothing leaves this
# clone. Needs git, gh (GH_TOKEN), cargo; works on a detached HEAD and leaves it as it found it.
set -euo pipefail

push=false
only=""
for arg in "$@"; do
  case "$arg" in
    --push) push=true ;;
    test/*) only=$arg ;;
    *) echo "usage: $0 [--push] [test/BRANCH]" >&2; exit 2 ;;
  esac
done
repo=${GITHUB_REPOSITORY:-$(gh repo view --json nameWithOwner --jq .nameWithOwner)}

# Written whole by cargo xtask gen, and the files it writes blocks of (crates/xtask).
whole=(site/devices.json packaging/linux/70-perihub.rules app/preview/data.json app/src/lib/devices.generated.json)
lines_of_both=(README.md docs/devices.md site/index.html packaging/linux/io.github.hex_dump.perihub.metainfo.xml)
in_list() { local f=$1; shift; for x in "$@"; do [ "$x" = "$f" ] && return 0; done; return 1; }

# The helper as it is here, whatever the branches have.
helpers=$(mktemp -d)
cp "$(dirname "$0")/changelog_carry.py" "$(dirname "$0")/release.py" "$helpers/"

git fetch --quiet --no-tags origin "+refs/heads/main:refs/remotes/origin/main" "+refs/heads/test/*:refs/remotes/origin/test/*"
start=$(git rev-parse HEAD)
trap 'git merge --abort 2>/dev/null || true; git checkout --quiet --detach "$start"; rm -rf "$helpers"' EXIT

refreshed=() skipped=() failed=()
for branch in $(git for-each-ref --format='%(refname:strip=3)' refs/remotes/origin/test/); do
  [ -n "$only" ] && [ "$branch" != "$only" ] && continue
  if git merge-base --is-ancestor origin/main "origin/$branch"; then
    skipped+=("$branch: has main already")
    continue
  fi
  waiting=()
  for n in $(git log --format=%B "origin/main..origin/$branch" | grep -iE '^\s*Tests?:\s*#[0-9]' | grep -oE '#[0-9]+' | tr -d '#' | sort -un); do
    state=$(gh api "repos/$repo/issues/$n" --jq 'if .pull_request then "pr" else .state end' 2>/dev/null || echo none)
    [ "$state" = open ] && waiting+=("#$n")
  done
  if [ ${#waiting[@]} -eq 0 ]; then
    skipped+=("$branch: no open issue waits on it")
    continue
  fi

  git checkout --quiet --detach "origin/$branch"
  if ! git merge --quiet --no-ff --no-commit origin/main > /dev/null 2>&1; then
    unsolved=()
    while IFS= read -r f; do
      [ -z "$f" ] && continue
      if [ "$f" = CHANGELOG.md ]; then
        continue  # written below, from the three versions
      elif in_list "$f" "${whole[@]}"; then
        git checkout --quiet --theirs -- "$f"
      elif in_list "$f" "${lines_of_both[@]}"; then
        tmp=$(mktemp -d)
        git show ":1:$f" > "$tmp/base" 2>/dev/null || : > "$tmp/base"
        git show ":2:$f" > "$tmp/ours"
        git show ":3:$f" > "$tmp/theirs"
        git merge-file --union -p "$tmp/ours" "$tmp/base" "$tmp/theirs" > "$f" || true
        rm -rf "$tmp"
      else
        unsolved+=("$f")
        continue
      fi
      git add -- "$f"
    done < <(git diff --name-only --diff-filter=U)
    if [ ${#unsolved[@]} -gt 0 ]; then
      git merge --abort
      failed+=("$branch: conflicts in ${unsolved[*]}")
      continue
    fi
  fi
  # CHANGELOG.md, conflict or not: main's, with the branch's entries under Unreleased.
  mb=$(git merge-base origin/main "origin/$branch")
  git show "$mb:CHANGELOG.md" > "$helpers/base.md"
  git show "origin/main:CHANGELOG.md" > "$helpers/main.md"
  git show "origin/$branch:CHANGELOG.md" > "$helpers/branch.md"
  python3 "$helpers/changelog_carry.py" "$helpers/base.md" "$helpers/main.md" "$helpers/branch.md" > CHANGELOG.md
  git add CHANGELOG.md
  if ! python3 tools/release.py lint > /dev/null 2>&1; then
    git merge --abort 2>/dev/null || git reset --quiet --hard
    failed+=("$branch: CHANGELOG.md does not pass the lint after the merge")
    continue
  fi
  if ! cargo xtask gen > "$helpers/gen.log" 2>&1; then
    # Its end, for the person who takes over (to stderr: the summary lists only the outcomes).
    tail -n 30 "$helpers/gen.log" >&2
    git merge --abort 2>/dev/null || git reset --quiet --hard
    failed+=("$branch: cargo xtask gen failed after the merge")
    continue
  fi
  git add -u
  git -c user.name="github-actions[bot]" -c user.email="41898282+github-actions[bot]@users.noreply.github.com" \
    commit --quiet --no-verify -m "Merge main into $branch: a test build with the latest release (${waiting[*]})"
  if $push; then
    git push --quiet origin "HEAD:refs/heads/$branch"
    # One build per refresh: the push above (GITHUB_TOKEN) starts none by itself.
    gh workflow run test-build.yml --repo "$repo" --ref "$branch"
  fi
  refreshed+=("$branch ($(git rev-parse --short HEAD), for ${waiting[*]})")
done

for s in "${refreshed[@]}"; do echo "refreshed: $s"; done
for s in "${skipped[@]}"; do echo "left as it is: $s"; done
for s in "${failed[@]}"; do echo "NEEDS A PERSON: $s"; done
[ ${#failed[@]} -eq 0 ]
