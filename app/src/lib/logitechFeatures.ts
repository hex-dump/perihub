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
  ["Programmable buttons and keys", [0x1b04]],
  ["Lighting", [0x8070, 0x8071]],
  ["Keyboard backlight", [0x1981, 0x1982]],
  ["Fn key swap", [0x40a0, 0x40a2, 0x40a3]],
  ["Easy-Switch between computers", [0x1814]],
  ["Onboard profiles", [0x8100]],
];

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
