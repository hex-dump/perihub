#!/usr/bin/env python3
"""Generates crates/perihub-core/src/peripheral/steelseries_models.rs from rivalcfg's device profiles.

rivalcfg (https://github.com/flozz/rivalcfg) is under the WTFPL. Its profiles are read with Python's
`ast` module, never imported or run. Only the settings PeriHub supports are kept: choices, colours,
ranges and DPI presets. Buttons, gradients, reactive colours and separate X/Y DPI are left out.

Usage: python3 -I tools/rivalcfg_to_rust.py RIVALCFG_CHECKOUT COMMIT > crates/perihub-core/src/peripheral/steelseries_models.rs
where RIVALCFG_CHECKOUT holds rivalcfg/devices/*.py and rivalcfg/devices/dpi/*.py at COMMIT.
"""

import ast
import sys
from pathlib import Path

SUPPORTED = {"choice", "rgbcolor", "range", "range_choice", "multidpi_range_choice", "multidpi_range"}
WIRELESS_FLAG = 0b01000000
WIRELESS_READBACK = 64


class Opaque:
    """Something the profiles compute: a lambda or a call, kept as source text."""

    def __init__(self, text):
        self.text = text


def evaluate(node, names, source):
    if isinstance(node, ast.Constant):
        return node.value
    if isinstance(node, ast.Dict):
        out = {}
        for k, v in zip(node.keys, node.values):
            if k is None:  # {**other}
                out.update(evaluate(v, names, source))
            else:
                out[evaluate(k, names, source)] = evaluate(v, names, source)
        return out
    if isinstance(node, (ast.List, ast.Tuple)):
        out = []
        for e in node.elts:
            if isinstance(e, ast.Starred):  # *[0x00, 0x00, 0x00] * 2
                out.extend(evaluate(e.value, names, source))
            else:
                out.append(evaluate(e, names, source))
        return out
    if isinstance(node, ast.BinOp) and isinstance(node.op, ast.Mult):
        return evaluate(node.left, names, source) * evaluate(node.right, names, source)
    if isinstance(node, ast.UnaryOp) and isinstance(node.op, ast.USub):
        return -evaluate(node.operand, names, source)
    if isinstance(node, ast.BinOp) and isinstance(node.op, ast.BitOr):
        return evaluate(node.left, names, source) | evaluate(node.right, names, source)
    if isinstance(node, ast.Name):
        if node.id not in names:
            raise ValueError(f"unknown name {node.id}")
        return names[node.id]
    if isinstance(node, ast.Subscript):
        base = evaluate(node.value, names, source)
        if isinstance(base, Opaque):
            return Opaque(ast.get_source_segment(source, node))
        return base[evaluate(node.slice, names, source)]
    if isinstance(node, ast.Attribute):
        base = ast.get_source_segment(source, node)
        if base.startswith("usbhid."):
            return base.split(".", 1)[1]
        if base in names:
            return names[base]
        return Opaque(base)
    return Opaque(ast.get_source_segment(source, node))


def load_module(path, extra):
    source = path.read_text()
    tree = ast.parse(source)
    names = dict(extra)
    for stmt in tree.body:
        if isinstance(stmt, ast.Assign) and len(stmt.targets) == 1 and isinstance(stmt.targets[0], ast.Name):
            names[stmt.targets[0].id] = evaluate(stmt.value, names, source)
    return names


def rust_bytes(values):
    return "&[" + ", ".join(f"0x{v:02X}" for v in values) + "]"


def rust_str(text):
    return '"' + text.replace("\\", "\\\\").replace('"', '\\"') + '"'


def flatten(value):
    """rivalcfg's merge_bytes of one choice value: an int or a list of ints."""
    return value if isinstance(value, list) else [value]


def cmd(info, wireless):
    command = list(info["command"])
    if wireless:
        command[0] |= WIRELESS_FLAG
    feature = info["report_type"] == "HID_REPORT_TYPE_FEATURE"
    return f"Cmd {{ feature: {str(feature).lower()}, bytes: {rust_bytes(command)} }}"


def table(choices):
    items = sorted(choices.items())
    return "&[" + ", ".join(f"({k}, {v})" for k, v in items) + "]"


def setting(key, info, wireless):
    kind = info["value_type"]
    if kind == "choice":
        values = ", ".join(f"({rust_str(str(k))}, {rust_bytes(flatten(v))})" for k, v in info["choices"].items())
        body = f"Kind::Choice(&[{values}])"
    elif kind == "rgbcolor":
        body = "Kind::Color"
    elif kind == "range":
        body = f"Kind::Range {{ input: {tuple(info['input_range'])}, output: {tuple(info['output_range'])}, bytes: {info.get('range_length_byte', 1)} }}"
    elif kind == "range_choice":
        body = f"Kind::RangeChoice {{ input: {tuple(info['input_range'])}, choices: {table(info['output_choices'])}, bytes: {info.get('range_length_byte', 1)} }}"
    elif kind in ("multidpi_range_choice", "multidpi_range"):
        encoding = (
            f"DpiEncoding::Choices({table(info['output_choices'])})"
            if kind == "multidpi_range_choice"
            else f"DpiEncoding::Range{tuple(info['output_range'])}"
        )
        body = (
            f"Kind::Dpi {{ input: {tuple(info['input_range'])}, encoding: {encoding}, dpi_bytes: {info['dpi_length_byte']}, "
            f"first_preset: {info['first_preset']}, max_presets: {info['max_preset_count']}, "
            f"count_flag: {str(info.get('count_mode') == 'flag').lower()} }}"
        )
    else:
        raise ValueError(kind)
    return (
        f"Setting {{ id: {rust_str(key)}, label: {rust_str(info['label'])}, cmd: {cmd(info, wireless)}, "
        f"suffix: {rust_bytes(info.get('command_suffix', []))}, packet: {info.get('packet_length', 0)}, kind: {body} }}"
    )


def battery(info, wireless):
    if not info:
        return "None"
    level = info["level"].text if isinstance(info["level"], Opaque) else ""
    if "data[1] & ~_BATTERY_CHARGING_FLAG" in level and "* 5" in level:
        fmt = "BatteryFormat::FlagAndFifths"
    elif level.replace(" ", "") == "lambdadata:int(data[0])":
        fmt = "BatteryFormat::PercentFirst"
    else:
        raise ValueError(f"unknown battery format: {level}")
    return f"Some(Battery {{ cmd: {cmd(info, wireless)}, reply: {info['response_length']}, format: {fmt} }})"


def main():
    root, commit = Path(sys.argv[1]), sys.argv[2]
    devices = root / "rivalcfg" / "devices"
    dpi = {f"{p.stem}.choices": load_module(p, {})["choices"] for p in (devices / "dpi").glob("*.py") if p.stem != "__init__"}
    profiles = {}
    wireless_of = {}
    pending = []
    for path in sorted(devices.glob("*.py")):
        if path.stem == "__init__":
            continue
        source = path.read_text()
        if "_patch_command" in source:
            base = next(n.names[0].name for n in ast.parse(source).body if isinstance(n, ast.ImportFrom) and n.module is None)
            names = load_module(path, {})
            wireless_of[path.stem] = (base, names["profile"])
            continue
        # Profiles that reuse another one's settings ("rival110.profile[...]") come after it.
        others = {f"{stem}.profile": profile for stem, profile in profiles.items()}
        try:
            names = load_module(path, {**dpi, **others})
        except (KeyError, TypeError):
            pending.append(path)
            continue
        profiles[path.stem] = names["profile"]
    for path in pending:
        others = {f"{stem}.profile": profile for stem, profile in profiles.items()}
        profiles[path.stem] = load_module(path, {**dpi, **others})["profile"]

    out = [
        "//! Generated by tools/rivalcfg_to_rust.py from rivalcfg's device profiles",
        f"//! (https://github.com/flozz/rivalcfg, WTFPL, commit {commit}). Do not edit by hand.",
        "",
        "use super::steelseries::{Battery, BatteryFormat, Cmd, DpiEncoding, Kind, Params, Setting};",
        "use super::{Kind as DeviceKind, Model, Protocol};",
        "",
        "pub const MODELS: &[Model] = &[",
    ]
    skipped = []

    def emit(stem, profile, wireless, base_profile):
        models = profile["models"]
        endpoints = {m["endpoint"] for m in models}
        assert len(endpoints) == 1, stem
        settings_src = base_profile["settings"] if wireless else profile["settings"]
        kept = []
        for key, info in settings_src.items():
            if info.get("value_type") in SUPPORTED:
                kept.append(setting(key, info, wireless))
            else:
                skipped.append(f"{stem}.{key} ({info.get('value_type')})")
        src = base_profile if wireless else profile
        save = src.get("save_command")
        firmware = src.get("firmware_version")
        out.append("    Model {")
        out.append("        vendor_id: 0x1038,")
        out.append("        product_ids: &[" + ", ".join(f"0x{m['product_id']:04X}" for m in models) + "],")
        out.append('        brand: "SteelSeries",')
        name = profile["name"].replace("SteelSeries ", "")
        if wireless:
            name += " (through its receiver)"
        out.append(f"        name: {rust_str(name)},")
        out.append("        kind: DeviceKind::Mouse,")
        out.append(f"        interface: Some({endpoints.pop()}),")
        out.append("        usage_page: None,")
        out.append("        protocol: Protocol::SteelSeries(Params {")
        out.append(f"            save: {'Some(' + cmd(save, wireless) + ')' if save else 'None'},")
        if firmware and not wireless:
            out.append(f"            firmware: Some(({cmd(firmware, False)}, {firmware['response_length']})),")
        else:
            out.append("            firmware: None,")
        out.append(f"            battery: {battery(src.get('battery_level'), wireless)},")
        out.append(f"            readback: {WIRELESS_READBACK if wireless else 0},")
        out.append("            settings: &[")
        out.extend(f"                {s}," for s in kept)
        out.append("            ],")
        out.append("        }),")
        out.append("    },")

    for stem, profile in profiles.items():
        emit(stem, profile, False, None)
    for stem, (base, profile) in wireless_of.items():
        emit(stem, profile, True, profiles[base])
    out.append("];")
    out.append("")
    out.append("/// rivalcfg settings PeriHub leaves out for now.")
    out.append("pub const SKIPPED: &[&str] = &[")
    out.extend(f"    {rust_str(s)}," for s in skipped)
    out.append("];")
    print("\n".join(out))


if __name__ == "__main__":
    main()
