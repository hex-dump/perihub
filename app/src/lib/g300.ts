// How the G300's profiles are written in words: on its page and in its test report.
import type { G300Button, G300Profile } from "./api";
import { keyLabel } from "./keys";

/** G4 to G9 are printed on the mouse; 1 to 3 are the clicks. */
export const BUTTON_NAMES = ["Left button", "Right button", "Wheel button", "G4", "G5", "G6", "G7", "G8", "G9"];
const MODIFIER_NAMES = ["Ctrl", "Shift", "Alt", "Win", "Right Ctrl", "Right Shift", "Right Alt", "Right Win"];
const MOUSE_NAMES: Record<number, string> = {
  1: "Left click",
  2: "Right click",
  3: "Middle click",
  4: "Back",
  5: "Forward",
};

export function action(b: G300Button): string {
  switch (b.type) {
    case "none":
      return "Nothing";
    case "mouse":
      return MOUSE_NAMES[b.button] ?? `Mouse button ${b.button}`;
    case "dpi_up":
      return "DPI +";
    case "dpi_down":
      return "DPI –";
    case "dpi_cycle":
      return "Cycle DPI";
    case "profile_cycle":
      return "Next profile";
    case "dpi_shift":
      return "DPI shift (hold)";
    case "dpi_default":
      return "Default DPI";
    case "key": {
      const mods = MODIFIER_NAMES.filter((_, i) => b.modifiers & (1 << i));
      return [...mods, ...(b.key ? [keyLabel(b.key)] : [])].join(" + ");
    }
    case "unknown":
      return `Unknown (${b.code.toString(16)})`;
  }
}

export function colorName(color: string): string {
  const names: Record<string, string> = {
    "#ffffff": "White",
    "#ff0000": "Red",
    "#00ff00": "Green",
    "#0000ff": "Blue",
    "#ffff00": "Yellow",
    "#00ffff": "Cyan",
    "#ff00ff": "Magenta",
    "#000000": "Off",
  };
  return names[color.toLowerCase()] ?? color;
}

export function describe(p: G300Profile): string {
  const levels = p.dpi_levels.map((d, i) => (i === p.default_level ? `${d}*` : `${d}`)).join(" / ");
  const buttons = p.buttons.map((b, i) => `${BUTTON_NAMES[i]}: ${action(b)}`).join("; ");
  const shift = p.shift_dpi ? `, shift ${p.shift_dpi}` : "";
  return `${colorName(p.color)}, ${p.report_rate_hz ?? "?"} Hz, DPI ${levels}${shift}, ${buttons}`;
}
