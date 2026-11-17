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
  theme: saved<ThemeChoice>(THEME_KEY, isTheme, "system"),
  accent: saved<AccentId>(ACCENT_KEY, isAccent, "orange"),
});

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
