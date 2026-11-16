import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const setTheme = vi.hoisted(() => vi.fn(async () => {}));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({ setTheme }) }));

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
});

afterEach(() => {
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
  });
});
