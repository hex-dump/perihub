#!/usr/bin/env python3
"""Issues waiting for their author: one reminder after 7 days, closed after 14 days without an answer.

  remind.py           says what it would do
  remind.py --apply   comments and closes (the reminder workflow, once a day)

An issue is waiting when its last comment, not counting automatic ones, is the maintainer's and asks
something (a question or a request: a note that asks nothing waits for no one). Then:
  - 7 days later it gets one reminder, which says it will be closed a week later without an answer;
  - 14 days after the question, and at least 7 days after that reminder, it is closed as "not planned" with
    the label "no answer" and a comment saying that any reply opens it again (waiting.yml reopens it when its
    author comments).
Only where closing is surely right (closable()), else it gets the reminder at most and is never closed:
  - opened by someone else than the maintainer, open, not locked, not a pull request;
  - it has the label "waiting for reporter" (the maintainer's question put it on, and an answer takes it off);
  - every label is one of SAFE_LABELS (device request, test report, needs captures, mouse, brand: X...):
    "ready to add", "needs maintainer", "keep open", packaging, bugs and the rest mean work on our side;
  - no open pull request and no test/* branch names it: a test build or a fix is waiting for it.
Uses the gh command line, with GH_TOKEN in the workflow.
"""

import json
import re
import subprocess
import sys
from datetime import datetime, timedelta, timezone
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from notify import asks  # noqa: E402

REPO = "hex-dump/perihub"
MAINTAINER = "hex-dump"
LABELS = ("device request", "test report")
WAITING = "waiting for reporter"
NO_ANSWER = "no answer"
# Issues with any of these labels, or with none, are reminded.
LOOKED_AT = {*LABELS, WAITING}
# The only labels an issue may have to be closed: anything else is work on our side, or a person's choice.
SAFE_LABELS = {*LABELS, WAITING, "needs captures", "mouse", "keyboard", "question"}
REMIND_AFTER = timedelta(days=7)
CLOSE_AFTER = timedelta(days=14)
# The author is told a week ahead: never closed sooner than this after the reminder.
WARNED_FOR = timedelta(days=7)
MARK = "<!-- perihub-reminder -->"
# In a reminder that told the author the issue would be closed: only such a reminder lets it close.
WARNED = "<!-- perihub-close-warning -->"
CLOSE_MARK = "<!-- perihub-closed-no-answer -->"
BOTS = ("github-actions[bot]",)


def gh_list(path: str) -> list[dict]:
    """Every item of a list from the GitHub API, all pages."""
    out = subprocess.run(["gh", "api", "--paginate", path, "--jq", ".[]"], capture_output=True, text=True, check=True).stdout
    return [json.loads(line) for line in out.splitlines() if line.strip()]


def when(stamp: str) -> datetime:
    return datetime.fromisoformat(stamp.replace("Z", "+00:00"))


def safe_label(name: str) -> bool:
    return name in SAFE_LABELS or name.startswith("brand: ")


def closable(issue: dict, linked: set[int]) -> bool:
    """Whether closing the issue for want of an answer is surely right (see the module's text)."""
    names = {label["name"] for label in issue["labels"]}
    return (
        issue["state"] == "open"
        and not issue.get("locked")
        and "pull_request" not in issue
        and issue["user"]["login"] != MAINTAINER
        and WAITING in names
        and all(safe_label(n) for n in names)
        and issue["number"] not in linked
    )


def decide(issue: dict, comments: list[dict], now: datetime, linked: set[int]) -> str | None:
    """"remind", "close" or None for an issue and its comments, oldest first."""
    names = {label["name"] for label in issue["labels"]}
    if "pull_request" in issue or issue["state"] != "open" or issue["user"]["login"] == MAINTAINER:
        return None
    if names and not names & LOOKED_AT:
        return None
    people = [c for c in comments if c["user"]["login"] not in BOTS]
    if not people or people[-1]["user"]["login"] != MAINTAINER or not asks(people[-1]["body"]):
        return None
    asked = when(people[-1]["created_at"])
    reminders = [when(c["created_at"]) for c in comments if MARK in c["body"] and when(c["created_at"]) > asked]
    if not reminders:
        return "remind" if now - asked >= REMIND_AFTER else None
    warned = [when(c["created_at"]) for c in comments if WARNED in c["body"] and when(c["created_at"]) > asked]
    if warned and closable(issue, linked) and now - asked >= CLOSE_AFTER and now - warned[-1] >= WARNED_FOR:
        return "close"
    return None


def linked_issues() -> set[int]:
    """The issues an open pull request names (title or text), or a commit of a test/* branch that is not on main
    names in a "Test: #N" line: work that waits for them."""
    found: set[int] = set()
    for pr in gh_list(f"repos/{REPO}/pulls?state=open&per_page=100"):
        found |= {int(n) for n in re.findall(r"#(\d{1,6})\b", f"{pr['title']} {pr.get('body') or ''}")}
    refs = gh_list(f"repos/{REPO}/git/matching-refs/heads/test/")
    for ref in refs:
        branch = ref["ref"].removeprefix("refs/heads/")
        out = subprocess.run(
            ["gh", "api", f"repos/{REPO}/compare/main...{branch}", "--jq", ".commits[].commit.message"],
            capture_output=True, text=True,
        )
        if out.returncode != 0:
            # Unknown: the issues of this branch cannot be told apart, so nothing is closed today.
            raise SystemExit(f"cannot compare {branch} with main: nothing is closed")
        for line in out.stdout.splitlines():
            m = re.fullmatch(r"\s*Tests?:\s*(#\d{1,6}(?:\s*,\s*#\d{1,6})*)\s*", line, re.I)
            if m:
                found |= {int(n) for n in re.findall(r"\d+", m.group(1))}
    return found


def reminder(issue: dict, closes: bool) -> str:
    names = {label["name"] for label in issue["labels"]}
    what = (
        "an answer or a new test report would help: it moves the device forward for everyone who has it. "
        if names & set(LABELS)
        else "an answer here would help. "
    )
    later = (
        "If there is no answer in a week, this issue will be closed for now; any reply opens it again. "
        if closes
        else ""
    )
    return (
        f"Hi @{issue['user']['login']}, a gentle reminder in case the message above got lost. When you have a moment, "
        f"{what}{later}No rush, and thanks!\n\n<sub>Automatic reminder, sent once.</sub>\n{MARK}" + (f"\n{WARNED}" if closes else "")
    )


CLOSING = (
    "Closing this for now, since there has been no answer for two weeks. That's fine, nothing is lost: **reply here "
    "at any time and it opens again by itself.**\n\n<sub>Closed automatically.</sub>\n" + CLOSE_MARK
)


def run(cmd: list[str], apply: bool) -> None:
    if apply:
        subprocess.run(cmd, check=True)


def main() -> None:
    apply = sys.argv[1:] == ["--apply"]
    now = datetime.now(timezone.utc)
    linked = linked_issues()
    for issue in gh_list(f"repos/{REPO}/issues?state=open&per_page=100"):
        if "pull_request" in issue:
            continue
        comments = gh_list(f"repos/{REPO}/issues/{issue['number']}/comments?per_page=100")
        action = decide(issue, comments, now, linked)
        n = str(issue["number"])
        if action == "remind":
            print(f"#{n}: reminder" + (" (will close in a week)" if closable(issue, linked) else ""))
            run(["gh", "issue", "comment", n, "--repo", REPO, "--body", reminder(issue, closable(issue, linked))], apply)
        elif action == "close":
            print(f"#{n}: close, no answer")
            run(["gh", "label", "create", NO_ANSWER, "--repo", REPO, "--color", "cfd3d7", "--force",
                 "--description", "Closed after two weeks without an answer; a reply opens it again"], apply)
            run(["gh", "issue", "comment", n, "--repo", REPO, "--body", CLOSING], apply)
            run(["gh", "issue", "edit", n, "--repo", REPO, "--add-label", NO_ANSWER], apply)
            run(["gh", "issue", "close", n, "--repo", REPO, "--reason", "not planned"], apply)


if __name__ == "__main__":
    main()
