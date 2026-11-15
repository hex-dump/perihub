#!/usr/bin/env python3
"""A single, gentle reminder on device requests and test reports waiting for their author.

  remind.py           says what it would do
  remind.py --apply   comments (the reminder workflow, once a day)

An issue is waiting when its last comment, not counting automatic ones, is the maintainer's and is
more than 14 days old. It gets one reminder for that comment, never more, and is never closed.
Uses the gh command line, with GH_TOKEN in the workflow.
"""

import json
import subprocess
import sys
from urllib.parse import quote
from datetime import datetime, timedelta, timezone

REPO = "hex-dump/perihub"
MAINTAINER = "hex-dump"
LABELS = ("device request", "device test", "mouse test")
WAIT = timedelta(days=14)
MARK = "<!-- perihub-reminder -->"
BOTS = ("github-actions[bot]",)


def gh(path: str) -> list[dict]:
    """Every item of a list from the GitHub API, all pages."""
    out = subprocess.run(["gh", "api", "--paginate", path, "--jq", ".[]"], capture_output=True, text=True, check=True).stdout
    return [json.loads(line) for line in out.splitlines() if line.strip()]


def when(stamp: str) -> datetime:
    return datetime.fromisoformat(stamp.replace("Z", "+00:00"))


def main() -> None:
    apply = sys.argv[1:] == ["--apply"]
    now = datetime.now(timezone.utc)
    seen = set()
    for label in LABELS:
        for issue in gh(f"repos/{REPO}/issues?state=open&labels={quote(label)}&per_page=100"):
            if issue["number"] in seen or "pull_request" in issue:
                continue
            seen.add(issue["number"])
            author = issue["user"]["login"]
            if author == MAINTAINER:
                continue
            comments = gh(f"repos/{REPO}/issues/{issue['number']}/comments?per_page=100")
            people = [c for c in comments if c["user"]["login"] not in BOTS]
            if not people or people[-1]["user"]["login"] != MAINTAINER:
                continue
            last = people[-1]
            if now - when(last["created_at"]) < WAIT:
                continue
            if any(MARK in c["body"] and when(c["created_at"]) > when(last["created_at"]) for c in comments):
                continue
            body = (
                f"Hi @{author}, a gentle reminder in case the message above got lost. When you have a moment, "
                "an answer or a new test report would help: it moves the device forward for everyone who has it. "
                "No rush, and thanks!\n\n<sub>Automatic reminder, sent once.</sub>\n" + MARK
            )
            print(f"#{issue['number']} ({author}): waiting since {last['created_at'][:10]}")
            if apply:
                subprocess.run(["gh", "issue", "comment", str(issue["number"]), "--repo", REPO, "--body", body], check=True)


if __name__ == "__main__":
    main()
