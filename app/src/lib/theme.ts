// Appearance: the theme and the accent color. Both are kept by prefs.svelte.ts and shown on <html> as
// data-theme ("dark" or "light", never "system") and data-accent, which styles.css turns into colors.
// public/theme-init.js does the same before the first paint and must read the same keys.

export type ThemeChoice = "system" | "dark" | "light";
export type AccentId = "orange" | "blue" | "green" | "purple" | "pink" | "teal";

export const THEMES: { id: ThemeChoice; label: string }[] = [
  { id: "system", label: "System" },
  { id: "dark", label: "Dark" },
  { id: "light", label: "Light" },
];

/** `swatch` is the accent on the dark theme; styles.css has the colors the interface really uses. */
export const ACCENTS: { id: AccentId; label: string; swatch: string }[] = [
  { id: "orange", label: "Orange", swatch: "#ff6a2b" },
  { id: "blue", label: "Blue", swatch: "#4f9dff" },
  { id: "green", label: "Green", swatch: "#46c76b" },
  { id: "purple", label: "Purple", swatch: "#ac8aff" },
  { id: "pink", label: "Pink", swatch: "#ff5fa2" },
  { id: "teal", label: "Teal", swatch: "#2cc6c0" },
];

export const THEME_KEY = "perihub.theme";
export const ACCENT_KEY = "perihub.accent";

export const isTheme = (v: unknown): v is ThemeChoice => THEMES.some((t) => t.id === v);
export const isAccent = (v: unknown): v is AccentId => ACCENTS.some((a) => a.id === v);

const LIGHT_QUERY = "(prefers-color-scheme: light)";

/** The system's query, or null where the webview has no matchMedia. */
export function systemQuery(): MediaQueryList | null {
  return typeof window !== "undefined" && typeof window.matchMedia === "function" ? window.matchMedia(LIGHT_QUERY) : null;
}

/** "System" is dark unless the system says light, as it was before there was a choice. */
export function resolveTheme(choice: ThemeChoice): "dark" | "light" {
  if (choice === "system") return systemQuery()?.matches ? "light" : "dark";
  return choice;
}

export function applyToDocument(choice: ThemeChoice, accent: AccentId) {
  const root = document.documentElement;
  root.dataset.theme = resolveTheme(choice);
  root.dataset.accent = accent;
}

/** Tells the window, so what the system draws (title bar, menus) matches. System gives the choice back to it. */
export async function setNativeTheme(choice: ThemeChoice) {
  try {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow().setTheme(choice === "system" ? null : choice);
  } catch {
    /* not in the app, or the system cannot: the interface still follows the choice */
  }
}
