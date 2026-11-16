// The screenshots' stand-in for @tauri-apps/api/window.

export function getCurrentWindow() {
  return { async setTheme(_theme: "light" | "dark" | null) {} };
}
