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

/** The effects PeriHub sets on a keyboard it drives (0x8071 ids), in the core's `host::effect`. */
export const EFFECT = { off: 0x00, fixed: 0x01, breathing: 0x0a, cycle: 0x15, wave: 0x16, ripple: 0x17 } as const;
const SET_EFFECTS: number[] = Object.values(EFFECT);

/** PeriHub can set this effect. */
export function settableEffect(id: number): boolean {
  return SET_EFFECTS.includes(id);
}

/** Effects with a colour of their own, and with a period. */
export function effectTakesColor(id: number): boolean {
  return id === EFFECT.fixed || id === EFFECT.breathing || id === EFFECT.ripple;
}
export function effectTakesPeriod(id: number): boolean {
  return id === EFFECT.breathing || id === EFFECT.cycle || id === EFFECT.wave;
}

/** The directions of a wave, with the 0x8071 capability bit that says the keyboard has each. */
const DIRECTIONS: [number, string, number][] = [
  [1, "Left to right", 0x0020],
  [6, "Right to left", 0x0400],
  [2, "Top to bottom", 0x0040],
  [7, "Bottom to top", 0x0800],
  [3, "From the centre", 0x0080],
  [8, "To the centre", 0x1000],
  [4, "Inward", 0x0100],
  [5, "Outward", 0x0200],
];

/** The directions a wave with capabilities `caps` can take, the keyboard's default first. */
export function waveDirections(caps: number): { value: number; label: string }[] {
  return [
    { value: 0, label: "The keyboard's default" },
    ...DIRECTIONS.filter(([, , bit]) => caps & bit).map(([value, label]) => ({ value, label })),
  ];
}

/** PeriHub can drive the device's lights in host mode, as in the core's `lights_drivable`: it lists
 * 0x8071 and 0x8101, PeriHub read its zones, and PeriHub does not change its DPI. */
export function lightsDrivable(device: UnsupportedReceiverDevice): boolean {
  const ids = new Set(device.features.map((f) => f.id));
  const dpiChangeable = !!device.dpi && !device.onboard;
  return ids.has(0x8071) && ids.has(0x8101) && !!device.lighting?.effects?.zones.length && !dpiChangeable;
}

/** Groups of keys by their 0x8081 ids (keyboard usage codes minus 3, modifiers minus 0x78, and
 * Logitech's own ids for media, G and logo keys). */
const KEY_GROUPS: [string, (id: number) => boolean][] = [
  ["Letters", (id) => id >= 1 && id <= 26],
  ["Numbers", (id) => id >= 27 && id <= 36],
  ["F keys", (id) => id >= 55 && id <= 66],
  ["Arrows and navigation", (id) => id >= 67 && id <= 79],
  ["Numpad", (id) => id >= 80 && id <= 96],
  ["Ctrl, Shift, Alt and Windows", (id) => id >= 104 && id <= 111],
  ["Media and brightness", (id) => id >= 150 && id <= 160],
  ["G keys", (id) => id >= 180 && id <= 188],
  ["Logo", (id) => id === 210],
];

/** The logo's key id (0x8081). */
export const LOGO_KEY = 210;

/** The keys "Colours per key" sets: all those with a colour of their own, except the logo when the
 * keyboard has a logo zone. The logo then follows that zone's effect (a per-key colour held it on one
 * colour, issue #30), and PeriHub gives its key the zone's colour while the zone is fixed. */
export function perKeyKeys(lighting: { keys: number[]; effects: { zones: { location: number }[] } | null }): number[] {
  const logoZone = lighting.effects?.zones.some((z) => z.location === 2) ?? false;
  return logoZone ? lighting.keys.filter((k) => k !== LOGO_KEY) : lighting.keys;
}

/** The keyboard's keys with their own colour, in groups; the keys of no group go in "Other keys". */
export function keyGroups(keys: number[]): { name: string; keys: number[] }[] {
  const groups = KEY_GROUPS.map(([name, has]) => ({ name, keys: keys.filter(has) }));
  const others = keys.filter((id) => !KEY_GROUPS.some(([, has]) => has(id)));
  groups.splice(5, 0, { name: "Other keys", keys: others });
  return groups.filter((g) => g.keys.length);
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
