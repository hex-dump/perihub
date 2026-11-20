#!/usr/bin/env python3
"""Writes crates/perihub-core/src/peripheral/razer_models.rs: facts about Razer mice, from OpenRazer.

OpenRazer (https://github.com/openrazer/openrazer) is under the GPL. PeriHub takes only facts from it
(USB product ids, which USB interface takes the reports, the transaction id each command needs, the
maximum DPI, which lights have a brightness and how wide the custom colour frame is), read here as text and written in PeriHub's own table format. No code is copied, and
OpenRazer is never compiled or run.

Usage: python3 -I tools/openrazer_facts.py OPENRAZER_CHECKOUT COMMIT > crates/perihub-core/src/peripheral/razer_models.rs
where OPENRAZER_CHECKOUT holds driver/razermouse_driver.{c,h} and daemon/openrazer_daemon/hardware/mouse.py.
Mice only: the receivers and docks, which may be shared with a keyboard, are left out, except the ids that
tools/auto-add.json allows ({"allow": ["1532:00c1"]}).

With --keyboards after COMMIT it writes crates/perihub-core/src/peripheral/razer_keyboard_models.rs instead,
from driver/razerkbd_driver.{c,h}, driver/razerchromacommon.c and daemon/openrazer_daemon/hardware/keyboards.py
(see main_keyboards).
"""

import ast
import json
import re
import sys
from pathlib import Path


ALLOW_FILE = Path(__file__).with_name("auto-add.json")


def allowed_pids(path=ALLOW_FILE):
    """Razer mice (product ids) the allow list tools/auto-add.json asks for even though they are receivers,
    docks or wireless dongles, which are otherwise left out: tools/auto_add.py adds an id there when a
    request for it is ready to add (the pull request it makes puts the device into the next release as experimental)."""
    if not path.exists():
        return set()
    ids = json.loads(path.read_text())["allow"]
    return {int(i.split(":")[1], 16) for i in ids if re.fullmatch(r"1532:[0-9a-f]{4}", i)}


ALLOWED = allowed_pids()


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


# Wireless ids that are the mouse itself behind its own dongle (nothing else shares the dongle), so a
# mouse entry is right for them; OpenRazer documents them as a Viper V3 Pro with the dongle's wait.
MOUSE_DONGLES = {"USB_DEVICE_ID_RAZER_VIPER_V3_PRO_WIRELESS", "USB_DEVICE_ID_RAZER_VIPER_V3_PRO_SE_WIRELESS"}


def daemon_mice(path):
    tree = ast.parse(path.read_text())
    classes = {}
    for node in tree.body:
        if not isinstance(node, ast.ClassDef):
            continue
        attrs = {}
        for stmt in node.body:
            if isinstance(stmt, ast.Assign) and isinstance(stmt.targets[0], ast.Name):
                name, value = stmt.targets[0].id, stmt.value
                # "METHODS = Base.METHODS + [...]": the base's list and what the class adds.
                if (isinstance(value, ast.BinOp) and isinstance(value.op, ast.Add) and isinstance(value.left, ast.Attribute)
                        and value.left.attr == name and isinstance(value.left.value, ast.Name)):
                    attrs[name + "_PLUS"] = (value.left.value.id, ast.literal_eval(value.right))
                    continue
                try:
                    attrs[name] = ast.literal_eval(value)
                except ValueError:
                    pass
        doc = ast.get_docstring(node) or ""
        attrs["name"] = re.sub(r"^Class for the Razer ", "", doc.strip().splitlines()[0] if doc else node.name)
        attrs["base"] = node.bases[0].id if node.bases and isinstance(node.bases[0], ast.Name) else None
        classes[node.name] = attrs

    def listed(cls, name):
        own = classes[cls]
        if name in own:
            return own[name]
        if name + "_PLUS" in own:
            base, extra = own[name + "_PLUS"]
            return (listed(base, name) or []) + extra
        return listed(own["base"], name) if own["base"] in classes else None

    mice = {}
    for cls, attrs in classes.items():
        merged, c = {}, cls
        while c in classes:
            for k, v in classes[c].items():
                merged.setdefault(k, v)
            c = classes[c]["base"]
        if listed(cls, "METHODS") is not None:
            merged["METHODS"] = listed(cls, "METHODS")
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
    poll2_read, poll2_write, poll2_second, poll2_codes = poll2_facts(driver, root)
    level_tid, charge_tid = battery_facts(driver)
    dongle_get, dongle_set = dongle_led_facts(driver, root)
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
        "//! interface of the reports, transaction ids, maximum DPI, the polling rates, the battery, the lights with a",
        "//! brightness and the width of the custom colour frame. No code was copied. Do not edit by hand.",
        "",
        "use super::razer::{Brightness, Frame, Params, Poll2, Tid};",
        "use super::{Kind, Model, Protocol};",
        "",
        "pub const MODELS: &[Model] = &[",
    ]
    left_out = []
    for define, pid in sorted(ids.items(), key=lambda kv: kv[1]):
        if pid not in mice:
            continue
        if define not in MOUSE_DONGLES and pid not in ALLOWED and re.search(r"RECEIVER|DOCK|HYPERSPEED|DONGLE|BLUETOOTH|_WIRELESS(_ALT)?$", define):
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
        # The second polling rate command (class 00, command c0 and 40, up to 8000 Hz): where reading it and
        # sending it use the same transaction id, with the rates the daemon offers; the driver sends it
        # twice, the second time with the first argument 01 and perhaps another transaction id.
        poll2 = "None"
        read2, write2, second2 = poll2_read.get(define), poll2_write.get(define), poll2_second.get(define)
        if (read2 is not None and read2 == write2 and second2 is not None
                and "get_poll_rate" in methods and "set_poll_rate" in methods):
            rates = m.get("POLL_RATES") or [125, 500, 1000]  # the daemon's default, device_base.py
            if not set(rates) <= set(poll2_codes):
                sys.exit(f"{define}: polling rates {rates} that OpenRazer's driver has no code for")
            second = "None" if second2 == 0 else f"Some(0x{second2:02X})"
            poll2 = f"Some(Poll2 {{ tid: 0x{read2:02X}, second_tid: {second}, rates: &[{', '.join(map(str, sorted(rates)))}] }})"
        # The battery: its level and its charging state are read with the same transaction id.
        battery = "None"
        if (level_tid.get(define, level_tid.get("default")) is not None
                and level_tid.get(define, level_tid.get("default")) == charge_tid.get(define, charge_tid.get("default"))
                and "get_battery" in methods and "is_charging" in methods):
            battery = f"Some(0x{level_tid.get(define, level_tid.get('default')):02X})"
        # The indicator light of a mouse's own wireless dongle: a mode from 1 to 3.
        dongle = "None"
        d_get, d_set = dongle_get.get(define), dongle_set.get(define)
        if (d_get is not None and d_get == d_set and "get_hyperpolling_wireless_dongle_indicator_led_mode" in methods
                and "set_hyperpolling_wireless_dongle_indicator_led_mode" in methods):
            dongle = f"Some(0x{d_get:02X})"
        if fw == dpi_get == poll_get == poll2 == battery == dongle == "None":
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
        out.append(f"            poll2: {poll2},")
        out.append(f"            battery: {battery},")
        out.append(f"            dongle_led: {dongle},")
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


def poll2_facts(driver, root):
    """The second polling rate command, per product define: the transaction id of its read, of its first
    write and of its second write (0: the driver sends no second one); and the code of each rate, checked
    between the driver's read and the command builder."""
    common = (root / "driver" / "razerchromacommon.c").read_text()
    get = re.search(r"razer_chroma_misc_get_polling_rate2\(void\)\s*\{\s*return get_razer_report\(0x00, 0xC0, 0x01\);", common)
    setter = common[common.index("razer_chroma_misc_set_polling_rate2("):]
    setter = setter[: setter.index("\n}\n")]
    if not get or "get_razer_report(0x00, 0x40, 0x02)" not in setter or "report.arguments[0] = argument;" not in setter:
        sys.exit("the second polling rate command is no longer class 00, c0 and 40 with two arguments")
    codes = {int(hz): int(code, 16) for hz, code in re.findall(r"case\s+(\d+):\s*report\.arguments\[1\]\s*=\s*(0x[0-9A-Fa-f]+);", setter)}
    read_body = function(driver, "razer_attr_read_poll_rate")
    read_codes = {int(hz): int(code, 16) for code, hz in re.findall(r"case\s+(0x[0-9A-Fa-f]+):\s*polling_rate\s*=\s*(\d+);", poll2_read_part(read_body))}
    for hz, code in read_codes.items():
        if codes.get(hz) != code and hz in (125, 500, 1000, 2000, 4000, 8000):
            sys.exit(f"polling rate {hz}: OpenRazer reads and writes it with different codes")
    reads = {k: v[1] for k, v in facts(read_body).items() if v[0] == "razer_chroma_misc_get_polling_rate2"}
    write_body = function(driver, "razer_attr_write_poll_rate")
    cut = write_body.index("razer_send_payload(")
    first = {k: v[1] for k, v in facts(write_body[:cut]).items() if v[0] == "razer_chroma_misc_set_polling_rate2"}
    second = {}
    for labels, code in groups(write_body[cut:]):
        if re.search(r"razer_chroma_misc_set_polling_rate2\(polling_rate, 0x01\)", code):
            tid = re.search(r"transaction_id\.id\s*=\s*(0x[0-9A-Fa-f]+)", code)
            for label in labels:
                if label in first and tid:
                    second[label] = int(tid.group(1), 16)
    return reads, first, second, codes


def poll2_read_part(read_body):
    """The part of the polling rate read that handles the second command: up to its own return."""
    start = read_body.index("razer_chroma_misc_get_polling_rate2")
    return read_body[start : read_body.index("return sysfs_emit", start)]


def battery_facts(driver):
    """The transaction id of the battery level read and of the charging state read, per product define
    (the request is built before the switch, which only sets the transaction id)."""
    out = []
    for name, wanted in (("razer_attr_read_charge_level", "razer_chroma_misc_get_battery_level()"),
                         ("razer_attr_read_charge_status", "razer_chroma_misc_get_charging_status()")):
        body = function(driver, name)
        if f"request = {wanted};" not in body:
            sys.exit(f"{name} no longer builds {wanted}")
        out.append({k: v[1] for k, v in facts(body, "get").items()})
    return out


def dongle_led_facts(driver, root):
    """The mouse's own wireless dongle has an indicator light with a mode from 1 to 3 (connection status,
    battery status, battery warning only): read with class 07 command 90, set with class 07 command 10."""
    common = (root / "driver" / "razerchromacommon.c").read_text()
    if ("get_razer_report(0x07, 0x90, 0x01)" not in common or "get_razer_report(0x07, 0x10, 0x01)" not in common
            or "mode < 0x01 || mode > 0x03" not in common):
        sys.exit("the dongle indicator light is no longer class 07, commands 90 and 10, modes 1 to 3")
    out = []
    for way, wanted in (("read", "razer_chroma_misc_get_hyperpolling_wireless_dongle_indicator_led_mode"),
                        ("write", "razer_chroma_misc_set_hyperpolling_wireless_dongle_indicator_led_mode")):
        body = function(driver, f"razer_attr_{way}_hyperpolling_wireless_dongle_indicator_led_mode")
        if wanted not in body:
            sys.exit(f"the dongle indicator light is no longer built with {wanted}")
        out.append({k: v[1] for k, v in facts(body, "x").items() if v[1] is not None})
    return out


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
# Keyboards PeriHub also changes (the brightness, the game mode and one colour on the custom frame), not
# only reads. Only the one with a requester; the others follow once a tester has confirmed it.
CHANGES = {0x02B9, 0x02BA}
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


def check_builders(common):
    """The commands PeriHub sends to a keyboard it changes are written in its driver with the bytes below:
    the class and command of each, its size and the arguments' places. A change in OpenRazer's builders
    stops the tool, so the facts of the driver and of the documentation stay the ones read here."""
    expected = {
        "razer_chroma_extended_matrix_brightness": r"get_razer_report\(0x0F, 0x04, 0x03\);\s*report\.arguments\[0\] = variable_storage;\s*report\.arguments\[1\] = led_id;\s*report\.arguments\[2\] = brightness;",
        "razer_chroma_standard_set_led_state": r"get_razer_report\(0x03, 0x00, 0x03\);\s*report\.arguments\[0\] = variable_storage;\s*report\.arguments\[1\] = led_id;\s*report\.arguments\[2\] = clamp\(led_state, 0x00, 0x01\);",
        "razer_chroma_extended_matrix_set_custom_frame": r"razer_chroma_extended_matrix_set_custom_frame2\(row_index, start_col, stop_col, rgb_data, 0x47\)",
        "razer_chroma_extended_matrix_set_custom_frame2": r"get_razer_report\(0x0F, 0x03, data_length\);.*?report\.arguments\[2\] = row_index;\s*report\.arguments\[3\] = start_col;\s*report\.arguments\[4\] = stop_col;\s*memcpy\(&report\.arguments\[5\], rgb_data, row_length\);",
        "razer_chroma_extended_matrix_effect_custom_frame": r"razer_chroma_extended_matrix_effect_base\(0x0C, 0x00, 0x00, 0x08\)",
        "razer_chroma_extended_matrix_effect_base": r"get_razer_report\(0x0F, 0x02, arg_size\);\s*report\.arguments\[0\] = variable_storage;\s*report\.arguments\[1\] = led_id;\s*report\.arguments\[2\] = effect_id;",
    }
    for name, pattern in expected.items():
        found = re.search(rf"^(?:static )?struct razer_report {name}\(", common, re.M)
        if not found:
            sys.exit(f"{name} is gone from razerchromacommon.c")
        body = common[found.start() : common.find("\n}\n", found.start())]
        if not re.search(pattern, body, re.S):
            sys.exit(f"{name} no longer builds its request the way PeriHub's keyboard driver sends it")


def change_params(define, m, read_brightness, read_game, brightness_set, game_set, frame_set, custom_set):
    """The `change` parameter of a keyboard PeriHub changes. It stops the tool unless OpenRazer changes the
    brightness and the game mode with the transaction id it reads them with, draws the custom frame row by row
    and then shows it with the custom effect (with one transaction id), and the daemon draws it on a matrix."""
    name = m["name"]
    bs, gs = brightness_set.get(define), game_set.get(define)
    fs, cs = frame_set.get(define), custom_set.get(define)
    if not (read_brightness and read_brightness[:2] == ("razer_chroma_extended_matrix_get_brightness", "VARSTORE,BACKLIGHT_LED")):
        sys.exit(f"{name}: the brightness is not read with the extended command")
    if bs != ("razer_chroma_extended_matrix_brightness", "VARSTORE,BACKLIGHT_LED,brightness", read_brightness[2]):
        sys.exit(f"{name}: the brightness is not written the way it is read")
    if not (read_game and read_game[:2] == ("razer_chroma_standard_get_led_state", "VARSTORE,GAME_LED")):
        sys.exit(f"{name}: the game mode is not read the way PeriHub reads it")
    if gs != ("razer_chroma_standard_set_led_state", "VARSTORE,GAME_LED,enabled", read_game[2]):
        sys.exit(f"{name}: the game mode is not written the way it is read")
    if not (fs and fs[0] == "razer_chroma_extended_matrix_set_custom_frame" and fs[1].startswith("row_id,start_col,stop_col,")):
        sys.exit(f"{name}: the custom frame is not the standard row command")
    if cs != ("razer_chroma_extended_matrix_effect_custom_frame", "", fs[2]):
        sys.exit(f"{name}: the custom effect does not use the transaction id of the frame")
    methods = m.get("METHODS", [])
    dims = m.get("MATRIX_DIMS")
    if not (m.get("HAS_MATRIX") and "set_key_row" in methods and "set_custom_effect" in methods):
        sys.exit(f"{name}: the daemon does not draw a custom frame on it")
    if not (isinstance(dims, list) and len(dims) == 2 and 1 <= dims[0] <= 6 and 1 <= dims[1] <= 25):
        sys.exit(f"{name}: matrix size {dims}")
    return f"Some(Change {{ frame: Frame {{ tid: 0x{fs[2]:02X}, rows: {dims[0]}, columns: {dims[1]} }} }})"


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
    check_builders((root / "driver" / "razerchromacommon.c").read_text())
    bright = read_facts(function(driver, "razer_attr_read_matrix_brightness"))
    game = read_facts(function(driver, "razer_attr_read_game_led_state"))
    # The battery reads build their request before the switch, which only sets the transaction id.
    # What PeriHub changes: the same builders the reads and the daemon's custom drawing use.
    brightness_set = read_facts(function(driver, "razer_attr_write_matrix_brightness"))
    game_set = read_facts(function(driver, "razer_attr_write_game_led_state"))
    frame_set = read_facts(function(driver, "razer_attr_write_matrix_custom_frame"))
    custom_set = read_facts(function(driver, "razer_attr_write_matrix_effect_custom"))
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
        "use super::razer_keyboard::{Brightness, Change, Frame, Params};",
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
        change = "None"
        if pid in CHANGES:
            change = change_params(define, m, b, g, brightness_set, game_set, frame_set, custom_set)
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
        out.append(f"            change: {change},")
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
