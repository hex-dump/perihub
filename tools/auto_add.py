#!/usr/bin/env python3
"""Builds the test branch of a device that can be added without a person: the id is written in the allow list
tools/auto-add.json, the generator of its table is run on the upstream source at the commit the table
already names, `cargo xtask gen` writes the rest, the checks run, and everything is committed on
test/auto-VVVV-PPPP with a "Test: #N" line. Only that: pushing the branch, starting the test build and
opening the draft pull request are the auto-add workflow's (.github/workflows/auto-add.yml).

  auto_add.py USB_ID ISSUE --src SOURCE_DIR [--skip-checks] [--dry-run [--keep]]
      prints {"branch", "name", "usb", "commit"} as JSON on stdout when a branch was made, and nothing
      (a note on stderr) when the device is known already; anything else it cannot do exits 1.
      --src is a folder the upstream source is fetched into, at the pinned commit (made if it is not there).
      --dry-run works in a throwaway git worktree of HEAD, so the checkout is left as it is, and removes it
      and its branch afterwards unless --keep (the worktree is then printed on stderr).

Which ids this can add is GENERATORS: the (project, vendor) pairs whose table is written by a script that
reads facts from the project's source. tools/triage.py keeps the same pairs (MECHANICAL; test_triage.py
checks they agree). Today it is OpenRazer's mice (1532): at the pinned commit every mouse is in the table
except the receivers, docks and wireless dongles the generator leaves out, which the allow list lets through.
An id the table has not got because upstream added it after the pinned commit is not added: bumping the
pin brings every new id and every changed fact, which a person should read. The same goes for what a
generator does not produce at all (laptops, keyboards, mice with no command PeriHub knows): the script
fails and the workflow hands the issue to a person. Nothing here touches main: the branch starts at HEAD.

The USB id and the issue number are checked for their shape, and are only passed as arguments, never to a shell.
"""

import argparse
import json
import re
import shutil
import subprocess
import sys
import tempfile
from difflib import unified_diff
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ALLOW = "tools/auto-add.json"
DEVICES = "site/devices.json"
BOT = ("github-actions[bot]", "41898282+github-actions[bot]@users.noreply.github.com")
# (project, vendor id) -> how its table is made. "script" is run as: script SOURCE COMMIT > table.
GENERATORS = {
    ("openrazer", "1532"): {
        "url": "https://github.com/openrazer/openrazer",
        "script": "tools/openrazer_facts.py",
        "table": "crates/perihub-core/src/peripheral/razer_models.rs",
    }
}
# What a table the generator rewrote may lose: the header lines with the commit, and the entries of LEFT_OUT.
LEFT_OUT_LINE = re.compile(r'^\s+"[A-Z0-9_]+ \(.*\)",$')
CHANGE_LIMITS = (".github/", "CHANGELOG.md")


class Fail(Exception):
    """Something this cannot do: a person takes over."""


class Skip(Exception):
    """Nothing to do."""


def valid_usb(usb: str) -> bool:
    return bool(re.fullmatch(r"[0-9a-f]{4}:[0-9a-f]{4}", usb))


def branch_name(usb: str) -> str:
    return "test/auto-" + usb.replace(":", "-")


def pinned_commit(table: str) -> str | None:
    """The upstream commit a generated table says it was made from (its header names it in full)."""
    m = re.search(r"\bcommit ([0-9a-f]{40})\b", "\n".join(table.splitlines()[:6]))
    return m.group(1) if m else None


def with_allowed(text: str, usb: str) -> str:
    """The allow list with this id in it, once, in order."""
    data = json.loads(text) if text.strip() else {}
    data["allow"] = sorted({*data.get("allow", []), usb})
    return json.dumps(data) + "\n"


MODEL = re.compile(r"^    Model \{\n.*?^    \},\n", re.S | re.M)


def product_ids(block: str) -> set[int]:
    return {int(p, 16) for line in re.findall(r"product_ids: &\[([^\]]*)\]", block) for p in line.split(",") if p.strip()}


def check_diff(old: str, new: str, usb: str) -> str | None:
    """Why the new table is not "the old one plus this device", or None when it is: every model of the old one
    is there unchanged, exactly one model is new and it is this one, and of the rest of the file (the header and
    LEFT_OUT) only the commit of the header and the entries of LEFT_OUT may go. A changed fact of another device,
    or a second new device, is for a person to read."""
    pid = int(usb.split(":")[1], 16)
    old_models, new_models = MODEL.findall(old), MODEL.findall(new)
    if any(pid in product_ids(m) for m in old_models):
        return f"{usb} is in the table already"
    added = [m for m in new_models if m not in old_models]
    if not any(pid in product_ids(m) for m in new_models):
        return f"the generator does not produce {usb}: it is a device this table leaves out (see its LEFT_OUT), or one upstream added after the pinned commit"
    missing = [m for m in old_models if m not in new_models]
    if missing or len(added) != 1 or product_ids(added[0]) != {pid} or len(new_models) != len(old_models) + 1:
        return "the new table does not differ from the old one by this device alone (another model is new or changed)"
    rest_old, rest_new = MODEL.sub("", old), MODEL.sub("", new)
    for line in unified_diff(rest_old.splitlines(), rest_new.splitlines(), lineterm="", n=0):
        if line.startswith("-") and not line.startswith("---"):
            body = line[1:]
            if not (body.startswith("//!") and "commit" in body) and not LEFT_OUT_LINE.match(body):
                return f"the new table changes something else: {body.strip()[:80]}"
    return None


def commit_message(name: str, usb: str, issue: str, project: str, commit: str) -> str:
    return (
        f"{name} ({usb}), added automatically from {project} facts\n\n"
        f"Made by the auto-add workflow (tools/auto_add.py): the id is in {ALLOW} and the table was written by its\n"
        f"generator from {project} at {commit[:12]}, the commit the table already names; cargo xtask gen wrote the rest.\n"
        "Not tried on hardware: a tester's clean report confirms it.\n\n"
        f"Test: #{issue}\n"
    )


def run(cmd: list[str], cwd: Path, check=True, stdout=None) -> subprocess.CompletedProcess:
    print("+", " ".join(cmd), file=sys.stderr)
    done = subprocess.run(cmd, cwd=cwd, text=True, stdout=stdout, stderr=None if stdout is None else subprocess.PIPE)
    if check and done.returncode:
        raise Fail(f"{' '.join(cmd[:3])} failed (exit {done.returncode})" + (f": {done.stderr.strip()[-300:]}" if done.stderr else ""))
    return done


def fetch_source(src: Path, url: str, commit: str) -> None:
    """The upstream source at exactly this commit in src (a shallow fetch of the one commit)."""
    if (src / ".git").exists():
        head = subprocess.run(["git", "rev-parse", "HEAD"], cwd=src, capture_output=True, text=True).stdout.strip()
        if head == commit:
            return
    else:
        src.mkdir(parents=True, exist_ok=True)
        run(["git", "init", "-q", "."], src)
        run(["git", "remote", "add", "origin", url], src)
    run(["git", "fetch", "-q", "--depth", "1", "origin", commit], src)
    run(["git", "checkout", "-q", "--detach", "FETCH_HEAD"], src)
    head = subprocess.run(["git", "rev-parse", "HEAD"], cwd=src, capture_output=True, text=True).stdout.strip()
    if head != commit:
        raise Fail(f"the source is at {head[:12]}, not at {commit[:12]}")


def checks(repo: Path) -> None:
    """What CI and the confirm workflow check, for what this changes (the Rust tables and the generated files)."""
    for cmd in (
        ["cargo", "fmt", "--all", "--check"],
        ["cargo", "clippy", "-p", "perihub-core", "-p", "xtask", "--all-targets", "--", "-D", "warnings"],
        ["cargo", "test", "-p", "perihub-core", "-p", "xtask"],
        ["cargo", "xtask", "check"],
        [sys.executable, "-I", "-m", "unittest", "discover", "-s", "tools", "-p", "test_*.py"],
        [sys.executable, "-I", "tools/release.py", "lint"],
    ):
        run(cmd, repo)


def build(usb: str, issue: str, src: Path, repo: Path, skip_checks: bool = False) -> dict:
    if not valid_usb(usb) or not re.fullmatch(r"[0-9]{1,6}", issue):
        raise Fail("the USB id is not vvvv:pppp, or the issue is not a number")
    vendor = usb.split(":")[0]
    found = [(p, g) for (p, v), g in GENERATORS.items() if v == vendor]
    if not found:
        raise Fail(f"no generator writes a table for vendor {vendor}")
    project, gen = found[0]
    devices = json.loads((repo / DEVICES).read_text())["devices"]
    if any(usb in d["usb_ids"] for d in devices):
        raise Skip(f"{usb} is in {DEVICES} already")
    if run(["git", "status", "--porcelain", "--untracked-files=no"], repo, stdout=subprocess.PIPE).stdout.strip():
        raise Fail("the checkout has changes: this starts from a clean one")
    table = repo / gen["table"]
    old = table.read_text()
    commit = pinned_commit(old)
    if not commit:
        raise Fail(f"{gen['table']} does not name its commit")
    fetch_source(src, gen["url"], commit)

    allow = repo / ALLOW
    allow.write_text(with_allowed(allow.read_text() if allow.exists() else "", usb))
    made = run([sys.executable, "-I", gen["script"], str(src), commit], repo, stdout=subprocess.PIPE)
    why = check_diff(old, made.stdout, usb)
    if why:
        raise Fail(why)
    table.write_text(made.stdout)
    run(["cargo", "xtask", "gen"], repo)

    after = json.loads((repo / DEVICES).read_text())["devices"]
    named = [d for d in after if usb in d["usb_ids"]]
    if len(named) != 1:
        raise Fail(f"{DEVICES} has {len(named)} devices with {usb} after cargo xtask gen")
    name = f"{named[0]['brand']} {named[0]['name']}"
    changed = run(["git", "diff", "--name-only"], repo, stdout=subprocess.PIPE).stdout.split()
    for path in changed:
        if path.startswith(CHANGE_LIMITS) or (path.startswith("crates/") and path != gen["table"]):
            raise Fail(f"the change would touch {path}")
    if not skip_checks:
        checks(repo)

    branch = branch_name(usb)
    run(["git", "switch", "-q", "-c", branch], repo)
    run(["git", "add", "-A"], repo)
    run(["git", "-c", f"user.name={BOT[0]}", "-c", f"user.email={BOT[1]}", "commit", "-q", "-m",
         commit_message(name, usb, issue, project, commit)], repo)
    sha = run(["git", "rev-parse", "HEAD"], repo, stdout=subprocess.PIPE).stdout.strip()
    return {"branch": branch, "name": name, "usb": usb, "commit": sha}


def main() -> None:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("usb")
    p.add_argument("issue")
    p.add_argument("--src", required=True, type=Path)
    p.add_argument("--skip-checks", action="store_true")
    p.add_argument("--dry-run", action="store_true")
    p.add_argument("--keep", action="store_true")
    args = p.parse_args()
    repo, work = ROOT, None
    try:
        if args.dry_run:
            work = Path(tempfile.mkdtemp(prefix="auto-add-")) / "repo"
            run(["git", "worktree", "add", "-q", "--detach", str(work), "HEAD"], ROOT)
            repo = work
        result = build(args.usb, args.issue, args.src.resolve(), repo, args.skip_checks)
        if args.dry_run:
            stat = run(["git", "show", "--stat", "--format=%s%n%n%b", "HEAD"], repo, stdout=subprocess.PIPE).stdout
            print(stat, file=sys.stderr)
        print(json.dumps(result))
    except Skip as e:
        print(f"auto-add: {e}: nothing to do", file=sys.stderr)
    except Fail as e:
        print(f"auto-add: {e}", file=sys.stderr)
        sys.exit(1)
    finally:
        if work and not args.keep:
            subprocess.run(["git", "worktree", "remove", "--force", str(work)], cwd=ROOT, capture_output=True)
            subprocess.run(["git", "branch", "-D", branch_name(args.usb)], cwd=ROOT, capture_output=True)
            shutil.rmtree(work.parent, ignore_errors=True)
        elif work:
            print(f"auto-add: kept the worktree {work}", file=sys.stderr)


if __name__ == "__main__":
    main()
