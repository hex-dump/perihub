import type { KeyBinding, SpecialFunction } from "./api";
import { prefs, type LayoutId } from "./prefs.svelte";

/** Key names that do not depend on the layout (HID usage → keycap label). */
const NAMES: Record<number, string> = {
  0x28: "Enter", 0x29: "Esc", 0x2a: "⌫", 0x2b: "Tab", 0x2c: "Space", 0x39: "Caps Lock",
  0x46: "PrtSc", 0x47: "ScrLk", 0x48: "Pause", 0x49: "Ins", 0x4a: "Home", 0x4b: "PgUp",
  0x4c: "Del", 0x4d: "End", 0x4e: "PgDn", 0x4f: "→", 0x50: "←", 0x51: "↓", 0x52: "↑",
  0x53: "Num Lock", 0x54: "Num /", 0x55: "Num *", 0x56: "Num -", 0x57: "Num +", 0x58: "Num Enter",
  0x63: "Num .", 0x65: "Menu", 0x67: "Num =", 0x85: "Num ,",
  0x87: "Intl 1", 0x88: "Intl 2", 0x89: "Intl 3", 0x8a: "Intl 4", 0x8b: "Intl 5", 0x90: "Lang 1", 0x91: "Lang 2",
  0xe0: "Ctrl", 0xe1: "Shift", 0xe2: "Alt", 0xe3: "Win", 0xe4: "R Ctrl", 0xe5: "R Shift",
  0xe7: "R Win", 0xf0: "SteelSeries",
};

/** Keys that every ISO layout shares (the ANSI-only key does not exist on an ISO board). */
const ISO = { 0x31: "ANSI \\", 0x64: "<", 0xe6: "Alt Gr" };

/** Symbols that change with the layout, as printed on the keycaps (from the xkb layouts). */
const LEGENDS: Record<LayoutId, Record<number, string>> = {
  "ansi-us": {
    0x2d: "-", 0x2e: "=", 0x2f: "[", 0x30: "]", 0x31: "\\", 0x32: "ISO #", 0x33: ";", 0x34: "'",
    0x35: "`", 0x36: ",", 0x37: ".", 0x38: "/", 0x64: "ISO \\", 0xe6: "R Alt",
  },
  "iso-uk": {
    0x2d: "-", 0x2e: "=", 0x2f: "[", 0x30: "]", 0x31: "ANSI \\", 0x32: "#", 0x33: ";", 0x34: "'",
    0x35: "`", 0x36: ",", 0x37: ".", 0x38: "/", 0x64: "\\", 0xe6: "Alt Gr",
  },
  "iso-it": {
    0x2d: "'", 0x2e: "ì", 0x2f: "è", 0x30: "+", 0x31: "ANSI \\", 0x32: "ù", 0x33: "ò", 0x34: "à",
    0x35: "\\", 0x36: ",", 0x37: ".", 0x38: "-", 0x64: "<", 0xe6: "Alt Gr",
  },
  "iso-de": {
    ...ISO, 0x1c: "Z", 0x1d: "Y", 0x2d: "ß", 0x2e: "´", 0x2f: "Ü", 0x30: "+", 0x32: "#", 0x33: "Ö", 0x34: "Ä",
    0x35: "^", 0x36: ",", 0x37: ".", 0x38: "-",
  },
  "iso-fr": {
    ...ISO, 0x04: "Q", 0x10: ",", 0x14: "A", 0x1a: "Z", 0x1d: "W", 0x2d: ")", 0x2e: "=", 0x2f: "^", 0x30: "$",
    0x32: "*", 0x33: "M", 0x34: "ù", 0x35: "²", 0x36: ";", 0x37: ":", 0x38: "!",
  },
  "iso-es": {
    ...ISO, 0x2d: "'", 0x2e: "¡", 0x2f: "`", 0x30: "+", 0x32: "Ç", 0x33: "Ñ", 0x34: "´", 0x35: "º", 0x36: ",",
    0x37: ".", 0x38: "-",
  },
  "iso-pt": {
    ...ISO, 0x2d: "'", 0x2e: "«", 0x2f: "+", 0x30: "´", 0x32: "~", 0x33: "Ç", 0x34: "º", 0x35: "\\", 0x36: ",",
    0x37: ".", 0x38: "-",
  },
  "iso-se": {
    ...ISO, 0x2d: "+", 0x2e: "´", 0x2f: "Å", 0x30: "¨", 0x32: "'", 0x33: "Ö", 0x34: "Ä", 0x35: "§", 0x36: ",",
    0x37: ".", 0x38: "-",
  },
  "iso-no": {
    ...ISO, 0x2d: "+", 0x2e: "\\", 0x2f: "Å", 0x30: "¨", 0x32: "'", 0x33: "Ø", 0x34: "Æ", 0x35: "|", 0x36: ",",
    0x37: ".", 0x38: "-",
  },
  "iso-dk": {
    ...ISO, 0x2d: "+", 0x2e: "´", 0x2f: "Å", 0x30: "¨", 0x32: "'", 0x33: "Æ", 0x34: "Ø", 0x35: "½", 0x36: ",",
    0x37: ".", 0x38: "-",
  },
};

export function keyLabel(usage: number, layout: LayoutId = prefs.layout): string {
  const legend = LEGENDS[layout][usage] ?? NAMES[usage];
  if (legend) return legend;
  if (usage >= 0x04 && usage <= 0x1d) return String.fromCharCode(65 + usage - 0x04);
  if (usage >= 0x1e && usage <= 0x26) return String(usage - 0x1d);
  if (usage === 0x27) return "0";
  if (usage >= 0x3a && usage <= 0x45) return `F${usage - 0x39}`;
  if (usage >= 0x68 && usage <= 0x73) return `F${usage - 0x68 + 13}`;
  if (usage >= 0x59 && usage <= 0x61) return `Num ${usage - 0x58}`;
  if (usage === 0x62) return "Num 0";
  return `0x${usage.toString(16).padStart(2, "0")}`;
}

/** Factory functions of the SteelSeries layer (1 and 2 confirmed by pressing them on a real keyboard). */
export function functionLabel(code: number): string {
  switch (code) {
    case 1: return ""; // F9: does nothing, so the key shows no label
    case 2: return "Mic mute (F10)";
    case 3: return "Brightness –";
    case 4: return "Brightness +";
    case 5: return "Win key lock";
    default: return `Factory function ${code}`;
  }
}

export function bindingLabel(binding: KeyBinding | undefined): string {
  if (!binding) return "";
  switch (binding.type) {
    case "key": return keyLabel(binding.usage);
    case "function": return functionLabel(binding.code);
    case "none": return "";
    case "raw": return "?";
  }
}

export interface KeyCap {
  usage: number;
  x: number;
  y: number;
  w: number;
  h: number;
  /** Key that exists on the keyboard but cannot be remapped by the firmware. */
  locked?: boolean;
}

type Row = (number | [number, number] | { gap: number } | { lock: number; w: number })[];

function row(y: number, items: Row, startX = 0): KeyCap[] {
  const caps: KeyCap[] = [];
  let x = startX;
  for (const item of items) {
    if (typeof item === "number") {
      caps.push({ usage: item, x, y, w: 1, h: 1 });
      x += 1;
    } else if (Array.isArray(item)) {
      caps.push({ usage: item[0], x, y, w: item[1], h: 1 });
      x += item[1];
    } else if ("gap" in item) {
      x += item.gap;
    } else {
      caps.push({ usage: item.lock, x, y, w: item.w, h: 1, locked: true });
      x += item.w;
    }
  }
  return caps;
}

// Rows that are the same in ANSI and ISO, in key units, as on the real keyboard: there are no
// Print Screen, Scroll Lock and Pause keys (the volume roller sits there), and the bottom row ends
// with Alt, Win, SteelSeries and Ctrl.
const FUNCTION_ROW = row(0, [0x29, { gap: 1 }, 0x3a, 0x3b, 0x3c, 0x3d, { gap: 0.5 }, 0x3e, 0x3f, 0x40, 0x41,
  { gap: 0.5 }, 0x42, 0x43, 0x44, 0x45]);
const NUMBER_ROW = row(1.25, [0x35, 0x1e, 0x1f, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x2d, 0x2e,
  [0x2a, 2], { gap: 0.25 }, 0x49, 0x4a, 0x4b]);
const NAV_ROW = row(2.25, [0x4c, 0x4d, 0x4e], 15.25);
const BOTTOM_ROW = row(5.25, [[0xe0, 1.25], [0xe3, 1.25], [0xe2, 1.25], [0x2c, 6.25], [0xe6, 1.25], [0xe7, 1.25],
  [0xf0, 1.25], [0xe4, 1.25], { gap: 0.25 }, 0x50, 0x51, 0x4f]);
const LETTERS_TOP = [0x14, 0x1a, 0x08, 0x15, 0x17, 0x1c, 0x18, 0x0c, 0x12, 0x13, 0x2f, 0x30];
const LETTERS_HOME = [0x04, 0x16, 0x07, 0x09, 0x0a, 0x0b, 0x0d, 0x0e, 0x0f, 0x33, 0x34];
const LETTERS_BOTTOM = [0x1d, 0x1b, 0x06, 0x19, 0x05, 0x11, 0x10, 0x36, 0x37, 0x38];

/** TKL ANSI: single-row Enter, wide left Shift, backslash above Enter. */
const TKL_ANSI: KeyCap[] = [
  ...FUNCTION_ROW,
  ...NUMBER_ROW,
  ...row(2.25, [[0x2b, 1.5], ...LETTERS_TOP, [0x31, 1.5]]),
  ...NAV_ROW,
  ...row(3.25, [[0x39, 1.75], ...LETTERS_HOME, [0x28, 2.25]]),
  ...row(4.25, [[0xe1, 2.25], ...LETTERS_BOTTOM, [0xe5, 2.75], { gap: 1.25 }, 0x52]),
  ...BOTTOM_ROW,
];

/** TKL ISO: two-row Enter, extra key next to the left Shift. */
const TKL_ISO: KeyCap[] = [
  ...FUNCTION_ROW,
  ...NUMBER_ROW,
  ...row(2.25, [[0x2b, 1.5], ...LETTERS_TOP]),
  { usage: 0x28, x: 13.75, y: 2.25, w: 1.25, h: 2 },
  ...NAV_ROW,
  ...row(3.25, [[0x39, 1.75], ...LETTERS_HOME, 0x32]),
  ...row(4.25, [[0xe1, 1.25], 0x64, ...LETTERS_BOTTOM, [0xe5, 2.75], { gap: 1.25 }, 0x52]),
  ...BOTTOM_ROW,
];

export function layoutCaps(layout: LayoutId): KeyCap[] {
  return layout === "ansi-us" ? TKL_ANSI : TKL_ISO;
}

export const LAYOUT_WIDTH = 18.25;
export const LAYOUT_HEIGHT = 6.25;

// ---- Mouse ----

export const G502_HERO = 0xc08b;
export const G502_X_WIRELESS = 0xc098;

export function isG502(productId: number): boolean {
  return productId === G502_HERO || productId === G502_X_WIRELESS;
}

/** Labels of the G502 family buttons; other models get generic names. */
const G502_BUTTON_NAMES = [
  "Left click",
  "Right click",
  "Wheel click",
  "Back",
  "Forward",
  "Sniper (DPI Shift)",
  "DPI –",
  "DPI +",
  "Profile switch",
  "Wheel tilt right",
  "Wheel tilt left",
];

export function mouseButtonName(productId: number, index: number): string {
  return (isG502(productId) ? G502_BUTTON_NAMES[index] : undefined) ?? `Button ${index + 1}`;
}

export const MOUSE_CLICKS: { button: number; label: string }[] = [
  { button: 1, label: "Left click" },
  { button: 2, label: "Right click" },
  { button: 3, label: "Middle click" },
  { button: 4, label: "Back" },
  { button: 5, label: "Forward" },
];

export const SPECIAL_FUNCTIONS: { value: SpecialFunction; label: string }[] = [
  { value: "dpi_up", label: "DPI +" },
  { value: "dpi_down", label: "DPI –" },
  { value: "dpi_cycle", label: "Cycle DPI" },
  { value: "dpi_default", label: "Default DPI" },
  { value: "dpi_shift", label: "Sniper (DPI Shift)" },
  { value: "profile_next", label: "Next profile" },
  { value: "profile_prev", label: "Previous profile" },
  { value: "profile_cycle", label: "Cycle profiles" },
  { value: "g_shift", label: "G-Shift" },
  { value: "tilt_left", label: "Scroll left" },
  { value: "tilt_right", label: "Scroll right" },
];

export const CONSUMER_KEYS: { usage: number; label: string }[] = [
  { usage: 0xcd, label: "Play / Pause" },
  { usage: 0xb5, label: "Next track" },
  { usage: 0xb6, label: "Previous track" },
  { usage: 0xb7, label: "Stop" },
  { usage: 0xe2, label: "Mute" },
  { usage: 0xe9, label: "Volume +" },
  { usage: 0xea, label: "Volume –" },
  { usage: 0x192, label: "Calculator" },
  { usage: 0x194, label: "File explorer" },
  { usage: 0x18a, label: "Mail" },
  { usage: 0x223, label: "Browser: home" },
  { usage: 0x224, label: "Browser: back" },
  { usage: 0x225, label: "Browser: forward" },
  { usage: 0x227, label: "Browser: refresh" },
];

export const MODIFIERS: { bit: number; label: string }[] = [
  { bit: 0x01, label: "Ctrl" },
  { bit: 0x02, label: "Shift" },
  { bit: 0x04, label: "Alt" },
  { bit: 0x08, label: "Win" },
  { bit: 0x40, label: "Alt Gr" },
];

/** LED zone names by HID++ location (1 = primary, 2 = logo). */
export function ledZoneName(productId: number, location: number): string {
  if (productId === G502_HERO && location === 1) return "DPI indicator";
  if (productId === G502_HERO && location === 2) return "G logo";
  if (location === 1) return "Main lighting";
  if (location === 2) return "Logo";
  return `Zone ${location}`;
}

export function reportRateLabel(ms: number): string {
  return `${Math.round(1000 / ms)} Hz`;
}
