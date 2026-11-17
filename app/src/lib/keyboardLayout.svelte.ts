// The layout drawn for a keyboard: its shape (ANSI, ISO, JIS, ABNT2), read from the keyboard where its
// protocol has a read for it, and its legends, from the layout the system types with
// (crates/perihub-core/src/kblayout.rs). The user can set either one for a keyboard when detection is
// wrong; the choice is kept in the embedded browser's storage.
import { api, type DeviceLayout, type Legends, type Physical, type SystemLayout } from "./api";

export type { DeviceLayout, Legends, Physical, SystemLayout };

/** What the user set for one keyboard. */
export interface LayoutChoice {
  physical?: Physical;
  legends?: Legends;
}

export const PHYSICALS: { id: Physical; label: string }[] = [
  { id: "ansi", label: "ANSI" },
  { id: "iso", label: "ISO" },
  { id: "jis", label: "JIS" },
  { id: "abnt", label: "ABNT2" },
];

export const LEGEND_SETS: { id: Legends; label: string }[] = [
  { id: "us", label: "US English" },
  { id: "uk", label: "UK English" },
  { id: "it", label: "Italian" },
  { id: "de", label: "German" },
  { id: "fr", label: "French" },
  { id: "es", label: "Spanish" },
  { id: "pt", label: "Portuguese" },
  { id: "se", label: "Swedish/Finnish" },
  { id: "no", label: "Norwegian" },
  { id: "dk", label: "Danish" },
  { id: "jp", label: "Japanese" },
  { id: "br", label: "Brazilian" },
];

/** Where a half of the layout comes from. */
export type From = "you" | "keyboard" | "system" | "language" | "default";

export interface KeyboardLayout {
  physical: Physical;
  legends: Legends;
  physicalFrom: From;
  legendsFrom: From;
}

/** The Apex 3 TKL, the keyboard whose keys PeriHub draws. */
export const APEX_3_TKL = "1038:1622";

const CHOICES_KEY = "perihub.keyboardLayouts";
/** Before 0.10 one layout, chosen on the Apex 3 TKL's Keys tab, was kept for every keyboard. */
const OLD_KEY = "perihub.layout";

const isPhysical = (v: unknown): v is Physical => PHYSICALS.some((p) => p.id === v);
const isLegends = (v: unknown): v is Legends => LEGEND_SETS.some((l) => l.id === v);

function loadChoices(): Record<string, LayoutChoice> {
  const choices: Record<string, LayoutChoice> = {};
  try {
    const saved: unknown = JSON.parse(localStorage.getItem(CHOICES_KEY) ?? "{}");
    if (saved && typeof saved === "object") {
      for (const [key, value] of Object.entries(saved as Record<string, LayoutChoice>)) {
        const choice: LayoutChoice = {};
        if (isPhysical(value?.physical)) choice.physical = value.physical;
        if (isLegends(value?.legends)) choice.legends = value.legends;
        if (choice.physical || choice.legends) choices[key] = choice;
      }
    }
    // "iso-it", "ansi-us": what was chosen for the Apex 3 TKL.
    const old = localStorage.getItem(OLD_KEY);
    if (old) {
      const [physical, legends] = old.split("-");
      if (!choices[APEX_3_TKL] && isPhysical(physical) && isLegends(legends)) choices[APEX_3_TKL] = { physical, legends };
      localStorage.removeItem(OLD_KEY);
      localStorage.setItem(CHOICES_KEY, JSON.stringify(choices));
    }
  } catch {
    /* storage unavailable */
  }
  return choices;
}

export const layouts = $state<{ system: SystemLayout | null; choices: Record<string, LayoutChoice> }>({
  system: null,
  choices: loadChoices(),
});

let asking: Promise<void> | null = null;

/** Reads the system's layout; `again` when a keyboard page opens, since the user may have switched. */
export function loadSystemLayout(again = false): Promise<void> {
  if (asking && !again) return asking;
  asking = (async () => {
    try {
      if (typeof api.systemLayout !== "function") return;
      layouts.system = (await api.systemLayout()) ?? null;
    } catch {
      /* not in the app, or the system said nothing: the language decides */
    }
  })();
  return asking;
}

export function setLayoutChoice(key: string, choice: LayoutChoice | null) {
  const next = { ...layouts.choices };
  if (choice && (choice.physical || choice.legends)) next[key] = choice;
  else delete next[key];
  layouts.choices = next;
  try {
    localStorage.setItem(CHOICES_KEY, JSON.stringify(next));
  } catch {
    /* storage unavailable */
  }
}

/** Legends for the language of the computer, when the system's layout is not known. Brazilian
 * Portuguese before Portuguese. */
const LANGUAGE_LEGENDS: [string, Legends][] = [
  ["it", "it"], ["en-gb", "uk"], ["en-ie", "uk"], ["de", "de"], ["fr", "fr"], ["es", "es"], ["pt-br", "br"],
  ["pt", "pt"], ["sv", "se"], ["fi", "se"], ["nb", "no"], ["nn", "no"], ["no", "no"], ["da", "dk"], ["ja", "jp"],
];

export function languageLegends(language = navigator.language): Legends | null {
  const lang = language.toLowerCase();
  return LANGUAGE_LEGENDS.find(([prefix]) => lang === prefix || lang.startsWith(`${prefix}-`))?.[1] ?? null;
}

/** The shape a layout is usually typed on (as kblayout::physical_for). */
export function physicalFor(legends: Legends): Physical {
  if (legends === "us") return "ansi";
  if (legends === "jp") return "jis";
  if (legends === "br") return "abnt";
  return "iso";
}

/** The legends key names follow outside a keyboard's own page: the system's. */
export function systemLegends(system = layouts.system): Legends {
  if (!asking) void loadSystemLayout();
  return system?.legends ?? languageLegends() ?? "us";
}

/** The layout to draw for a keyboard: what the user set, else the keyboard's shape and the system's
 * legends, else a guess from the system's language. */
export function keyboardLayout(
  key: string | null,
  device: DeviceLayout | null | undefined,
  system: SystemLayout | null = layouts.system,
  choice: LayoutChoice | undefined = key ? layouts.choices[key] : undefined,
): KeyboardLayout {
  const language = languageLegends();
  const [legends, legendsFrom]: [Legends, From] = choice?.legends
    ? [choice.legends, "you"]
    : system?.legends
      ? [system.legends, "system"]
      : language
        ? [language, "language"]
        : ["us", "default"];
  const [physical, physicalFrom]: [Physical, From] = choice?.physical
    ? [choice.physical, "you"]
    : device?.physical
      ? [device.physical, "keyboard"]
      : choice?.legends
        ? [physicalFor(choice.legends), "you"]
        : system?.physical
          ? [system.physical, "system"]
          : [physicalFor(legends), legendsFrom];
  return { physical, legends, physicalFrom, legendsFrom };
}

export const physicalName = (p: Physical) => PHYSICALS.find((x) => x.id === p)!.label;
export const legendsName = (l: Legends) => LEGEND_SETS.find((x) => x.id === l)!.label;

/** "Italian (ISO)". */
export function layoutName(layout: KeyboardLayout): string {
  return `${legendsName(layout.legends)} (${physicalName(layout.physical)})`;
}

/** "detected", "set by you", "guessed from the system's language"… */
export function layoutStatus(layout: KeyboardLayout): string {
  const detected = (from: From) => from === "keyboard" || from === "system";
  if (layout.physicalFrom === "you" || layout.legendsFrom === "you") return "set by you";
  if (detected(layout.physicalFrom) && detected(layout.legendsFrom)) return "detected";
  if (detected(layout.physicalFrom)) return "shape detected, legends guessed";
  return layout.legendsFrom === "language" ? "guessed from the system's language" : "not detected";
}

const FROM_TEXT: Record<From, string> = {
  you: "set by you",
  keyboard: "read from the keyboard",
  system: "from the system's layout",
  language: "from the system's language",
  default: "US by default",
};

/** Where each half comes from, in a sentence for the page. */
export function layoutSources(layout: KeyboardLayout, device: DeviceLayout | null | undefined, system = layouts.system) {
  const shape = layout.physicalFrom === "keyboard" && device
    ? `Shape read from the keyboard (${device.read}${device.region ? `, ${device.region}` : ""})`
    : `Shape ${FROM_TEXT[layout.physicalFrom]}`;
  const legends = layout.legendsFrom === "system" && system
    ? `key legends from the system's layout “${system.name}” (${system.source})`
    : `key legends ${FROM_TEXT[layout.legendsFrom]}`;
  const unknown = device && !device.physical ? `. The keyboard answered ${device.read}, which PeriHub cannot decode yet` : "";
  const notDrawn = system && !system.legends ? `. PeriHub has no legends for the system's layout “${system.name}”` : "";
  return `${shape}; ${legends}${unknown}${notDrawn}.`;
}

/** The test report's line: what is shown, and where it comes from. Layout names only, nothing personal. */
export function layoutReport(key: string | null, device: DeviceLayout | null | undefined, system = layouts.system): string {
  const layout = keyboardLayout(key, device, system);
  const detected = keyboardLayout(null, device, system);
  const read = device ? device.read + (device.region ? ` (${device.region})` : "") : "no read";
  const os = system ? `${system.name} (${system.source})` : "not read";
  const chosen = layout.physicalFrom === "you" || layout.legendsFrom === "you" ? `; detected: ${layoutName(detected)}` : "";
  return `Keyboard layout: ${layoutName(layout)}, ${layoutStatus(layout)}${chosen}; keyboard: ${read}; system: ${os}`;
}
