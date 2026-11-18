#!/usr/bin/env python3
"""Writes crates/perihub-core/src/peripheral/razer_models.rs: facts about Razer mice, from OpenRazer.

OpenRazer (https://github.com/openrazer/openrazer) is under the GPL. PeriHub takes only facts from it
(USB product ids, which USB interface takes the reports, the transaction id each command needs, the
maximum DPI, which lights have a brightness and how wide the custom colour frame is), read here as text and written in PeriHub's own table format. No code is copied, and
OpenRazer is never compiled or run.

Usage: python3 -I tools/openrazer_facts.py OPENRAZER_CHECKOUT COMMIT > crates/perihub-core/src/peripheral/razer_models.rs
where OPENRAZER_CHECKOUT holds driver/razermouse_driver.{c,h} and daemon/openrazer_daemon/hardware/mouse.py.
Mice only: the receivers and docks, which may be shared with a keyboard, are left out.

With --keyboards after COMMIT it writes crates/perihub-core/src/peripheral/razer_keyboard_models.rs instead,
from driver/razerkbd_driver.{c,h} and daemon/openrazer_daemon/hardware/keyboards.py (see main_keyboards).
"""

import ast
import re
import sys
from pathlib import Path


def defines(header):
    return {m[0]: int(m[1], 16) for m in re.findall(r"#define\s+(USB_DEVICE_ID_RAZER_\w+)\s+(0x[0-9A-Fa-f]+)", header)}


def function(source, name):
    start = source.index(f"static ssize_t {name}(")
    depth, i = 0, source.index("{", start)
    for j in range(i, len(source)):
        depth += {"{": 1, "}": -1}.get(source[j], 0)
        if depth == 0:
            return source[i : j + 1]
    raise ValueError(name)


def groups(body):
    """Each case group of the switch on the product id: its labels and its code."""
    out, labels, code = [], [], []
    for line in body.splitlines():
        label = re.match(r"\s*case\s+(USB_DEVICE_ID_RAZER_\w+)\s*:", line)
        default = re.match(r"\s*default\s*:", line)
        if label or default:
            if code and labels:
                out.append((labels, "\n".join(code)))
                labels, code = [], []
            labels.append(label.group(1) if label else "default")
        elif labels:
            code.append(line)
    if labels:
        out.append((labels, "\n".join(code)))
    return out


def facts(body, builder_before=None):
    """Per product define: (builder function, transaction id) as the group sets them."""
    out = {}
    for labels, code in groups(body):
        builder = re.search(r"request\s*=\s*(razer_\w+)\(", code)
        tid = re.search(r"transaction_id\.id\s*=\s*(0x[0-9A-Fa-f]+)", code)
        name = builder.group(1) if builder else builder_before
        for label in labels:
            out[label] = (name, int(tid.group(1), 16) if tid else None)
    return out


def write_dpi_facts(body):
    """razer_attr_write_dpi: a first switch where a few old mice build their own request and return,
    then set_dpi_xy for everyone, then a second switch with the transaction id (default: not
    supported)."""
    split = body.index("request = razer_chroma_misc_set_dpi_xy(varstore")
    first, second = body[:split], body[split:]
    special = {label for labels, code in groups(first) if re.search(r"request\s*=\s*razer_\w+\(", code) for label in labels}
    out = {}
    for labels, code in groups(second):
        tid = re.search(r"transaction_id\.id\s*=\s*(0x[0-9A-Fa-f]+)", code)
        for label in labels:
            if label != "default" and label not in special and tid:
                out[label] = ("razer_chroma_misc_set_dpi_xy", int(tid.group(1), 16))
    return out


def frame_facts(frame_body, custom_body):
    """Per product define: the transaction id and packet size of the custom colour frame, where the frame
    is the extended one (class 0f, command 03) and the effect that shows it is the extended custom one
    (class 0f, command 02, not stored), both with the same transaction id."""
    frames = {}
    for labels, code in groups(frame_body):
        builder = re.search(r"request\s*=\s*(razer_\w+)\(", code)
        tid = re.search(r"transaction_id\.id\s*=\s*(0x[0-9A-Fa-f]+)", code)
        if not builder or not tid:
            continue
        if builder.group(1) == "razer_chroma_extended_matrix_set_custom_frame":
            fit = False
        elif builder.group(1) == "razer_chroma_extended_matrix_set_custom_frame2" and re.search(r"&buf\[offset\],\s*0\)", code):
            fit = True
        else:
            continue
        for label in labels:
            frames[label] = (int(tid.group(1), 16), fit)
    shows = facts(custom_body)
    out = {}
    for label, (tid, fit) in frames.items():
        builder, custom_tid = shows.get(label, (None, None))
        if builder == "razer_chroma_extended_matrix_effect_custom_frame" and custom_tid == tid:
            out[label] = (tid, fit)
    return out


# The lights of a mouse, by the name OpenRazer's daemon gives them, and the LED id the driver sends.
ZONES = [("logo", "Logo", 0x04), ("scroll", "Scroll wheel", 0x01), ("left", "Left side", 0x11),
         ("right", "Right side", 0x10), ("backlight", "Backlight", 0x05)]
BRIGHTNESS = {"razer_chroma_extended_matrix_get_brightness": ("razer_chroma_extended_matrix_brightness", "false"),
              "razer_chroma_standard_get_led_brightness": ("razer_chroma_standard_set_led_brightness", "true")}


def daemon_mice(path):
    tree = ast.parse(path.read_text())
    classes = {}
    for node in tree.body:
        if not isinstance(node, ast.ClassDef):
            continue
        attrs = {}
        for stmt in node.body:
            if isinstance(stmt, ast.Assign) and isinstance(stmt.targets[0], ast.Name):
                try:
                    attrs[stmt.targets[0].id] = ast.literal_eval(stmt.value)
                except ValueError:
                    pass
        doc = ast.get_docstring(node) or ""
        attrs["name"] = re.sub(r"^Class for the Razer ", "", doc.strip().splitlines()[0] if doc else node.name)
        attrs["base"] = node.bases[0].id if node.bases and isinstance(node.bases[0], ast.Name) else None
        classes[node.name] = attrs
    mice = {}
    for cls, attrs in classes.items():
        merged, c = {}, cls
        while c in classes:
            for k, v in classes[c].items():
                merged.setdefault(k, v)
            c = classes[c]["base"]
        if "USB_PID" in merged:
            mice[merged["USB_PID"]] = merged
    return mice


def main():
    root, commit = Path(sys.argv[1]), sys.argv[2]
    driver = (root / "driver" / "razermouse_driver.c").read_text()
    ids = defines((root / "driver" / "razermouse_driver.h").read_text())
    mice = daemon_mice(root / "daemon" / "openrazer_daemon" / "hardware" / "mouse.py")

    report = function_static_int(driver, "razer_get_report")
    index3 = {label for labels, code in groups(report) if "index = 0x03" in code for label in labels}
    waits = {}
    for labels, code in groups(report):
        wait = re.search(r"(RAZER_\w+_WAIT_US)\)", code)
        for label in labels:
            waits[label] = wait.group(1) if wait else "RAZER_MOUSE_WAIT_US"
    firmware = facts(function(driver, "razer_attr_read_firmware_version"), "razer_chroma_standard_get_firmware_version")
    read_dpi_body = function(driver, "razer_attr_read_dpi")
    read_dpi = facts(read_dpi_body)
    stored_read = {label for labels, code in groups(read_dpi_body) if "get_dpi_xy(VARSTORE)" in code for label in labels}
    write_dpi = write_dpi_facts(function(driver, "razer_attr_write_dpi"))
    read_poll = facts(function(driver, "razer_attr_read_poll_rate"))
    write_poll = facts(function(driver, "razer_attr_write_poll_rate"))
    bright_get = facts(function(driver, "razer_attr_read_led_brightness"))
    bright_set = facts(function(driver, "razer_attr_write_led_brightness"))
    # Each light reaches the same two functions with its own LED: check that the ids above are those.
    leds = {m[0]: int(m[1], 16) for m in re.findall(r"#define\s+(\w+_LED)\s+(0x[0-9A-Fa-f]+)",
                                                   (root / "driver" / "razercommon.h").read_text())}
    for zone, _, led in ZONES:
        for way in ("read", "write"):
            body = function(driver, f"razer_attr_{way}_{zone}_led_brightness")
            used = re.search(rf"razer_attr_{way}_led_brightness\([^;]*?(\w+_LED)\);", body)
            if not used or leds.get(used.group(1)) != led:
                sys.exit(f"{zone}: OpenRazer does not use LED 0x{led:02x} for its brightness")
    frames = frame_facts(function(driver, "razer_attr_write_matrix_custom_frame"),
                         function(driver, "razer_attr_write_matrix_effect_custom"))

    wait_us = {"RAZER_MOUSE_WAIT_US": 600, "RAZER_NEW_MOUSE_RECEIVER_WAIT_US": 31000,
               "RAZER_ATHERIS_RECEIVER_WAIT_US": 400000, "RAZER_VIPER_MOUSE_RECEIVER_WAIT_US": 59900}
    out = [
        "//! Generated by tools/openrazer_facts.py from facts in OpenRazer",
        f"//! (https://github.com/openrazer/openrazer, GPL-2.0, commit {commit}): product ids, the USB",
        "//! interface of the reports, transaction ids, maximum DPI, the lights with a brightness and the width of",
        "//! the custom colour frame. No code was copied. Do not edit by hand.",
        "",
        "use super::razer::{Brightness, Frame, Params, Tid};",
        "use super::{Kind, Model, Protocol};",
        "",
        "pub const MODELS: &[Model] = &[",
    ]
    left_out = []
    for define, pid in sorted(ids.items(), key=lambda kv: kv[1]):
        if pid not in mice:
            continue
        if re.search(r"RECEIVER|DOCK|HYPERSPEED|DONGLE|BLUETOOTH|_WIRELESS(_ALT)?$", define):
            left_out.append(f"{define} (receiver or wireless dongle)")
            continue
        m = mice[pid]
        methods = m.get("METHODS", [])
        def tid(table, wanted):
            builder, t = table.get(define, table.get("default", (None, None)))
            return f"Some(0x{t:02X})" if builder == wanted and t is not None else "None"
        fw = tid(firmware, "razer_chroma_standard_get_firmware_version")
        dpi_get = tid(read_dpi, "razer_chroma_misc_get_dpi_xy")
        dpi_set = tid(write_dpi, "razer_chroma_misc_set_dpi_xy")
        poll_get = tid(read_poll, "razer_chroma_misc_get_polling_rate")
        poll_set = tid(write_poll, "razer_chroma_misc_set_polling_rate")
        # A setting is changed only where reading and writing it use the same transaction id.
        if dpi_set != dpi_get:
            dpi_set = "None"
        if poll_set != poll_get:
            poll_set = "None"
        if "get_dpi_xy" not in methods:
            dpi_get = dpi_set = "None"
        if "get_poll_rate" not in methods:
            poll_get = poll_set = "None"
        if fw == dpi_get == poll_get == "None":
            left_out.append(f"{define} (no command PeriHub knows)")
            continue
        # Brightness: the lights the daemon offers it for, where reading and writing it use the same
        # family of commands and the same transaction id.
        brightness = "None"
        got, got_tid = bright_get.get(define, bright_get.get("default", (None, None)))
        put, put_tid = bright_set.get(define, bright_set.get("default", (None, None)))
        if got in BRIGHTNESS and BRIGHTNESS[got][0] == put and got_tid == put_tid and got_tid is not None:
            leds = [f"0x{led:02X}" for zone, _, led in ZONES
                    if f"get_{zone}_brightness" in methods and f"set_{zone}_brightness" in methods]
            if leds:
                brightness = (f"Some(Brightness {{ leds: &[{', '.join(leds)}], standard: {BRIGHTNESS[got][1]}, "
                              f"tid: 0x{got_tid:02X} }})")
        # The custom colour frame: one row, where the daemon offers it.
        frame = "None"
        dims = m.get("MATRIX_DIMS")
        if (define in frames and "set_custom_effect" in methods and "set_key_row" in methods
                and dims and dims[0] == 1 and 1 <= dims[1] <= 25):
            frame_tid, fit = frames[define]
            frame = f"Some(Frame {{ tid: 0x{frame_tid:02X}, columns: {dims[1]}, fit: {str(fit).lower()} }})"
        name = m["name"].replace('"', "")
        out.append("    Model {")
        out.append("        vendor_id: 0x1532,")
        out.append(f"        product_ids: &[0x{pid:04X}],")
        out.append('        brand: "Razer",')
        out.append(f'        name: "{name}",')
        out.append("        kind: Kind::Mouse,")
        out.append(f"        interface: Some({3 if define in index3 else 0}),")
        out.append("        usage_page: None,")
        out.append("        protocol: Protocol::Razer(Params {")
        out.append(f"            wait_us: {wait_us[waits.get(define, 'RAZER_MOUSE_WAIT_US')]},")
        out.append(f"            dpi_max: {m.get('DPI_MAX', 0)},")
        out.append(f"            tid: Tid {{ firmware: {fw}, dpi_get: {dpi_get}, dpi_set: {dpi_set}, poll_get: {poll_get}, poll_set: {poll_set} }},")
        out.append(f"            dpi_read_stored: {str(define in stored_read).lower()},")
        out.append(f"            brightness: {brightness},")
        out.append(f"            frame: {frame},")
        out.append("        }),")
        out.append("    },")
    out.append("];")
    out.append("")
    out.append("/// OpenRazer mice PeriHub leaves out, and why.")
    out.append("pub const LEFT_OUT: &[&str] = &[")
    out.extend(f'    "{x}",' for x in left_out)
    out.append("];")
    print("\n".join(out))


def function_static_int(source, name):
    start = source.index(f"static int {name}(")
    depth, i = 0, source.index("{", start)
    for j in range(i, len(source)):
        depth += {"{": 1, "}": -1}.get(source[j], 0)
        if depth == 0:
            return source[i : j + 1]
    raise ValueError(name)


# ---------------------------------------------------------------------------------------------
# Keyboards: python3 -I tools/openrazer_facts.py OPENRAZER_CHECKOUT COMMIT --keyboards
#   > crates/perihub-core/src/peripheral/razer_keyboard_models.rs
# Reads driver/razerkbd_driver.{c,h} and daemon/openrazer_daemon/hardware/keyboards.py. Only what
# PeriHub reads from a keyboard: the interface of the reports, the wait before the answer, and the
# transaction id of the battery, brightness and game mode reads. Laptops and keypads are left out,
# and so is any product id another OpenRazer driver also claims (a receiver shared with a mouse).

# Keyboards whose interface 70-perihub.rules opens on Linux: only where a tester showed what that
# interface carries (issue #24: through the receiver, only the media keys). On most Razer keyboards
# the interface OpenRazer sends to also carries typed keys, so the rule must not open it.
LINUX_OPENED = {0x02B9, 0x02BA}
# Names PeriHub already used, kept so that settings saved with them still match.
NAMES = {0x02B9: "BlackWidow V4 Mini HyperSpeed", 0x02BA: "BlackWidow V4 Mini HyperSpeed (wireless)"}
NOT_KEYBOARDS = r"Blade|Book|Tartarus|Orbweaver|Nostromo"
KBD_WAITS = {"RAZER_BLACKWIDOW_CHROMA_WAIT_US": 600, "RAZER_BLACKWIDOW_V3_WIRELESS_WAIT_US": 4900,
             "RAZER_DEATHSTALKER_V2_WIRELESS_WAIT_US": 4900}


def function_void(source, name):
    start = source.index(f"static void {name}(")
    depth, i = 0, source.index("{", start)
    for j in range(i, len(source)):
        depth += {"{": 1, "}": -1}.get(source[j], 0)
        if depth == 0:
            return source[i : j + 1]
    raise ValueError(name)


def daemon_classes(path):
    """Every device class of a daemon file, with what it inherits, by product id."""
    tree = ast.parse(path.read_text())
    classes = {}
    for node in tree.body:
        if not isinstance(node, ast.ClassDef):
            continue
        attrs = {}
        for stmt in node.body:
            if isinstance(stmt, ast.Assign) and isinstance(stmt.targets[0], ast.Name):
                try:
                    attrs[stmt.targets[0].id] = ast.literal_eval(stmt.value)
                except ValueError:
                    pass
        doc = ast.get_docstring(node) or ""
        attrs["name"] = re.sub(r"^Class for the (Razer )?", "", doc.strip().splitlines()[0] if doc else node.name)
        attrs["class"] = node.name
        attrs["base"] = node.bases[0].id if node.bases and isinstance(node.bases[0], ast.Name) else None
        classes[node.name] = attrs
    out = {}
    for cls in classes:
        merged, c = {}, cls
        while c in classes:
            for k, v in classes[c].items():
                merged.setdefault(k, v)
            c = classes[c]["base"]
        merged["class"], merged["name"] = cls, classes[cls]["name"]
        if "USB_PID" in merged:
            out[merged["USB_PID"]] = merged
    return out


def read_facts(body):
    """Per product define: (builder, its arguments, transaction id); for a group that tells laptops
    apart, the builder of the other devices (the last one)."""
    out = {}
    for labels, code in groups(body):
        builders = re.findall(r"request\s*=\s*(razer_\w+)\(([^)]*)\)", code)
        tid = re.search(r"transaction_id\.id\s*=\s*(0x[0-9A-Fa-f]+)", code)
        for label in labels:
            out[label] = (builders[-1][0], builders[-1][1].replace(" ", ""), int(tid.group(1), 16)) if builders and tid else None
    return out


def main_keyboards(root, commit):
    driver = (root / "driver" / "razerkbd_driver.c").read_text()
    ids = defines((root / "driver" / "razerkbd_driver.h").read_text())
    others = set()
    for h in ("razermouse_driver.h", "razeraccessory_driver.h", "razerkraken_driver.h"):
        others |= set(defines((root / "driver" / h).read_text()).values())
    daemon = daemon_classes(root / "daemon" / "openrazer_daemon" / "hardware" / "keyboards.py")

    # The firmware and the layout are read the same way on every keyboard, with transaction id ff.
    fw = function(driver, "razer_attr_read_firmware_version")
    if not re.search(r"razer_chroma_standard_get_firmware_version\(\);\s*request\.transaction_id\.id = 0xFF;", fw):
        sys.exit("the firmware is no longer read the same way on every keyboard")
    layout = function(driver, "razer_attr_read_kbd_layout")
    if not re.search(r"get_razer_report\(0x00, 0x86, 0x02\);\s*request\.transaction_id\.id = 0xFF;", layout):
        sys.exit("the layout is no longer read the same way on every keyboard")

    params = {}
    for labels, code in groups(function_void(driver, "razer_get_report_params")):
        index = re.search(r"\*report_index\s*=\s*(0x[0-9A-Fa-f]+)", code)
        response = re.search(r"\*response_index\s*=\s*(0x[0-9A-Fa-f]+)", code)
        wait = re.search(r"\*wait\s*=\s*(RAZER_\w+_WAIT_US)", code)
        if index.group(1) != response.group(1):
            sys.exit(f"{labels}: the answer comes from another interface")
        for label in labels:
            params[label] = (int(index.group(1), 16), KBD_WAITS[wait.group(1)])
    bright = read_facts(function(driver, "razer_attr_read_matrix_brightness"))
    game = read_facts(function(driver, "razer_attr_read_game_led_state"))
    # The battery reads build their request before the switch, which only sets the transaction id.
    level = {k: v[1] for k, v in facts(function(driver, "razer_attr_read_charge_level"), "get_battery_level").items() if v[1]}
    charging = {k: v[1] for k, v in facts(function(driver, "razer_attr_read_charge_status"), "get_charging_status").items() if v[1]}
    for name, body in (("level", "razer_attr_read_charge_level"), ("charging", "razer_attr_read_charge_status")):
        wanted = {"level": "razer_chroma_misc_get_battery_level()", "charging": "razer_chroma_misc_get_charging_status()"}[name]
        if f"request = {wanted};" not in function(driver, body):
            sys.exit(f"the battery {name} is no longer read the same way")

    out = [
        "//! Generated by tools/openrazer_facts.py --keyboards from facts in OpenRazer",
        f"//! (https://github.com/openrazer/openrazer, GPL-2.0, commit {commit}): product ids, the USB",
        "//! interface of the reports, the wait before the answer and the transaction id of each read. No code",
        "//! was copied. Do not edit by hand.",
        "",
        "use super::razer_keyboard::{Brightness, Params};",
        "use super::{Kind, Model, Protocol};",
        "",
        "pub const MODELS: &[Model] = &[",
    ]
    left_out = []
    for define, pid in sorted(ids.items(), key=lambda kv: kv[1]):
        if pid not in daemon:
            continue
        m = daemon[pid]
        if re.search(NOT_KEYBOARDS, m["class"]):
            left_out.append(f"{define} (not a keyboard, or built into a laptop)")
            continue
        if pid in others:
            left_out.append(f"{define} (another OpenRazer driver claims it too)")
            continue
        methods = m.get("METHODS", [])
        interface, wait_us = params.get(define, params["default"])
        b = bright.get(define, bright.get("default"))
        brightness = "None"
        if b and b[1] == "VARSTORE,BACKLIGHT_LED":
            if b[0] == "razer_chroma_extended_matrix_get_brightness":
                brightness = f"Some(Brightness {{ standard: false, tid: 0x{b[2]:02X} }})"
            elif b[0] == "razer_chroma_standard_get_led_brightness":
                brightness = f"Some(Brightness {{ standard: true, tid: 0x{b[2]:02X} }})"
        g = game.get(define, game.get("default"))
        game_tid = "None"
        if g and g[0] == "razer_chroma_standard_get_led_state" and g[1] == "VARSTORE,GAME_LED" and "get_game_mode" in methods:
            game_tid = f"Some(0x{g[2]:02X})"
        lv, ch = level.get(define, level.get("default")), charging.get(define, charging.get("default"))
        battery = "None"
        if lv is not None and lv == ch and "get_battery" in methods and "is_charging" in methods:
            battery = f"Some(0x{lv:02X})"
        name = NAMES.get(pid, m["name"]).replace('"', "")
        out.append("    Model {")
        out.append("        vendor_id: 0x1532,")
        out.append(f"        product_ids: &[0x{pid:04X}],")
        out.append('        brand: "Razer",')
        out.append(f'        name: "{name}",')
        out.append("        kind: Kind::Keyboard,")
        out.append(f"        interface: Some({interface}),")
        out.append("        usage_page: None,")
        out.append("        protocol: Protocol::RazerKeyboard(Params {")
        out.append(f"            wait_us: {wait_us},")
        out.append(f"            battery: {battery},")
        out.append(f"            brightness: {brightness},")
        out.append(f"            game: {game_tid},")
        out.append(f"            linux: {str(pid in LINUX_OPENED).lower()},")
        out.append("        }),")
        out.append("    },")
    out.append("];")
    out.append("")
    out.append("/// OpenRazer keyboards PeriHub leaves out, and why.")
    out.append("pub const LEFT_OUT: &[&str] = &[")
    out.extend(f'    "{x}",' for x in left_out)
    out.append("];")
    print("\n".join(out))


if __name__ == "__main__":
    if sys.argv[3:] == ["--keyboards"]:
        main_keyboards(Path(sys.argv[1]), sys.argv[2])
    else:
        main()
