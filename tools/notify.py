#!/usr/bin/env python3
"""Comments on the issues a change is about, and the "waiting for reporter" label.

  notify.py testbuild SHA RUN_URL RUN_ID [--base origin/main] [--before OLD] [--branch NAME] [--apply]
      a test build of SHA is ready: the open issues named by a "Test: #N" or "Tests: #N, #M" line in
      a commit message new to this push (OLD..SHA, or BASE..SHA for a new branch) get a new comment,
      so they are notified; the earlier test build comments of the same branch are then deleted. Commits
      already on main or on another origin/test/* branch are not new. A plain "#N" does nothing.
  notify.py release VERSION RELEASE_URL [--notes FILE] [--apply]
      VERSION is published: one comment on each open issue the CHANGELOG notes of that version name,
      quoting those lines, unless the maintainer commented since the previous release that the fix
      ships (next version, will ship, fixed in...). It mentions switching from a test build only if
      an automatic test build comment was posted since the previous release.
  notify.py waiting EVENT.json [--apply]
      the issue_comment event: the maintainer asked something, so the issue is waiting for its
      reporter; the reporter answered, so it is not

Without --apply it only says what it would do. Each comment carries a marker, so running twice
posts once. Issue text is only read for "#N" numbers and question marks, never run. Uses the gh
command line, with GH_TOKEN in the workflows.
"""

import json
import re
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

REPO = "hex-dump/perihub"
MAINTAINER = "hex-dump"
WAITING = "waiting for reporter"
# The label tools/remind.py puts on an issue it closed because nobody answered.
NO_ANSWER = "no answer"
# Markers of the automatic comments: they never count as a question to the reporter.
MARKS = ("<!-- perihub-",)

REQUEST = re.compile(r"\b(could you|can you|would you|will you|please|let me know)\b", re.I)


def asks(body: str) -> bool:
    """Whether a comment asks the reader something: a question mark that ends a sentence, or a
    request word. Code, quotes and links do not count (a "?" in a URL is not a question), and nor do
    the automatic comments."""
    if any(m in body for m in MARKS):
        return False
    text = re.sub(r"```.*?```", " ", body, flags=re.S)
    text = re.sub(r"`[^`\n]*`", " ", text)
    text = re.sub(r"https?://\S+", " ", text)
    text = "\n".join(line for line in text.splitlines() if not line.lstrip().startswith(">"))
    return bool(re.search(r"\?(\s|$|[)\"'*_])", text) or REQUEST.search(text))


def gh(*args: str) -> str:
    return subprocess.run(["gh", *args], capture_output=True, text=True, check=True).stdout


def api(path: str) -> list[dict]:
    out = gh("api", "--paginate", path, "--jq", ".[]")
    return [json.loads(line) for line in out.splitlines() if line.strip()]


def git(*args: str) -> str:
    return subprocess.run(["git", *args], capture_output=True, text=True, check=True).stdout


# Only commit subjects are read: bodies often name other issues as examples ("as in #30"), and an
# issue named there is not one the change is about.
def refs(text: str) -> list[int]:
    """The issue numbers "#30", "issue #30" or "issues #24, #26" name, in order, once each."""
    found = []
    for n in re.findall(r"(?<![\w&/#])#(\d{1,6})\b", text):
        if int(n) not in found:
            found.append(int(n))
    return found


def when(stamp: str) -> datetime:
    return datetime.fromisoformat(stamp.replace("Z", "+00:00"))


def post(number: int, body: str, apply: bool) -> None:
    if apply:
        gh("issue", "comment", str(number), "--repo", REPO, "--body", body)


def bot(comment: dict) -> bool:
    return comment["user"]["login"].endswith("[bot]")


def open_issue(number: int) -> bool:
    """An open issue, not a pull request, with that number."""
    try:
        issue = json.loads(gh("api", f"repos/{REPO}/issues/{number}"))
    except subprocess.CalledProcessError:
        return False
    return issue["state"] == "open" and "pull_request" not in issue


def is_ancestor(old: str, new: str) -> bool:
    if not old or set(old) == {"0"}:
        return False
    return subprocess.run(["git", "merge-base", "--is-ancestor", old, new], capture_output=True).returncode == 0


def trailers(log: str) -> list[int]:
    """The issues named in "Test: #N" or "Tests: #N, #M" lines of commit messages, once each. A plain
    "#N" in a message does not count: it is often only a reference."""
    found = []
    for line in log.splitlines():
        m = re.fullmatch(r"\s*Tests?:\s*(#\d{1,6}(?:\s*,\s*#\d{1,6})*)\s*", line, re.I)
        if m:
            for n in refs(m.group(1)):
                if n not in found:
                    found.append(n)
    return found


TESTBUILD_MARK = "<!-- perihub-testbuild"
# <!-- perihub-testbuild:BRANCH:SHA -->, or the older <!-- perihub-testbuild:SHA --> without a branch.
TESTBUILD = re.compile(r"<!-- perihub-testbuild:(?:([^\s:]+):)?[0-9a-f]{7,40} -->")


def new_commits(sha: str, base: str, before: str, branch: str) -> str:
    """The messages of the commits this push brings: before..sha (base..sha for a new branch or when
    before is unknown), without what is on main or on any other test/* branch. A branch merged into a
    stacked one, or main merged into a branch, does not bring its commits again."""
    start = before if before and set(before) != {"0"} and subprocess.run(
        ["git", "cat-file", "-e", before + "^{commit}"], capture_output=True).returncode == 0 else base
    others = [r for r in git("for-each-ref", "--format=%(refname)", "refs/remotes/origin/test/").split()
              if r != f"refs/remotes/origin/{branch}"]
    exclude = [f"^{r}" for r in [base, *others]]
    return git("log", "--format=%B", f"{start}..{sha}", *exclude)


def testbuild(sha: str, url: str, run_id: str, base: str, apply: bool, before: str = "", branch: str = "") -> None:
    sha = git("rev-parse", sha).strip()
    branch = branch.removeprefix("refs/heads/")
    built = when(git("show", "-s", "--format=%cI", sha).strip()).astimezone(timezone.utc)
    mark = f"{TESTBUILD_MARK}:{branch}:{sha} -->"
    for number in trailers(new_commits(sha, base, before, branch)):
        if not open_issue(number):
            continue
        comments = api(f"repos/{REPO}/issues/{number}/comments?per_page=100")
        body = (
            f"New test build of {branch or 'this branch'} for this, from {built:%Y-%m-%d %H:%M} UTC: {url}\n\n"
            "Download the file for your system at the bottom of that page (you need to be signed in to GitHub). "
            "It shows the version number of the last release, that's expected. Install it, connect the device and run "
            "**Test my device**: the report it makes says which test build it is from.\n\n"
            f"{mark}"
        )
        # Only our own test build comments of this branch, or from before the marker had a branch, are
        # replaced; a comment of another branch, or anyone else's, is never touched.
        ours = [
            c for c in comments
            if bot(c) and (m := TESTBUILD.search(c["body"])) and m.group(1) in (None, branch)
        ]
        if any(url in c["body"] or f"actions/runs/{run_id}" in c["body"] for c in comments):
            print(f"#{number}: a comment already has the link of this run")
        # The maintainer posted a test build by hand after this commit, with the steps to try.
        elif not ours and any(
            c["user"]["login"] == MAINTAINER and when(c["created_at"]) > built and "/actions/runs/" in c["body"]
            for c in comments
        ):
            print(f"#{number}: the maintainer already posted a test build for this")
        else:
            # A new comment, so the reporter is notified (an edit would not be); then the old ones go.
            print(f"#{number}: comment with {url}" + (f", delete {len(ours)} old" if ours else ""))
            post(number, body, apply)
            for c in ours if apply else []:
                gh("api", "-X", "DELETE", f"repos/{REPO}/issues/comments/{c['id']}")


def previous_tag(version: str) -> str | None:
    def key(tag: str) -> tuple:
        return tuple(int(n) for n in tag[1:].split("."))

    tags = [t for t in git("tag", "--list", "v*.*.*").split() if re.fullmatch(r"v\d+\.\d+\.\d+", t)]
    older = [t for t in tags if key(t) < key(f"v{version}")]
    return max(older, key=key) if older else None


# A maintainer comment that already promised the fix: "will ship", "in the next version/release",
# "fixed in", "coming in". Deliberately loose, a false match only means one comment less.
PROMISED = re.compile(
    r"\b(next (release|version|update)|will (ship|be (in|part of|included|released|fixed|out))|"
    r"(ships|shipping|included|fixed|coming|landing|lands) in|in the (next|upcoming) )",
    re.I,
)
MAX_LINE = 160


def changelog_lines(notes: str) -> dict[int, list[str]]:
    """For each issue number, the CHANGELOG lines of the notes that name it, as plain short text."""
    out: dict[int, list[str]] = {}
    for line in notes.splitlines():
        if not line.strip() or line.startswith("#"):
            continue
        # Only "issue #N", "issues #N and #M": other "#N" are pull requests of other projects.
        named = re.findall(r"issues? #\d{1,6}(?:(?:,| and|, and) #\d{1,6})*", line, re.I)
        for number in refs(" ".join(named)):
            text = re.sub(r"\[([^\]]*)\]\([^)]*\)", r"\1", line.strip().lstrip("-* ").strip())
            text = re.sub(r"[*_`]", "", text)
            if len(text) > MAX_LINE:
                cut = text[:MAX_LINE]
                # End at a sentence if there is one in the second half, else at a word.
                end = max(cut.rfind(". "), cut.rfind("; "))
                text = cut[:end] + "." if end > MAX_LINE // 2 else cut.rsplit(" ", 1)[0] + "..."
            out.setdefault(number, []).append(text)
    return out


def release(version: str, url: str, notes: str, apply: bool) -> None:
    mark = f"<!-- perihub-release:{version} -->"
    previous = previous_tag(version)
    since = when(git("log", "-1", "--format=%cI", previous).strip()) if previous else None
    # Only the issues the CHANGELOG of this version names: there is a line to quote for them.
    for number, lines in changelog_lines(notes).items():
        if not open_issue(number):
            continue
        comments = api(f"repos/{REPO}/issues/{number}/comments?per_page=100")
        said = (mark, f"releases/tag/v{version}", f"{version} is out")
        if any(s in c["body"] for c in comments for s in said):
            print(f"#{number}: already told about {version}")
            continue
        recent = [c for c in comments if since is None or when(c["created_at"]) > since]
        if any(c["user"]["login"] == MAINTAINER and PROMISED.search(c["body"]) for c in recent):
            print(f"#{number}: the maintainer already said it ships")
            continue
        quoted = "\n".join(f"> {t}" for t in lines[:1])
        body = f"{version} has this: {url}\n\n{quoted}"
        if any(TESTBUILD_MARK in c["body"] for c in recent):
            body += "\n\nIf you were using the test build, you can switch to the release."
        body += f"\n\n{mark}"
        print(f"#{number}: comment about {version}")
        post(number, body, apply)


def waiting(event_path: str, apply: bool) -> None:
    event = json.loads(Path(event_path).read_text())
    issue, comment = event["issue"], event["comment"]
    who = comment["user"]
    if "pull_request" in issue or who.get("type") == "Bot" or who["login"].endswith("[bot]"):
        return
    names = {label["name"] for label in issue.get("labels", [])}
    has = WAITING in names
    reporter = issue["user"]["login"]
    number = str(issue["number"])
    # Closed by tools/remind.py for want of an answer: the answer opens it again, as that comment promised.
    if who["login"] == reporter and issue.get("state") == "closed" and NO_ANSWER in names:
        print(f"#{number}: {reporter} answered after it was closed for no answer, reopened")
        if apply:
            gh("issue", "reopen", number, "--repo", REPO)
            gh("issue", "edit", number, "--repo", REPO, "--remove-label", NO_ANSWER, *(["--remove-label", WAITING] if has else []))
        return
    if who["login"] == reporter:
        if has:
            print(f"#{number}: {reporter} answered, label off")
            if apply:
                gh("issue", "edit", number, "--repo", REPO, "--remove-label", WAITING)
    elif who["login"] == MAINTAINER and asks(comment["body"]) and not has:
        print(f"#{number}: the maintainer asked, label on")
        if apply:
            gh("label", "create", WAITING, "--repo", REPO, "--color", "fbca04", "--force",
               "--description", "The maintainer asked the reporter something")
            gh("issue", "edit", number, "--repo", REPO, "--add-label", WAITING)


def main() -> None:
    args = sys.argv[1:]
    apply = "--apply" in args
    args = [a for a in args if a != "--apply"]

    def option(name: str, default: str) -> str:
        if name in args:
            i = args.index(name)
            value = args[i + 1]
            del args[i : i + 2]
            return value
        return default

    base, notes_file = option("--base", "origin/main"), option("--notes", "")
    before, branch = option("--before", ""), option("--branch", "")
    if args[:1] == ["testbuild"] and len(args) == 4:
        testbuild(args[1], args[2], args[3], base, apply, before, branch)
    elif args[:1] == ["release"] and len(args) == 3:
        release(args[1], args[2], Path(notes_file).read_text() if notes_file else "", apply)
    elif args[:1] == ["waiting"] and len(args) == 2:
        waiting(args[1], apply)
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main()
