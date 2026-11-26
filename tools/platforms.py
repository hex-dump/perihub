#!/usr/bin/env python3
"""The configuration collections of the device families PeriHub already has a driver for: one table, read by
tools/triage.py (to recognize a requested device by the HID descriptors its request shows) and tools/auto_add.py
(to add the id of a recognized device to its family's table, read only, without a person).

A family is recognized when the request's "What PeriHub saw" block (the app's Send a report → Request support) has
a collection that is the family's: the same USB interface (where the family has a fixed one), usage page and usage
("HID collections"), and, where the family's signature has them, the report ids and lengths of that collection
("Report descriptors (lengths in bytes, with the id byte)"). Two levels:

  "full"        the collection and its reports match (or the family's signature has no reports to compare);
  "collection"  the collection matches but the request has no report descriptor for it (made with a PeriHub older
                than 0.9.7, or "not read"): the family is likely, a person confirms.

A report descriptor that is there and differs is no match. A request that matches more than one family matches none.
Only "full" matches of a family with "sibling" (the fields auto_add.py needs to write the id into the driver's
table) are added without a person. Every signature comes from the driver named in "driver" (its constants, and the
devices that answered as documented); tools/test_platforms.py checks the driver still says so.

The text is from anyone: it is only parsed with regular expressions, never run.
"""

import re
from dataclasses import dataclass, field


@dataclass(frozen=True)
class Report:
    """A report the collection declares: its id (0: no id) and its lengths in bytes, with the id byte."""

    id: int
    input: int = 0
    output: int = 0
    feature: int = 0


@dataclass(frozen=True)
class Collection:
    usage_page: int
    usage: int | None  # None: any usage on that page
    interface: int | None = None  # None: any interface
    reports: tuple[Report, ...] = ()  # each must be declared by the collection; () = nothing to compare


@dataclass(frozen=True)
class Sibling:
    """How auto_add.py writes a new id into the family's table (in "driver"), read only."""

    brand: str
    kind: str  # "keyboard" or "mouse"
    fallback: str  # the name when the request's Device model says nothing: {v} and {p} are the ids
    entry: str  # the Rust lines of one model: {vendor}, {product} (upper-case hex), {name}, {params}
    params: dict = field(default_factory=dict)  # vendor id -> the Params the model gets ("" -> "default")
    linux: str = ""  # what the answer says about Linux, when the family is not opened there


@dataclass(frozen=True)
class Platform:
    key: str  # the family in site/devices.json (or a name of its own when it has no table)
    name: str  # as the answers say it
    driver: str  # the Rust file the signature comes from
    collections: tuple[Collection, ...]  # any of them
    vendors: frozenset[str] = frozenset()  # USB vendor ids it is recognized for; empty: any
    sibling: Sibling | None = None
    # What happens to a device recognized by its collection alone, with no table to add it to ("found"): PeriHub
    # finds it by itself. None: a maintainer adds it.
    found_by_collection: bool = False


MODEL_BLOCK = """    Model {{
        vendor_id: 0x{vendor},
        product_ids: &[0x{product}],
        brand: "{brand}",
        name: "{name}",
        kind: Kind::{kind},
        interface: Some({interface}),
        usage_page: Some(0x{usage_page:04X}),
        protocol: Protocol::{protocol}({params}),
    }},
"""

PLATFORMS: tuple[Platform, ...] = (
    Platform(
        key="wraith",
        name="Artery W669 keyboards (Wraith W60, X68HE)",
        driver="crates/perihub-core/src/peripheral/wraith.rs",
        vendors=frozenset({"2e3c"}),
        # Interface 2, usage page ff1b, usage 0091, report 01 with 64 bytes each way: the W60 (#35) and the
        # X68HE (#123) declare it byte for byte.
        collections=(Collection(0xFF1B, 0x0091, 2, (Report(0x01, 64, 64),)),),
        sibling=Sibling(
            brand="Artery",
            kind="keyboard",
            fallback="W669 keyboard {v} {p}",
            entry=MODEL_BLOCK.replace("{interface}", "2").replace("{usage_page:04X}", "FF1B").replace("{protocol}", "Wraith")
            .replace("{kind}", "Keyboard").replace("{brand}", "Artery"),
            params={"": "Params"},
            linux="On Linux it is not opened, like the other keyboards of this family: its configuration collection "
            "can carry how far each key is pressed (Windows only for now).",
        ),
    ),
    Platform(
        key="evision",
        name="EVision keyboards",
        driver="crates/perihub-core/src/peripheral/evision.rs",
        vendors=frozenset({"320f", "0c45"}),
        # Interface 1, usage page ff1c (usage 0092 on the Surmen V400 of #80), report 04, 64 bytes each way.
        collections=(Collection(0xFF1C, None, 1, (Report(0x04, 64, 64),)),),
        sibling=Sibling(
            brand="EVision",
            kind="keyboard",
            fallback="RGB keyboard {v} {p}",
            entry='    keyboard(0x{vendor}, &[0x{product}], "{name}", {params}),\n',
            params={"": "READ"},
            linux="On Linux it is not opened, like every EVision keyboard: the vendor collection shares interface 1 "
            "with the keyboard (Windows only for now).",
        ),
    ),
    Platform(
        key="yjx",
        name="YJX mice",
        driver="crates/perihub-core/src/peripheral/yjx.rs",
        vendors=frozenset({"a8a4", "a8a5"}),
        # Interface 2, usage page ff01, usage 0010, 64-byte input and output reports without a number.
        collections=(Collection(0xFF01, 0x0010, 2, (Report(0x00, 64, 64),)),),
        sibling=Sibling(
            brand="YJX",
            kind="mouse",
            fallback="mouse {v} {p}",
            entry=MODEL_BLOCK.replace("{interface}", "2").replace("{usage_page:04X}", "FF01").replace("{protocol}", "Yjx")
            .replace("{kind}", "Mouse").replace("{brand}", "YJX"),
            # a8a5 is the receiver's vendor id, a8a4 the cable's.
            params={"a8a5": "Params { receiver: true }", "a8a4": "Params { receiver: false }"},
        ),
    ),
    Platform(
        key="logitech",
        name="Logitech HID++ devices",
        driver="crates/perihub-core/src/manager.rs",
        vendors=frozenset({"046d"}),
        # HID++ short (0001, report 10) and long (0002, report 11) collections on ff00, and the collections of their
        # own on ff43 (0202 over Bluetooth, 0602 on the G213). HID++ 2.0 describes its features: the Logitech driver
        # reads what the device has. The models are written by a person (memory formats, receivers).
        collections=(
            Collection(0xFF00, 0x0001),
            Collection(0xFF00, 0x0002),
            Collection(0xFF43, 0x0202),
            Collection(0xFF43, 0x0602),
        ),
    ),
    Platform(
        key="qmk",
        name="QMK keyboards with VIA or Vial",
        driver="crates/perihub-core/src/peripheral/qmk.rs",
        # Usage page ff60, usage 0061: QMK's raw HID channel, whatever the USB id.
        collections=(Collection(0xFF60, 0x0061),),
        found_by_collection=True,
    ),
)
BY_KEY = {p.key: p for p in PLATFORMS}

# "interface 2, usage page ff1b, usage 0091", or "no interface number, ..." (Bluetooth, macOS): interface -1.
COLLECTION_LINE = re.compile(
    r"^\s*(?:interface (-?\d+)|no interface number), usage page ([0-9a-f]{4}), usage ([0-9a-f]{4})\s*$", re.I | re.M
)
DESCRIPTOR_LINE = re.compile(r"^\s*(?:interface (-?\d+)|no interface number), \d+ bytes: (.+?)\s*$", re.I | re.M)
DESCRIPTOR_PART = re.compile(r"usage page ([0-9a-f]{4}), usage ([0-9a-f]{4}): (.*)", re.I)
REPORT = re.compile(r"(no id|id ([0-9a-f]{2})) \(([^)]*)\)", re.I)
LENGTH = re.compile(r"(in|out|feature) (\d+)")


def collections_of(saw: str) -> set[tuple[int, int, int]]:
    """(interface, usage page, usage) of every line of "HID collections" (and of "Report descriptors")."""
    found = {(int(i or -1), int(p, 16), int(u, 16)) for i, p, u in COLLECTION_LINE.findall(saw or "")}
    return found | set(descriptors_of(saw))


def descriptors_of(saw: str) -> dict[tuple[int, int, int], set[Report]]:
    """The reports of each collection of "Report descriptors", by (interface, usage page, usage)."""
    out: dict[tuple[int, int, int], set[Report]] = {}
    for interface, rest in DESCRIPTOR_LINE.findall(saw or ""):
        for part in rest.split("; "):
            m = DESCRIPTOR_PART.fullmatch(part.strip())
            if not m:
                continue
            reports = set()
            for whole, rid, lengths in REPORT.findall(m.group(3)):
                n = {k: int(v) for k, v in LENGTH.findall(lengths)}
                reports.add(Report(0 if whole.lower() == "no id" else int(rid, 16), n.get("in", 0), n.get("out", 0), n.get("feature", 0)))
            key = (int(interface or -1), int(m.group(1), 16), int(m.group(2), 16))
            out.setdefault(key, set()).update(reports)
    return out


def level(platform: Platform, usb: str, saw: str) -> str | None:
    """"full", "collection" or None: how well what the request shows matches this family."""
    if platform.vendors and usb.split(":")[0] not in platform.vendors:
        return None
    seen, described = collections_of(saw), descriptors_of(saw)
    best = None
    for c in platform.collections:
        for key in sorted(seen):
            interface, page, usage = key
            if page != c.usage_page or (c.usage is not None and usage != c.usage):
                continue
            if c.interface is not None and interface != c.interface:
                continue
            if not c.reports:
                return "full"
            if key in described:
                if set(c.reports) <= described[key]:
                    return "full"
                continue  # the descriptor is there and says otherwise
            best = "collection"
    return best


@dataclass(frozen=True)
class Match:
    platform: Platform
    level: str  # "full" or "collection"

    @property
    def auto(self) -> bool:
        """Whether auto_add.py adds the id by itself (read only)."""
        return self.level == "full" and self.platform.sibling is not None


def recognize(usb: str, saw: str) -> Match | None:
    """The one family whose collection the request shows, or None (none, or several)."""
    found = [(p, lvl) for p in PLATFORMS if (lvl := level(p, usb, saw))]
    full = [f for f in found if f[1] == "full"]
    pick = full if full else found
    return Match(*pick[0]) if len(pick) == 1 else None


def signature(platform: Platform) -> str:
    """The family's collection in words, for an answer: "interface 2, usage page ff1b, usage 0091, report id 01 (in 64, out 64 bytes)"."""
    c = platform.collections[0]
    parts = []
    if c.interface is not None:
        parts.append(f"interface {c.interface}")
    parts.append(f"usage page {c.usage_page:04x}")
    if c.usage is not None:
        parts.append(f"usage {c.usage:04x}")
    for r in c.reports:
        lengths = ", ".join(f"{k} {n}" for k, n in (("in", r.input), ("out", r.output), ("feature", r.feature)) if n)
        parts.append(("no report id" if r.id == 0 else f"report id {r.id:02x}") + f" ({lengths} bytes)")
    return ", ".join(parts)
