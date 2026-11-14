import type { Macro, MacroKind, MacroStep, MouseProfile, ProfileSettings, SoftMode, SoftStep, SoftwareMacro } from "./api";
import { CONSUMER_KEYS, keyLabel, mouseButtonName, MOUSE_CLICKS } from "./keys";

/** Kinds offered in the editor: "forever" exists in some mice's memory but cannot be stopped. */
export const MACRO_KINDS: { kind: MacroKind; label: string; hint: string }[] = [
  { kind: "once", label: "No repeat", hint: "Runs once each time you press the button." },
  {
    kind: "while_held",
    label: "Repeat while holding",
    hint: "Runs again and again while you hold the button down, and stops when you let go.",
  },
  {
    kind: "sequence",
    label: "Press and release",
    hint: "One part runs when you press the button, the other when you let go: for example, a key stays down for as long as you hold the button.",
  },
];

/** How a macro run by PeriHub repeats. */
export const SOFT_MODES: { type: SoftMode["type"]; label: string; hint: string }[] = [
  { type: "once", label: "Once", hint: "Runs once each time you press the button." },
  { type: "repeat", label: "Repeat", hint: "Runs the number of times you choose." },
  {
    type: "toggle",
    label: "Press again to stop",
    hint: "Starts with one press and repeats until you press the button again (or choose Stop software macros in the tray menu).",
  },
];

const KIND_LABELS: Record<MacroKind, string> = {
  once: "No repeat",
  while_held: "Repeat while holding",
  sequence: "Press and release",
  forever: "Repeats until the profile changes",
};

/** Modifier keys by their bit in a HID keyboard report. */
export const MODIFIER_KEYS: { bit: number; label: string }[] = [
  { bit: 0x01, label: "Ctrl" },
  { bit: 0x02, label: "Shift" },
  { bit: 0x04, label: "Alt" },
  { bit: 0x08, label: "Win" },
  { bit: 0x10, label: "R Ctrl" },
  { bit: 0x20, label: "R Shift" },
  { bit: 0x40, label: "Alt Gr" },
  { bit: 0x80, label: "R Win" },
];

export const DEFAULT_DELAY_MS = 50;
export const MAX_DELAY_MS = 65535;

function keyName(modifiers: number, key: number): string {
  // G HUB records a modifier key with its own bit set too: Ctrl down is key Ctrl + modifier Ctrl.
  const own = key >= 0xe0 && key <= 0xe7 ? 1 << (key - 0xe0) : 0;
  const mods = MODIFIER_KEYS.filter((m) => modifiers & m.bit & ~own).map((m) => m.label);
  return [...mods, ...(key ? [keyLabel(key)] : [])].join("+");
}

function buttonsName(buttons: number): string {
  const names = MOUSE_CLICKS.filter((c) => buttons & (1 << (c.button - 1))).map((c) => c.label);
  return names.join("+") || `mouse buttons 0x${buttons.toString(16)}`;
}

function consumerName(usage: number): string {
  return CONSUMER_KEYS.find((c) => c.usage === usage)?.label ?? `media key 0x${usage.toString(16)}`;
}

/** A step as shown in the editor, e.g. "Press Ctrl+C". */
export function stepLabel(s: SoftStep): string {
  switch (s.type) {
    case "text": return `Type “${s.text}”`;
    case "open": return `Open ${s.target}`;
    case "key_down": return `Press ${keyName(s.modifiers, s.key)}`;
    case "key_up": return `Release ${keyName(s.modifiers, s.key)}`;
    case "mouse_down": return `Press ${buttonsName(s.buttons)}`;
    case "mouse_up": return `Release ${buttonsName(s.buttons)}`;
    case "consumer_down": return `Press ${consumerName(s.usage)}`;
    case "consumer_up": return `Release ${consumerName(s.usage)}`;
    case "wheel": return s.amount > 0 ? `Scroll up ${s.amount}` : `Scroll down ${-s.amount}`;
    case "delay": return `Wait ${s.ms} ms`;
    case "noop": return "Short pause";
  }
}

/** Bytes a macro takes in the mouse's memory (a long one also needs a few bytes to continue in
 * another sector). */
export function macroSize(m: Macro): number {
  const step = (s: MacroStep) => (s.type === "noop" ? 1 : s.type === "wheel" ? 2 : 3);
  const steps = [...m.press, ...m.release].reduce((sum, s) => sum + step(s), 0);
  // The end command, and the release or repeat command between the parts.
  return steps + 1 + (m.kind === "once" ? 0 : 1);
}

/** One line for the buttons list, e.g. "Repeat while holding: Ctrl+C V · 120 ms". */
export function macroSummary(m: Macro | null): string {
  if (!m) return "Macro that PeriHub cannot read";
  const steps = [...m.press, ...m.release];
  const pressed = steps.flatMap((s) => {
    switch (s.type) {
      case "key_down": return [keyName(s.modifiers, s.key)];
      case "mouse_down": return [buttonsName(s.buttons)];
      case "consumer_down": return [consumerName(s.usage)];
      case "wheel": return [s.amount > 0 ? "Scroll up" : "Scroll down"];
      default: return [];
    }
  });
  const shown = pressed.length > 10 ? [...pressed.slice(0, 10), "…"] : pressed;
  const ms = steps.reduce((sum, s) => sum + (s.type === "delay" ? s.ms : 0), 0);
  return `${KIND_LABELS[m.kind]}: ${shown.join(" ") || "no keys"} · ${ms} ms`;
}

/** A macro already on a button, offered as a starting point in the editor. */
export interface MacroSource {
  label: string;
  content: Macro;
}

/** Readable macros on every button of every profile, each content once. `current` replaces the
 * saved version of the profile being edited, so its unsaved macros are offered too. */
export function macroLibrary(productId: number, profiles: MouseProfile[], current: MouseProfile): MacroSource[] {
  const sources: MacroSource[] = [];
  const seen = new Set<string>();
  for (const profile of profiles.map((p) => (p.number === current.number ? current : p))) {
    const layers: [string, ProfileSettings["buttons"]][] = [["", profile.settings.buttons], [" (G-Shift)", profile.settings.gshift_buttons]];
    for (const [layer, buttons] of layers) {
      buttons.forEach((action, i) => {
        if (action.type !== "macro" || !action.content) return;
        const key = JSON.stringify(action.content);
        if (seen.has(key)) return;
        seen.add(key);
        const where = `Profile ${profile.number} · ${mouseButtonName(productId, i)}${layer}`;
        sources.push({ label: `${where} · ${macroSummary(action.content)}`, content: action.content });
      });
    }
  }
  return sources;
}

// ---- Recording ----

const CODE_MODIFIERS: Record<string, number> = {
  ControlLeft: 0x01, ShiftLeft: 0x02, AltLeft: 0x04, MetaLeft: 0x08,
  ControlRight: 0x10, ShiftRight: 0x20, AltRight: 0x40, MetaRight: 0x80,
};

/** KeyboardEvent.code → HID usage. The code names the physical key, so recording works on any layout. */
const CODE_USAGES: Record<string, number> = {
  Enter: 0x28, Escape: 0x29, Backspace: 0x2a, Tab: 0x2b, Space: 0x2c, Minus: 0x2d, Equal: 0x2e,
  BracketLeft: 0x2f, BracketRight: 0x30, Backslash: 0x31, IntlHash: 0x32, Semicolon: 0x33, Quote: 0x34,
  Backquote: 0x35, Comma: 0x36, Period: 0x37, Slash: 0x38, CapsLock: 0x39, PrintScreen: 0x46,
  ScrollLock: 0x47, Pause: 0x48, Insert: 0x49, Home: 0x4a, PageUp: 0x4b, Delete: 0x4c, End: 0x4d,
  PageDown: 0x4e, ArrowRight: 0x4f, ArrowLeft: 0x50, ArrowDown: 0x51, ArrowUp: 0x52, NumLock: 0x53,
  NumpadDivide: 0x54, NumpadMultiply: 0x55, NumpadSubtract: 0x56, NumpadAdd: 0x57, NumpadEnter: 0x58,
  Numpad0: 0x62, NumpadDecimal: 0x63, IntlBackslash: 0x64, ContextMenu: 0x65, NumpadEqual: 0x67,
  NumpadComma: 0x85, IntlRo: 0x87, KanaMode: 0x88, IntlYen: 0x89, Convert: 0x8a, NonConvert: 0x8b,
  Lang1: 0x90, Lang2: 0x91,
};
for (let i = 0; i < 26; i++) CODE_USAGES[`Key${String.fromCharCode(65 + i)}`] = 0x04 + i;
for (let i = 1; i <= 9; i++) {
  CODE_USAGES[`Digit${i}`] = 0x1e + i - 1;
  CODE_USAGES[`Numpad${i}`] = 0x59 + i - 1;
}
CODE_USAGES.Digit0 = 0x27;
for (let i = 1; i <= 12; i++) CODE_USAGES[`F${i}`] = 0x3a + i - 1;
for (let i = 13; i <= 24; i++) CODE_USAGES[`F${i}`] = 0x68 + i - 13;

/** The step for a key event while recording, or null for keys a macro cannot press. */
export function recordedStep(code: string, down: boolean): MacroStep | null {
  const type = down ? "key_down" : "key_up";
  const bit = CODE_MODIFIERS[code];
  if (bit) return { type, modifiers: bit, key: 0 };
  const usage = CODE_USAGES[code];
  return usage ? { type, modifiers: 0, key: usage } : null;
}

/** One line for a macro run by PeriHub, e.g. "Press again to stop: Ctrl+C · Type “gg”". */
export function softSummary(m: SoftwareMacro): string {
  const mode = m.mode.type === "repeat" ? `Repeat ${m.mode.times} times` : SOFT_MODES.find((x) => x.type === m.mode.type)!.label;
  const parts = m.steps.flatMap((s) => {
    switch (s.type) {
      case "key_down": case "mouse_down": case "consumer_down": return [stepLabel(s).replace(/^Press /, "")];
      case "wheel": case "text": case "open": return [stepLabel(s)];
      default: return [];
    }
  });
  const shown = parts.length > 6 ? [...parts.slice(0, 6), "…"] : parts;
  return `${mode}: ${shown.join(" ") || "no steps"}`;
}

/** The release that matches a press step (to close keys still held when recording stops). */
export function releaseOf(step: SoftStep): MacroStep | null {
  switch (step.type) {
    case "key_down": return { type: "key_up", modifiers: step.modifiers, key: step.key };
    case "mouse_down": return { type: "mouse_up", buttons: step.buttons };
    case "consumer_down": return { type: "consumer_up", usage: step.usage };
    default: return null;
  }
}
