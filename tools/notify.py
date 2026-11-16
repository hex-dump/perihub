#!/usr/bin/env python3
"""Comments on the issues a change is about, once, and the "waiting for reporter" label.

  notify.py testbuild SHA RUN_URL RUN_ID [--base origin/main] [--apply]
      a test build of SHA is ready: one comment on each open issue that "#N" names in the commit
      messages of BASE..SHA (the workflow's test build job)
  notify.py release VERSION RELEASE_URL [--notes FILE] [--apply]
      VERSION is published: one comment on each open issue named in its release notes or in the
      commits since the previous tag (the release comment workflow)
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
from datetime import datetime
from pathlib import Path

REPO = "hex-dump/perihub"
MAINTAINER = "hex-dump"
WAITING = "waiting for reporter"
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


def open_issue(number: int) -> bool:
    """An open issue, not a pull request, with that number."""
    try:
        issue = json.loads(gh("api", f"repos/{REPO}/issues/{number}"))
    except subprocess.CalledProcessError:
        return False
    return issue["state"] == "open" and "pull_request" not in issue


def testbuild(sha: str, url: str, run_id: str, base: str, apply: bool) -> None:
    sha = git("rev-parse", sha).strip()
    mark = f"<!-- perihub-testbuild:{sha} -->"
    committed = when(git("show", "-s", "--format=%cI", sha).strip())
    for number in refs(git("log", "--format=%B", f"{base}..{sha}")):
        if not open_issue(number):
            continue
        comments = api(f"repos/{REPO}/issues/{number}/comments?per_page=100")
        if any(mark in c["body"] for c in comments):
            print(f"#{number}: already told about {sha[:8]}")
        # The maintainer often posts the link by hand, with the steps to try: no second comment.
        elif any(
            when(c["created_at"]) > committed and (url in c["body"] or f"actions/runs/{run_id}" in c["body"])
            for c in comments
        ):
            print(f"#{number}: a comment already has the link of this run")
        else:
            body = (
                f"New test build for this: {url}\n\n"
                "Download the file for your system at the bottom of that page (you need to be signed in to GitHub). "
                f"It still says the previous version number, that's expected.\n\n{mark}"
            )
            print(f"#{number}: comment with {url}")
            post(number, body, apply)


def previous_tag(version: str) -> str | None:
    def key(tag: str) -> tuple:
        return tuple(int(n) for n in tag[1:].split("."))

    tags = [t for t in git("tag", "--list", "v*.*.*").split() if re.fullmatch(r"v\d+\.\d+\.\d+", t)]
    older = [t for t in tags if key(t) < key(f"v{version}")]
    return max(older, key=key) if older else None


def release(version: str, url: str, notes: str, apply: bool) -> None:
    mark = f"<!-- perihub-release:{version} -->"
    text = notes
    previous = previous_tag(version)
    if previous:
        text += "\n" + git("log", "--format=%B", f"{previous}..v{version}")
    for number in refs(text):
        if not open_issue(number):
            continue
        if any(mark in c["body"] for c in api(f"repos/{REPO}/issues/{number}/comments?per_page=100")):
            print(f"#{number}: already told about {version}")
            continue
        body = (
            f"{version} is out with this: {url}. "
            f"If you were using a test build, you can switch to the release now.\n\n{mark}"
        )
        print(f"#{number}: comment about {version}")
        post(number, body, apply)


def waiting(event_path: str, apply: bool) -> None:
    event = json.loads(Path(event_path).read_text())
    issue, comment = event["issue"], event["comment"]
    who = comment["user"]
    if "pull_request" in issue or who.get("type") == "Bot" or who["login"].endswith("[bot]"):
        return
    has = WAITING in {label["name"] for label in issue.get("labels", [])}
    reporter = issue["user"]["login"]
    number = str(issue["number"])
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
    if args[:1] == ["testbuild"] and len(args) == 4:
        testbuild(args[1], args[2], args[3], base, apply)
    elif args[:1] == ["release"] and len(args) == 3:
        release(args[1], args[2], Path(notes_file).read_text() if notes_file else "", apply)
    elif args[:1] == ["waiting"] and len(args) == 2:
        waiting(args[1], apply)
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main()
