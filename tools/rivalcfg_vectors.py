#!/usr/bin/env python3
"""Extracts the HID reports rivalcfg's own tests expect into crates/perihub-core/tests/fixtures/rivalcfg_vectors.json.

rivalcfg (https://github.com/flozz/rivalcfg) is under the WTFPL. Its tests are read with Python's
`ast` module or as text, never run. PeriHub's SteelSeries driver must send the same bytes.

Usage: python3 -I tools/rivalcfg_vectors.py RIVALCFG_CHECKOUT > crates/perihub-core/tests/fixtures/rivalcfg_vectors.json
"""

import ast
import importlib.util
import json
import sys
from pathlib import Path

spec = importlib.util.spec_from_file_location("gen", Path(__file__).with_name("rivalcfg_to_rust.py"))
gen = importlib.util.module_from_spec(spec)
spec.loader.exec_module(gen)


def profiles(devices):
    dpi = {f"{p.stem}.choices": gen.load_module(p, {})["choices"] for p in (devices / "dpi").glob("*.py") if p.stem != "__init__"}
    out, pending = {}, []
    for path in sorted(devices.glob("*.py")):
        if path.stem == "__init__":
            continue
        source = path.read_text()
        try:
            names = gen.load_module(path, {**dpi, **{f"{s}.profile": p for s, p in out.items()}})
        except (KeyError, TypeError, ValueError):
            pending.append(path)
            continue
        if "_patch_command" in source:
            continue
        out[path.stem] = names["profile"]
    for path in pending:
        if "_patch_command" not in path.read_text():
            out[path.stem] = gen.load_module(path, {**dpi, **{f"{s}.profile": p for s, p in out.items()}})["profile"]
    return out


def wireless_pids(devices):
    """Wireless profiles: their product ids and the wired profile they patch."""
    out = {}
    for path in sorted(devices.glob("*.py")):
        source = path.read_text()
        if "_patch_command" not in source:
            continue
        base = next(n.names[0].name for n in ast.parse(source).body if isinstance(n, ast.ImportFrom) and n.module is None)
        names = gen.load_module(path, {})
        out[path.stem] = ([m["product_id"] for m in names["profile"]["models"]], base)
    return out


def main():
    root = Path(sys.argv[1])
    devices = root / "rivalcfg" / "devices"
    known = profiles(devices)
    wireless = wireless_pids(devices)
    pid = {stem: p["models"][0]["product_id"] for stem, p in known.items()}
    pid.update({stem: ids[0] for stem, (ids, _) in wireless.items()})
    settings_of = {stem: p["settings"] for stem, p in known.items()}
    settings_of.update({stem: known[base]["settings"] for stem, (_, base) in wireless.items()})
    vectors = []

    for test in sorted((root / "test" / "devices" / "old_specs").glob("test_*.py")):
        source = test.read_text()
        tree = ast.parse(source)
        stem = next(
            (a.name for n in tree.body if isinstance(n, ast.ImportFrom) and n.module == "rivalcfg.devices" for a in n.names),
            None,
        )
        if stem not in pid:
            continue
        for fn in ast.walk(tree):
            if not (isinstance(fn, ast.FunctionDef) and fn.name.startswith("test_set_")):
                continue
            setting = fn.name.removeprefix("test_set_")
            for dec in fn.decorator_list:
                if not (isinstance(dec, ast.Call) and len(dec.args) == 2):
                    continue
                if not (isinstance(dec.args[0], ast.Constant) and dec.args[0].value == "value,expected_hid_report"):
                    continue
                for case in dec.args[1].elts:
                    try:
                        value = ast.literal_eval(case.elts[0])
                        report = ast.literal_eval(case.elts[1])
                    except ValueError:
                        continue
                    if isinstance(report, bytes):
                        vectors.append({"product_id": pid[stem], "setting": setting, "value": value, "report": list(report)})

    for spec_file in sorted((root / "test" / "devices" / "specs").glob("*.txt")):
        stem = spec_file.stem
        options = {cli: key for key, info in settings_of[stem].items() for cli in info.get("cli", [])}
        lines = spec_file.read_text().splitlines()
        i = 0
        while i < len(lines):
            line = lines[i].strip()
            i += 1
            if not line.startswith("--"):
                continue
            option, _, value = line.partition(" ")
            if option not in options or not value:
                continue
            # The first report after the command line: "< 02 00 |" then its data lines.
            while i < len(lines) and not lines[i].startswith("<"):
                i += 1
            if i >= len(lines):
                break
            head_part, _, rest = lines[i].partition("|")
            head = head_part.strip("< ").split()
            data = rest.split()
            i += 1
            while i < len(lines) and lines[i].startswith("  "):
                data += lines[i].split()
                i += 1
            report = [int(b, 16) for b in head + data]
            vectors.append({"product_id": pid[stem], "setting": options[option], "value": value, "report": report})

    json.dump(vectors, sys.stdout, indent=0)
    print()


if __name__ == "__main__":
    main()
