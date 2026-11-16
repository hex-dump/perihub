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

declare global {
  interface Window {
    /** Linux: the desktop's theme as the XDG portal says it (system_theme.rs sets it before the page runs and the
     * "system-theme" event keeps it), because WebKitGTK answers matchMedia from the GTK theme's name. */
    __PERIHUB_SYSTEM_THEME__?: "dark" | "light";
  }
}

/** What the app reported for the desktop, if it did. */
export function reportedTheme(): "dark" | "light" | null {
  const v = typeof window !== "undefined" ? window.__PERIHUB_SYSTEM_THEME__ : undefined;
  return v === "dark" || v === "light" ? v : null;
}

/** Keeps what the "system-theme" event says: a theme, or nothing when the desktop has no preference to read. */
export function setReportedTheme(v: unknown) {
  if (v === "dark" || v === "light") window.__PERIHUB_SYSTEM_THEME__ = v;
  else delete window.__PERIHUB_SYSTEM_THEME__;
}

/** What the app reported, else matchMedia; dark unless either says light, as it was before there was a choice. */
export function resolveTheme(choice: ThemeChoice): "dark" | "light" {
  if (choice !== "system") return choice;
  return reportedTheme() ?? (systemQuery()?.matches ? "light" : "dark");
}

/** WebKitGTK: the window has no "follow the system" that works (see __PERIHUB_SYSTEM_THEME__), so it is told. */
export const isLinux = () => typeof navigator !== "undefined" && /Linux/.test(navigator.userAgent) && !/Android/.test(navigator.userAgent);

export function applyToDocument(choice: ThemeChoice, accent: AccentId) {
  const root = document.documentElement;
  root.dataset.theme = resolveTheme(choice);
  root.dataset.accent = accent;
}

/** Tells the window, so what the system draws (title bar, menus) matches. System gives the choice back to it,
 * except on Linux, where null would only turn the GTK dark preference off: the resolved theme is set. */
export async function setNativeTheme(choice: ThemeChoice) {
  try {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    await getCurrentWindow().setTheme(choice === "system" && !isLinux() ? null : resolveTheme(choice));
  } catch {
    /* not in the app, or the system cannot: the interface still follows the choice */
  }
}
