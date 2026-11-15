// What a Logitech HID++ 2.0 device tells about itself, in words: its kind and the features PeriHub
// names on its page. Feature ids are facts reported by the devices themselves; the kinds come from
// Logitech's HID++ 2.0 specification (0x0005 function 2).

import type { UnsupportedReceiverDevice } from "./api";

const KINDS = ["Keyboard", "Remote control", "Numpad", "Mouse", "Touchpad", "Trackball", "Presenter", "Receiver"];

/** "Mouse", "Keyboard"…, or "Type 9" for a kind the specification does not list. */
export function kindName(kind: number | null): string {
  if (kind === null) return "Device";
  return KINDS[kind] ?? `Type ${kind}`;
}

/** The sidebar icon: keyboards and numpads as keyboards, everything else as a mouse. */
export function kindIcon(kind: number | null): "keyboard" | "mouse" {
  return kind === 0 || kind === 2 ? "keyboard" : "mouse";
}

/** Features grouped by what they let the user do. One device often has two ids for the same thing. */
const CAPABILITIES: [string, number[]][] = [
  ["Battery level", [0x1000, 0x1001, 0x1004]],
  ["Adjustable DPI", [0x2201, 0x2202]],
  ["Polling rate", [0x8060, 0x8061]],
  ["SmartShift wheel", [0x2110, 0x2111]],
  ["High-resolution wheel", [0x2121]],
  ["Thumb wheel", [0x2150]],
  ["Programmable buttons and keys", [0x1b04, 0x1b05]],
  ["Lighting", [0x8070, 0x8071]],
  ["Per-key lighting", [0x8080, 0x8081]],
  ["Lighting brightness", [0x8040]],
  ["Game mode (keys turned off)", [0x4521, 0x4522, 0x4523]],
  ["Keyboard backlight", [0x1981, 0x1982]],
  ["Fn key swap", [0x40a0, 0x40a2, 0x40a3]],
  ["Easy-Switch between computers", [0x1814]],
  ["Onboard profiles", [0x8100, 0x8101]],
];

/** Effects of 0x8071, by the id the device reports for each (the names are Solaar's, from the
 * devices it supports). */
const EFFECTS: Record<number, string> = {
  0x00: "Off",
  0x01: "Fixed colour",
  0x02: "Pulse",
  0x03: "Colour cycle",
  0x04: "Colour wave",
  0x08: "Startup effect",
  0x09: "Demo",
  0x0a: "Breathing",
  0x0b: "Ripple",
  0x0e: "Decomposition",
  0x0f: "Signature 1",
  0x10: "Signature 2",
  0x15: "Colour cycle",
  0x16: "Colour wave",
  0x17: "Ripple",
};

/** The name of a 0x8071 effect, or "Effect 0x0012" for one PeriHub does not know. */
export function effectName(id: number): string {
  return EFFECTS[id] ?? `Effect 0x${id.toString(16).padStart(4, "0")}`;
}

/** Where a 0x8071 lighting zone is, from the location the device reports. */
export function zoneName(location: number, kind: number | null): string {
  const main = kind === 0 || kind === 2 ? "Keys" : "Main lighting";
  if (location === 1) return main;
  if (location >= 6 && location <= 11) return `${main} ${location - 5}`;
  return ["Zone", "", "Logo", "Left side", "Right side", "All lights"][location] ?? `Zone at ${location}`;
}

/** The keyboard layout 0x4540 reports. Only codes seen on a real keyboard get a name: Solaar reads 4
 * as AZERTY, but a German G915 X LS answers 4 (issue #30). */
export function layoutName(code: number): string {
  if (code === 4) return "German, ISO (code 4)";
  return `code ${code}`;
}

/** What the device can do, in words, in a fixed order, and how many other features it lists. */
export function capabilities(device: UnsupportedReceiverDevice): { named: string[]; others: number } {
  const ids = new Set(device.features.map((f) => f.id));
  const named = CAPABILITIES.filter(([, list]) => list.some((id) => ids.has(id))).map(([name]) => name);
  const known = new Set(CAPABILITIES.flatMap(([, list]) => list));
  // The root, feature table, firmware, name and the device's hidden features say nothing to the user.
  const basic = new Set([0x0000, 0x0001, 0x0002, 0x0003, 0x0005]);
  const others = device.features.filter((f) => !known.has(f.id) && !basic.has(f.id) && !(f.kind & 0x40)).length;
  return { named, others };
}
