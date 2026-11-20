#!/usr/bin/env python3
"""Tests of tools/remind.py: python3 -I -m unittest discover -s tools -p "test_*.py"
"""

import sys
import unittest
from datetime import datetime, timedelta, timezone
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import remind  # noqa: E402

NOW = datetime(2026, 12, 1, tzinfo=timezone.utc)


def stamp(days_ago: float) -> str:
    return (NOW - timedelta(days=days_ago)).strftime("%Y-%m-%dT%H:%M:%SZ")


def issue(labels=("device request", "waiting for reporter"), author="tester", number=5, **extra):
    return {"number": number, "state": "open", "user": {"login": author}, "labels": [{"name": n} for n in labels], **extra}


def comment(who: str, days_ago: float, body: str) -> dict:
    return {"user": {"login": who}, "created_at": stamp(days_ago), "body": body}


QUESTION = "Which program changes the lighting?"
BOT = "github-actions[bot]"


def thread(asked: float, reminded: float | None = None) -> list[dict]:
    out = [comment("tester", asked + 1, "Here is my keyboard."), comment(remind.MAINTAINER, asked, QUESTION)]
    if reminded is not None:
        out.append(comment(BOT, reminded, remind.reminder(issue(), True)))
    return out


class Decide(unittest.TestCase):
    def test_the_reminder_after_a_week_then_the_close_a_week_after_it(self):
        self.assertIsNone(remind.decide(issue(), thread(6), NOW, set()))
        self.assertEqual(remind.decide(issue(), thread(7), NOW, set()), "remind")
        self.assertIsNone(remind.decide(issue(), thread(10, reminded=3), NOW, set()))
        self.assertEqual(remind.decide(issue(), thread(14, reminded=7), NOW, set()), "close")

    def test_never_closed_without_a_week_of_warning(self):
        # An old question reminded only yesterday (the reminder workflow was off): a week more.
        self.assertIsNone(remind.decide(issue(), thread(30, reminded=1), NOW, set()))
        self.assertEqual(remind.decide(issue(), thread(30, reminded=7), NOW, set()), "close")

    def test_a_reminder_that_did_not_warn_never_leads_to_a_close(self):
        quiet = thread(30) + [comment(BOT, 10, remind.reminder(issue(), False))]
        self.assertIsNone(remind.decide(issue(), quiet, NOW, set()))

    def test_an_answer_or_a_new_question_starts_again(self):
        answered = thread(20, reminded=10) + [comment("tester", 9, "It is the Skytech app.")]
        self.assertIsNone(remind.decide(issue(), answered, NOW, set()))
        # Someone else with the same device answered: not the maintainer's turn to wait.
        other = thread(20, reminded=10) + [comment("someone", 9, "Mine too.")]
        self.assertIsNone(remind.decide(issue(), other, NOW, set()))
        asked_again = thread(20, reminded=10) + [comment(remind.MAINTAINER, 2, "And which version?")]
        self.assertIsNone(remind.decide(issue(), asked_again, NOW, set()))

    def test_a_note_that_asks_nothing_waits_for_no_one(self):
        note = [comment(remind.MAINTAINER, 30, "Thanks, noted.")]
        self.assertIsNone(remind.decide(issue(), note, NOW, set()))

    def test_only_the_safe_cases_are_closed(self):
        old = thread(30, reminded=10)
        for labels in [
            ("device request", "waiting for reporter", "ready to add"),  # our work
            ("device request", "waiting for reporter", "needs maintainer"),
            ("device request", "waiting for reporter", "keep open"),
            ("bug", "waiting for reporter"),
            ("device request",),  # the question did not put the label on: not sure it waits
        ]:
            with self.subTest(labels=labels):
                self.assertNotEqual(remind.decide(issue(labels=labels), old, NOW, set()), "close")
        self.assertEqual(
            remind.decide(issue(labels=("test report", "waiting for reporter", "mouse", "brand: Razer")), old, NOW, set()), "close"
        )

    def test_never_an_issue_a_test_branch_or_a_pull_request_waits_for(self):
        self.assertIsNone(remind.decide(issue(number=85), thread(30, reminded=10), NOW, {85}))

    def test_never_the_maintainer_s_own_a_locked_or_a_closed_issue(self):
        old = thread(30, reminded=10)
        self.assertIsNone(remind.decide(issue(author=remind.MAINTAINER), old, NOW, set()))
        self.assertIsNone(remind.decide(issue(locked=True), old, NOW, set()))
        self.assertIsNone(remind.decide(issue(state="closed"), old, NOW, set()))

    def test_the_reminder_says_it_will_close_only_when_it_will(self):
        self.assertIn("closed for now", remind.reminder(issue(), True))
        self.assertNotIn("closed", remind.reminder(issue(labels=("device request", "ready to add")), False))


if __name__ == "__main__":
    unittest.main()
