#!/usr/bin/env python3
"""Builds the test branch of a device that can be added without a person, in one of two ways, then runs
`cargo xtask gen` and the checks, adds the line under "New devices" in CHANGELOG.md and commits everything on
test/auto-VVVV-PPPP with a "Test: #N" line:

  - OpenRazer's mice (GENERATORS): the id is written in the allow list tools/auto-add.json and the generator of its
    table is run on the upstream source at the commit the table already names.
  - A sibling id of a family PeriHub already has a driver for (tools/platforms.py): the device request (--request, the
    issue as the GitHub API gives it) shows that family's configuration collection with the same reports, so the id
    is written into the family's table in the driver, read only, named after the request's Device model (cleaned,
    or the family's own name for an id when it says nothing). The request is checked again here, whatever started it.

Only that: pushing the branch, starting the test build and opening the pull request (a normal one, for the maintainer
to merge after CI) are the auto-add workflow's (.github/workflows/auto-add.yml).

  auto_add.py USB_ID ISSUE (--src SOURCE_DIR | --request ISSUE.json [--family KEY]) [--skip-checks] [--dry-run [--keep]]
      prints {"branch", "name", "usb", "commit"} as JSON on stdout when a branch was made, and nothing
      (a note on stderr) when the device is known already; anything else it cannot do exits 1.
      --src is a folder the upstream source is fetched into, at the pinned commit (made if it is not there).
      --family lets a person name the family when the request shows its collection but no report descriptor (an
      older PeriHub): the collection must still match.
      --dry-run works in a throwaway git worktree of HEAD, so the checkout is left as it is, and removes it
      and its branch afterwards unless --keep (the worktree is then printed on stderr).

Which ids the first way can add is GENERATORS: the (project, vendor) pairs whose table is written by a script that
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
import html
import json
import re
import shutil
import subprocess
import sys
import tempfile
from difflib import unified_diff
from pathlib import Path

# Run as python3 -I (the workflow does): the folder of this script is not on the path by itself.
sys.path.insert(0, str(Path(__file__).resolve().parent))
import platforms  # noqa: E402
import triage  # noqa: E402

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
CHANGE_LIMITS = (".github/", "tools/")
CHANGELOG = "CHANGELOG.md"
ISSUES = "https://github.com/hex-dump/perihub/issues/"


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


# Words that say nothing about which device it is: Windows' device class in front ("(Standard system devices)"), the
# chip maker's generic names, and what every keyboard or mouse is.
GENERIC = re.compile(
    r"\b(usb|hid|device|devices|keyboard|mouse|gaming|wireless|wired|receiver|2\.4g|composite|standard|system|"
    r"generic|sonix|telink|artery|rgb|evision)\b",
    re.I,
)


def model_name(text: str, sibling: platforms.Sibling, usb: str, taken: set[str]) -> str:
    """The name of the new model: the request's Device model without Windows' device class in front, quotes and odd
    characters, at most 40 characters; the family's name for an id (sibling.fallback) when it says nothing, or is a
    name the brand has already."""
    v, p = usb.split(":")
    fallback = sibling.fallback.format(v=v, p=p)
    s = re.sub(r"^\s*\([^)]*\)\s*", "", text or "")
    s = re.sub(r"[^A-Za-z0-9 +._/-]", " ", s)
    s = " ".join(s.split())
    if s.lower().startswith(sibling.brand.lower() + " "):
        s = s[len(sibling.brand) + 1 :]
    s = s[:40].strip(" -._/")
    if not re.search(r"[A-Za-z]", GENERIC.sub("", s)) or s in taken:
        return fallback
    return s


def request_of(path: Path) -> tuple[str | None, str, str]:
    """The USB id, the Device model and the "What PeriHub saw" block of a device request (the issue as JSON)."""
    data = json.loads(path.read_text())
    body = data.get("body") or ""
    answers = triage.fields(body)
    saw = answers.get("What PeriHub saw", "") or body
    # The id the triage answered about: PeriHub's own report first, then the form's field (triage.request_ids).
    reported, typed = triage.request_ids(answers, saw)
    usb = (reported or typed or [triage.usb_id(saw)])[0]
    device = re.search(r"^Device: (.+)$", saw, re.M)
    model = answers.get("Device model", "") or (device.group(1) if device else "")
    return usb, model.strip(), saw


def with_entry(source: str, entry: str) -> str:
    """The driver with one more model at the end of its MODELS table."""
    anchor = "pub const MODELS: &[Model] = &["
    start = source.find(anchor)
    if start < 0:
        raise Fail("the driver has no MODELS table this can add to")
    # The "]" that closes the table (the product ids inside it have brackets of their own); cargo fmt lays it out after.
    depth, i = 1, start + len(anchor)
    while depth and i < len(source):
        depth += {"[": 1, "]": -1}.get(source[i], 0)
        i += 1
    if depth:
        raise Fail("the MODELS table of the driver does not end")
    end = i - 1
    head = source[:end].rstrip()
    comma = "" if head.endswith(",") else ","
    return head + comma + "\n" + entry + source[end:]


def with_changelog_line(text: str, line: str) -> str:
    """CHANGELOG.md with a line at the end of "### New devices" under "## Unreleased" (the section is made, first,
    when there is none)."""
    start = text.find("\n## Unreleased\n")
    if start < 0:
        raise Fail("CHANGELOG.md has no Unreleased section")
    start += 1
    end = text.find("\n## ", start + 1)
    end = len(text) if end < 0 else end + 1
    section = text[start:end]
    heading = section.find("\n### New devices\n")
    if heading >= 0:
        rest = heading + len("\n### New devices\n")
        stop = section.find("\n\n", rest)
        stop = len(section.rstrip("\n")) if stop < 0 else stop
        section = section[:stop] + "\n" + line + section[stop:]
    else:
        first = section.find("\n### ")
        block = f"### New devices\n{line}\n\n"
        section = section[: first + 1] + block + section[first + 1 :] if first >= 0 else section.rstrip("\n") + "\n\n" + block
    return text[:start] + section + text[end:]


def sibling_line(brand: str, name: str, usb: str, issue: str, platform: platforms.Platform) -> str:
    linux = f" {platform.sibling.linux}" if platform.sibling.linux else ""
    return (
        f"- **{brand} {name} (`{usb}`), read only** ([#{issue}]({ISSUES}{issue})): its configuration collection "
        f"({platforms.signature(platform)}) and its reports are the ones of the {platform.name} PeriHub already reads, so "
        f"it was added automatically from its device request with the same reads (`{Path(platform.driver).name}`).{linux} "
        "Nobody has tried it with PeriHub yet: **Test my device** sends a report that moves it up."
    )


def generated_line(name: str, usb: str, issue: str, project: str) -> str:
    return (
        f"- **{name} (`{usb}`), experimental** ([#{issue}]({ISSUES}{issue})): added automatically from facts found in "
        f"{project} (no code copied), written by `{GENERATORS[(project, usb.split(':')[0])]['script']}`. Nobody has tried "
        "it with PeriHub yet: **Test my device** sends a report that moves it up."
    )


def sibling_message(brand: str, name: str, usb: str, issue: str, platform: platforms.Platform) -> str:
    return (
        f"{brand} {name} ({usb}): read only, added automatically\n\n"
        f"Made by the auto-add workflow (tools/auto_add.py): the device request shows the configuration collection of the\n"
        f"{platform.name} ({platforms.signature(platform)}), so the id was added to the table of\n"
        f"{platform.driver}, read only; cargo xtask gen wrote the rest.\n"
        "Not tried on hardware: a tester's clean report makes it supported.\n\n"
        f"Test: #{issue}\n"
    )


def commit_message(name: str, usb: str, issue: str, project: str, commit: str) -> str:
    return (
        f"{name} ({usb}), added automatically from {project} facts\n\n"
        f"Made by the auto-add workflow (tools/auto_add.py): the id is in {ALLOW} and the table was written by its\n"
        f"generator from {project} at {commit[:12]}, the commit the table already names; cargo xtask gen wrote the rest.\n"
        "Not tried on hardware: it ships as experimental, and a tester's clean report makes it supported.\n\n"
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


def build(
    usb: str,
    issue: str,
    src: Path | None,
    repo: Path,
    skip_checks: bool = False,
    request: Path | None = None,
    family: str | None = None,
) -> dict:
    if not valid_usb(usb) or not re.fullmatch(r"[0-9]{1,6}", issue):
        raise Fail("the USB id is not vvvv:pppp, or the issue is not a number")
    vendor = usb.split(":")[0]
    devices = json.loads((repo / DEVICES).read_text())["devices"]
    if any(usb in d["usb_ids"] for d in devices):
        raise Skip(f"{usb} is in {DEVICES} already")
    if run(["git", "status", "--porcelain", "--untracked-files=no"], repo, stdout=subprocess.PIPE).stdout.strip():
        raise Fail("the checkout has changes: this starts from a clean one")
    found = [(p, g) for (p, v), g in GENERATORS.items() if v == vendor]
    if found:
        if src is None:
            raise Fail("--src is needed for an id a generator writes")
        return build_generated(usb, issue, src, repo, skip_checks, *found[0])
    if request is None:
        raise Fail(f"no generator writes a table for vendor {vendor}, and no --request to recognize its family from")
    return build_sibling(usb, issue, request, repo, skip_checks, family, devices)


def site_rows(repo: Path) -> set[str]:
    """The model names the rows of site/index.html's device list hold (data-models, which cargo xtask gen fills from
    each row's data-catalog): site/build.py stops the website, and CI, when a model has no row."""
    text = (repo / "site/index.html").read_text()
    return {html.unescape(n) for m in re.findall(r'data-models="([^"]*)"', text) for n in m.split("|") if n}


def build_sibling(
    usb: str, issue: str, request: Path, repo: Path, skip_checks: bool, family: str | None, devices: list[dict]
) -> dict:
    """The id written into the table of the family the request's descriptors show (tools/platforms.py), read only."""
    asked, model, saw = request_of(request)
    if asked != usb:
        raise Fail(f"the request is for {asked}, not {usb}")
    if family:
        platform = platforms.BY_KEY.get(family)
        if platform is None or platform.sibling is None:
            raise Fail(f"{family} is not a family whose table takes a new id")
        if not platforms.level(platform, usb, saw):
            raise Fail(f"the request does not show the collection of {platform.name}")
    else:
        match = platforms.recognize(usb, saw)
        if not match or not match.auto:
            raise Fail("the request does not show, with its reports, the collection of one family whose table takes a new id")
        platform = match.platform
    sibling = platform.sibling
    params = sibling.params.get(usb.split(":")[0], sibling.params.get(""))
    if params is None:
        raise Fail(f"{platform.key} has no Params for vendor {usb.split(':')[0]}")
    taken = {d["name"] for d in devices if d["brand"] == sibling.brand}
    name = model_name(model, sibling, usb, taken)
    v, p = usb.split(":")
    entry = sibling.entry.format(vendor=v.upper(), product=p.upper(), name=name, params=params)

    driver = repo / platform.driver
    driver.write_text(with_entry(driver.read_text(), entry))
    run(["cargo", "fmt", "--all"], repo)
    run(["cargo", "xtask", "gen"], repo)
    after = json.loads((repo / DEVICES).read_text())["devices"]
    named = [d for d in after if usb in d["usb_ids"]]
    if len(named) != 1 or named[0]["family"] != platform.key or named[0]["level"] != "read_only":
        raise Fail(f"{DEVICES} does not have {usb} once, read only, in {platform.key}, after cargo xtask gen")
    brand, name = named[0]["brand"], named[0]["name"]
    if name not in site_rows(repo):
        raise Fail(f"no row of site/index.html takes {brand} {name} (a row with data-catalog for {brand} is needed)")
    log = repo / CHANGELOG
    log.write_text(with_changelog_line(log.read_text(), sibling_line(brand, name, usb, issue, platform)))
    changed = run(["git", "diff", "--name-only"], repo, stdout=subprocess.PIPE).stdout.split()
    for path in changed:
        if path.startswith(CHANGE_LIMITS) or (path.startswith("crates/") and path != platform.driver):
            raise Fail(f"the change would touch {path}")
    if not skip_checks:
        checks(repo)
    return commit(repo, usb, sibling_message(brand, name, usb, issue, platform), f"{brand} {name}")


def commit(repo: Path, usb: str, message: str, name: str) -> dict:
    branch = branch_name(usb)
    run(["git", "switch", "-q", "-c", branch], repo)
    run(["git", "add", "-A"], repo)
    run(["git", "-c", f"user.name={BOT[0]}", "-c", f"user.email={BOT[1]}", "commit", "-q", "-m", message], repo)
    sha = run(["git", "rev-parse", "HEAD"], repo, stdout=subprocess.PIPE).stdout.strip()
    return {"branch": branch, "name": name, "usb": usb, "commit": sha}


def build_generated(usb: str, issue: str, src: Path, repo: Path, skip_checks: bool, key: tuple, gen: dict) -> dict:
    """The id written by the generator of its table (OpenRazer's mice), from the source at the pinned commit."""
    project = key[0]
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
    log = repo / CHANGELOG
    log.write_text(with_changelog_line(log.read_text(), generated_line(name, usb, issue, project)))
    changed = run(["git", "diff", "--name-only"], repo, stdout=subprocess.PIPE).stdout.split()
    for path in changed:
        if (path.startswith(CHANGE_LIMITS) and path != ALLOW) or (path.startswith("crates/") and path != gen["table"]):
            raise Fail(f"the change would touch {path}")
    if not skip_checks:
        checks(repo)
    return commit(repo, usb, commit_message(name, usb, issue, project, commit), name)


def main() -> None:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("usb")
    p.add_argument("issue")
    p.add_argument("--src", type=Path)
    p.add_argument("--request", type=Path)
    p.add_argument("--family")
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
        result = build(
            args.usb,
            args.issue,
            args.src.resolve() if args.src else None,
            repo,
            args.skip_checks,
            args.request.resolve() if args.request else None,
            args.family,
        )
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
