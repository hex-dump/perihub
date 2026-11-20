"""Tests for tools/notify.py: python3 -I -m unittest discover -s tools -p 'test_*.py'"""

import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))
import notify  # noqa: E402

S1, S2 = "a" * 40, "b" * 40


def comment(cid, login, body):
    return {"id": cid, "user": {"login": login}, "body": body, "created_at": "2000-01-01T00:00:00Z"}


class Trailers(unittest.TestCase):
    def test_only_test_lines(self):
        self.assertEqual(notify.trailers("fix\n\nTest: #30\nTests: #4, #5\nas in #99\n"), [30, 4, 5])


class Git(unittest.TestCase):
    def run_git(self, *a):
        return subprocess.run(["git", "-C", self.d, "-c", "user.name=t", "-c", "user.email=t@t", *a],
                              capture_output=True, text=True, check=True).stdout.strip()

    def commit(self, msg):
        self.run_git("commit", "--allow-empty", "-q", "-m", msg)
        return self.run_git("rev-parse", "HEAD")

    def test_new_commits_skip_other_branches(self):
        with tempfile.TemporaryDirectory() as self.d:
            self.run_git("init", "-q", "-b", "main")
            self.commit("base")
            self.run_git("update-ref", "refs/remotes/origin/main", "HEAD")
            self.run_git("checkout", "-q", "-b", "lights")
            self.commit("lights\n\nTest: #30")
            self.run_git("update-ref", "refs/remotes/origin/test/lights", "HEAD")
            self.run_git("checkout", "-q", "-b", "usb", "main")
            before = self.commit("usb\n\nTest: #31")
            self.run_git("merge", "-q", "--no-edit", "lights")
            after = self.run_git("rev-parse", "HEAD")
            self.run_git("update-ref", "refs/remotes/origin/test/usb", "HEAD")
            cwd = os.getcwd()
            os.chdir(self.d)
            self.addCleanup(os.chdir, cwd)
            with mock.patch.object(notify, "git", lambda *a: self.run_git(*a) + "\n"), \
                 mock.patch.object(notify, "is_ancestor", lambda *a: True):
                log = notify.new_commits(after, "origin/main", before, "test/usb")
                self.assertEqual(notify.trailers(log), [])
                # a new branch: its own commit counts, the merged branch's does not
                log = notify.new_commits(after, "origin/main", "0" * 40, "test/usb")
                self.assertEqual(notify.trailers(log), [31])


    def test_a_dispatched_build_of_an_auto_add_branch_names_its_issue(self):
        # A run started by hand (workflow_dispatch) has no "before" commit: the commits of the branch that are
        # not on main count, which is the one commit tools/auto_add.py makes.
        with tempfile.TemporaryDirectory() as self.d:
            self.run_git("init", "-q", "-b", "main")
            self.commit("base")
            self.run_git("update-ref", "refs/remotes/origin/main", "HEAD")
            self.run_git("checkout", "-q", "-b", "test/auto-1532-00c1")
            tip = self.commit("Razer Viper V3 Pro (Wireless) (1532:00c1), added automatically\n\nTest: #85")
            cwd = os.getcwd()
            os.chdir(self.d)
            self.addCleanup(os.chdir, cwd)
            with mock.patch.object(notify, "git", lambda *a: self.run_git(*a) + "\n"):
                log = notify.new_commits(tip, "origin/main", "", "test/auto-1532-00c1")
            self.assertEqual(notify.trailers(log), [85])


class Testbuild(unittest.TestCase):
    def run_tb(self, comments, branch="test/x", log="Test: #30"):
        calls = []
        with mock.patch.object(notify, "git", lambda *a: "2026-01-01T00:00:00+00:00" if a[0] == "show" else S2), \
             mock.patch.object(notify, "new_commits", lambda *a: log), \
             mock.patch.object(notify, "open_issue", lambda n: True), \
             mock.patch.object(notify, "api", lambda p: comments), \
             mock.patch.object(notify, "gh", lambda *a: calls.append(a) or ""):
            notify.testbuild(S2, "https://x/actions/runs/9", "9", "origin/main", True, "", branch)
        return calls

    def test_posts_then_deletes_own_same_branch_and_legacy(self):
        mk = notify.TESTBUILD_MARK
        comments = [
            comment(1, "github-actions[bot]", f"{mk}:test/x:{S1} -->"),
            comment(2, "github-actions[bot]", f"{mk}:{S1} -->"),
            comment(3, "github-actions[bot]", f"{mk}:test/other:{S1} -->"),
            comment(4, "someone", f"{mk}:test/x:{S1} -->"),
            comment(5, "github-actions[bot]", "hello"),
        ]
        calls = self.run_tb(comments)
        self.assertEqual(calls[0][:3], ("issue", "comment", "30"))
        self.assertIn(f"{mk}:test/x:{S2} -->", calls[0][-1])
        self.assertEqual([c[-1].split("/")[-1] for c in calls[1:]], ["1", "2"])

    def test_same_run_is_not_repeated(self):
        self.assertEqual(self.run_tb([comment(1, "github-actions[bot]", "https://x/actions/runs/9")]), [])



class ReopenAfterNoAnswer(unittest.TestCase):
    """An issue tools/remind.py closed for want of an answer opens again when its author answers."""

    def run_waiting(self, state, labels, who):
        import json as _json
        import tempfile
        from unittest import mock

        event = {
            "issue": {"number": 7, "state": state, "user": {"login": "tester"}, "labels": [{"name": n} for n in labels]},
            "comment": {"user": {"login": who, "type": "User"}, "body": "Here it is."},
        }
        with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as f:
            _json.dump(event, f)
        calls = []
        with mock.patch.object(notify, "gh", side_effect=lambda *a: calls.append(a) or ""):
            notify.waiting(f.name, True)
        return calls

    def test_the_author_s_answer_reopens_it(self):
        calls = self.run_waiting("closed", ["device request", "no answer", "waiting for reporter"], "tester")
        self.assertEqual(calls[0], ("issue", "reopen", "7", "--repo", notify.REPO))
        self.assertIn("no answer", calls[1])
        self.assertIn("waiting for reporter", calls[1])

    def test_nobody_else_and_no_other_closed_issue(self):
        self.assertFalse(any(c[:2] == ("issue", "reopen") for c in self.run_waiting("closed", ["no answer"], "someone")))
        self.assertFalse(any(c[:2] == ("issue", "reopen") for c in self.run_waiting("closed", ["device request"], "tester")))

if __name__ == "__main__":
    unittest.main()
