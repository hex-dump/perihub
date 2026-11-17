import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const setTheme = vi.hoisted(() => vi.fn(async () => {}));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({ setTheme }) }));
/** The handlers the app registered with listen(), by event name; `emit` is the Rust side sending one. */
const handlers = vi.hoisted(() => new Map<string, (event: { payload: unknown }) => void>());
vi.mock("@tauri-apps/api/event", () => ({
  listen: async (name: string, fn: (event: { payload: unknown }) => void) => {
    handlers.set(name, fn);
    return () => handlers.delete(name);
  },
}));
const emit = (payload: string | null) => handlers.get("system-theme")!({ payload });
const onLinux = (linux: boolean) =>
  vi.spyOn(navigator, "userAgent", "get").mockReturnValue(linux ? "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15" : "Mozilla/5.0 (Windows NT 10.0)");

/** A matchMedia whose answer the test changes, as the system does when its theme changes. */
function fakeSystem(light: boolean) {
  const listeners = new Set<() => void>();
  const query = {
    get matches() {
      return light;
    },
    media: "(prefers-color-scheme: light)",
    addEventListener: (_: string, fn: () => void) => listeners.add(fn),
    removeEventListener: (_: string, fn: () => void) => listeners.delete(fn),
  };
  window.matchMedia = (() => query) as unknown as typeof window.matchMedia;
  return {
    listeners,
    change(next: boolean) {
      light = next;
      listeners.forEach((fn) => fn());
    },
  };
}

// A fresh module for each test: the preferences are read from storage when it loads.
const load = async () => import("./prefs.svelte");
const html = () => document.documentElement.dataset;

beforeEach(() => {
  vi.resetModules();
  localStorage.clear();
  delete document.documentElement.dataset.theme;
  delete document.documentElement.dataset.accent;
  setTheme.mockClear();
  handlers.clear();
  delete window.__PERIHUB_SYSTEM_THEME__;
});

afterEach(() => {
  vi.restoreAllMocks();
  // @ts-expect-error jsdom has none
  delete window.matchMedia;
});

describe("theme", () => {
  it("is System, orange and dark when nothing was chosen and the system says nothing", async () => {
    const { prefs, initTheme } = await load();
    initTheme();
    expect([prefs.theme, prefs.accent]).toEqual(["system", "orange"]);
    expect(html()).toMatchObject({ theme: "dark", accent: "orange" });
    expect(setTheme).not.toHaveBeenCalled();
  });

  it("setting it changes data-theme, keeps the choice and tells the window", async () => {
    fakeSystem(false);
    const { setTheme: choose, initTheme } = await load();
    initTheme();
    choose("light");
    expect(html().theme).toBe("light");
    expect(localStorage.getItem("perihub.theme")).toBe("light");
    await vi.waitFor(() => expect(setTheme).toHaveBeenLastCalledWith("light"));
    choose("dark");
    expect(html().theme).toBe("dark");
    await vi.waitFor(() => expect(setTheme).toHaveBeenLastCalledWith("dark"));
    choose("system");
    expect(localStorage.getItem("perihub.theme")).toBe("system");
    await vi.waitFor(() => expect(setTheme).toHaveBeenLastCalledWith(null));
  });

  it("System follows the system as it changes, and only System does", async () => {
    const system = fakeSystem(false);
    const { setTheme: choose, initTheme } = await load();
    const stop = initTheme();
    expect(html().theme).toBe("dark");
    system.change(true);
    expect(html().theme).toBe("light");
    system.change(false);
    expect(html().theme).toBe("dark");
    choose("dark");
    system.change(true);
    expect(html().theme).toBe("dark");
    choose("system");
    expect(html().theme).toBe("light");
    stop();
    expect(system.listeners.size).toBe(0);
  });

  it("what the app reports for the desktop wins over matchMedia, which WebKitGTK can get wrong", async () => {
    fakeSystem(true);
    window.__PERIHUB_SYSTEM_THEME__ = "dark";
    const { initTheme } = await load();
    initTheme();
    expect(html().theme).toBe("dark");
    window.__PERIHUB_SYSTEM_THEME__ = "light";
    fakeSystem(false);
    initTheme();
    expect(html().theme).toBe("light");
  });

  it("the system-theme event changes System only, and is remembered for when System is chosen", async () => {
    fakeSystem(false);
    window.__PERIHUB_SYSTEM_THEME__ = "dark";
    const { setTheme: choose, initTheme } = await load();
    const stop = initTheme();
    await vi.waitFor(() => expect(handlers.has("system-theme")).toBe(true));
    emit("light");
    expect(html().theme).toBe("light");
    emit("dark");
    expect(html().theme).toBe("dark");
    choose("dark");
    emit("light");
    expect(html().theme).toBe("dark");
    choose("system");
    expect(html().theme).toBe("light");
    emit(null);
    expect(html().theme).toBe("dark");
    stop();
    expect(handlers.has("system-theme")).toBe(false);
  });

  it("on Linux System sets the resolved theme on the window, never null, and follows the event", async () => {
    onLinux(true);
    fakeSystem(false);
    window.__PERIHUB_SYSTEM_THEME__ = "light";
    const { setTheme: choose, initTheme } = await load();
    initTheme();
    await vi.waitFor(() => expect(setTheme).toHaveBeenLastCalledWith("light"));
    await vi.waitFor(() => expect(handlers.has("system-theme")).toBe(true));
    emit("dark");
    await vi.waitFor(() => expect(setTheme).toHaveBeenLastCalledWith("dark"));
    choose("light");
    choose("system");
    await vi.waitFor(() => expect(setTheme).toHaveBeenLastCalledWith("dark"));
    expect(setTheme).not.toHaveBeenCalledWith(null);
  });

  it("elsewhere System gives the window back to the system with null", async () => {
    onLinux(false);
    fakeSystem(false);
    const { setTheme: choose, initTheme } = await load();
    initTheme();
    expect(setTheme).not.toHaveBeenCalled();
    choose("system");
    await vi.waitFor(() => expect(setTheme).toHaveBeenLastCalledWith(null));
  });

  it("starts from what was saved", async () => {
    fakeSystem(true);
    localStorage.setItem("perihub.theme", "dark");
    localStorage.setItem("perihub.accent", "teal");
    const { prefs, initTheme } = await load();
    initTheme();
    expect([prefs.theme, prefs.accent]).toEqual(["dark", "teal"]);
    expect(html()).toMatchObject({ theme: "dark", accent: "teal" });
    await vi.waitFor(() => expect(setTheme).toHaveBeenCalledWith("dark"));
  });

  it("ignores a saved value it does not know", async () => {
    localStorage.setItem("perihub.theme", "sepia");
    localStorage.setItem("perihub.accent", "ultraviolet");
    const { prefs } = await load();
    expect([prefs.theme, prefs.accent]).toEqual(["system", "orange"]);
  });

  it("keeps the accent for the next start", async () => {
    const { setAccent, initTheme } = await load();
    initTheme();
    setAccent("purple");
    expect(html().accent).toBe("purple");
    expect(localStorage.getItem("perihub.accent")).toBe("purple");
    vi.resetModules();
    const again = await load();
    expect(again.prefs.accent).toBe("purple");
  });

  it("the script that runs before the first paint gives the same answer", async () => {
    const script = (await import("../../public/theme-init.js?raw")).default as string;
    const run = () => {
      delete document.documentElement.dataset.theme;
      delete document.documentElement.dataset.accent;
      new Function(script)();
    };
    fakeSystem(true);
    run();
    expect(html()).toMatchObject({ theme: "light" });
    localStorage.setItem("perihub.theme", "dark");
    localStorage.setItem("perihub.accent", "pink");
    run();
    expect(html()).toMatchObject({ theme: "dark", accent: "pink" });
    localStorage.setItem("perihub.theme", "light");
    fakeSystem(false);
    run();
    expect(html().theme).toBe("light");
    localStorage.removeItem("perihub.theme");
    window.__PERIHUB_SYSTEM_THEME__ = "dark";
    fakeSystem(true);
    run();
    expect(html().theme).toBe("dark");
    window.__PERIHUB_SYSTEM_THEME__ = "light";
    fakeSystem(false);
    run();
    expect(html().theme).toBe("light");
  });
});
