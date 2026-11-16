import {
  ACCENT_KEY,
  THEME_KEY,
  applyToDocument,
  isAccent,
  isTheme,
  isLinux,
  setNativeTheme,
  setReportedTheme,
  systemQuery,
  type AccentId,
  type ThemeChoice,
} from "./theme";

// Interface preferences stored in the embedded browser (convenience only: if missing, defaults are used).

export type LayoutId =
  | "ansi-us" | "iso-uk" | "iso-it" | "iso-de" | "iso-fr" | "iso-es" | "iso-pt" | "iso-se" | "iso-no" | "iso-dk";

export const LAYOUTS: { id: LayoutId; label: string }[] = [
  { id: "ansi-us", label: "ANSI (US)" },
  { id: "iso-uk", label: "ISO (UK)" },
  { id: "iso-it", label: "ISO (Italian)" },
  { id: "iso-de", label: "ISO (German)" },
  { id: "iso-fr", label: "ISO (French)" },
  { id: "iso-es", label: "ISO (Spanish)" },
  { id: "iso-pt", label: "ISO (Portuguese)" },
  { id: "iso-se", label: "ISO (Swedish/Finnish)" },
  { id: "iso-no", label: "ISO (Norwegian)" },
  { id: "iso-dk", label: "ISO (Danish)" },
];

/** Default layout for the system language. Brazilian Portuguese (ABNT2) has extra keys, so it stays on ANSI. */
const LANGUAGE_LAYOUTS: [string, LayoutId][] = [
  ["it", "iso-it"], ["en-gb", "iso-uk"], ["en-ie", "iso-uk"], ["de", "iso-de"], ["fr", "iso-fr"],
  ["es", "iso-es"], ["pt-pt", "iso-pt"], ["sv", "iso-se"], ["fi", "iso-se"], ["nb", "iso-no"],
  ["nn", "iso-no"], ["no", "iso-no"], ["da", "iso-dk"],
];

const LAYOUT_KEY = "perihub.layout";

function initialLayout(): LayoutId {
  try {
    const saved = localStorage.getItem(LAYOUT_KEY);
    if (LAYOUTS.some((l) => l.id === saved)) return saved as LayoutId;
  } catch {
    /* storage unavailable */
  }
  const lang = navigator.language.toLowerCase();
  const match = LANGUAGE_LAYOUTS.find(([prefix]) => lang === prefix || lang.startsWith(`${prefix}-`));
  return match?.[1] ?? "ansi-us";
}

function saved<T extends string>(key: string, valid: (v: unknown) => v is T, fallback: T): T {
  try {
    const value = localStorage.getItem(key);
    if (valid(value)) return value;
  } catch {
    /* storage unavailable */
  }
  return fallback;
}

export const prefs = $state({
  layout: initialLayout(),
  theme: saved<ThemeChoice>(THEME_KEY, isTheme, "system"),
  accent: saved<AccentId>(ACCENT_KEY, isAccent, "orange"),
});

export function setLayout(layout: LayoutId) {
  prefs.layout = layout;
  try {
    localStorage.setItem(LAYOUT_KEY, layout);
  } catch {
    /* storage unavailable */
  }
}

function store(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    /* storage unavailable */
  }
}

export function setTheme(theme: ThemeChoice) {
  prefs.theme = theme;
  store(THEME_KEY, theme);
  applyToDocument(prefs.theme, prefs.accent);
  void setNativeTheme(theme);
}

export function setAccent(accent: AccentId) {
  prefs.accent = accent;
  store(ACCENT_KEY, accent);
  applyToDocument(prefs.theme, prefs.accent);
}

/** Shows the saved theme and accent, and follows the system while the theme is "System". Call once at startup. */
export function initTheme(): () => void {
  applyToDocument(prefs.theme, prefs.accent);
  // The system is already what the window follows, unless a choice made it differ or it cannot (Linux).
  if (prefs.theme !== "system" || isLinux()) void setNativeTheme(prefs.theme);
  const onChange = () => {
    if (prefs.theme !== "system") return;
    applyToDocument(prefs.theme, prefs.accent);
    if (isLinux()) void setNativeTheme(prefs.theme);
  };
  const query = systemQuery();
  query?.addEventListener("change", onChange);
  // Linux: the desktop's theme as the app reads it from the portal (system_theme.rs). It is kept even when the
  // choice is not System, so that choosing System later starts from it.
  let stopped = false;
  let unlisten: (() => void) | undefined;
  void import("@tauri-apps/api/event")
    .then(({ listen }) =>
      listen<string | null>("system-theme", (event) => {
        setReportedTheme(event.payload);
        onChange();
      }),
    )
    .then((off) => (stopped ? off() : (unlisten = off)))
    .catch(() => {
      /* not in the app */
    });
  return () => {
    stopped = true;
    unlisten?.();
    query?.removeEventListener("change", onChange);
  };
}
