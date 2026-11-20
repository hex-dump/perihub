import { beforeEach, describe, expect, it, vi } from "vitest";
import type { CorsairInfo, DeviceLayout, SystemLayout } from "./api";

const systemLayout = vi.hoisted(() => vi.fn());
vi.mock("./api", async (original) => ({
  ...(await original<typeof import("./api")>()),
  api: { systemLayout },
}));

// A fresh module for each test: the choices are read from storage when it loads.
const load = async () => import("./keyboardLayout.svelte");

const italian: SystemLayout = { legends: "it", name: "it", source: "localectl", physical: "iso" };
const us: SystemLayout = { legends: "us", name: "us", source: "Windows", physical: "ansi" };
const corsairIso: DeviceLayout = { physical: "iso", region: null, read: "Corsair property 0x41 = 2" };

beforeEach(() => {
  vi.resetModules();
  vi.restoreAllMocks();
  localStorage.clear();
  systemLayout.mockReset();
  vi.spyOn(navigator, "language", "get").mockReturnValue("en-US");
});

describe("keyboardLayout", () => {
  it("takes the shape from the keyboard and the legends from the system", async () => {
    const { keyboardLayout, layoutName, layoutStatus } = await load();
    const layout = keyboardLayout("1b1c:1b73", corsairIso, us);
    expect(layout).toEqual({ physical: "iso", legends: "us", physicalFrom: "keyboard", legendsFrom: "system" });
    expect(layoutName(layout)).toBe("US English (ISO)");
    expect(layoutStatus(layout)).toBe("detected");
    expect(layoutName(keyboardLayout("1b1c:1b73", corsairIso, italian))).toBe("Italian (ISO)");
  });

  it("without a read, takes the shape the system's layout implies", async () => {
    const { keyboardLayout, layoutName } = await load();
    const layout = keyboardLayout("1038:1622", null, us);
    expect(layout.physicalFrom).toBe("system");
    expect(layoutName(layout)).toBe("US English (ANSI)");
    const jis: SystemLayout = { legends: "jp", name: "e0010411", source: "Windows", physical: "jis" };
    expect(layoutName(keyboardLayout("1038:1622", null, jis))).toBe("Japanese (JIS)");
    // A code the keyboard sent that PeriHub cannot decode leaves the shape to the system.
    const logitech: DeviceLayout = { physical: null, region: null, read: "Logitech HID++ 0x4540 = 1a" };
    expect(keyboardLayout(null, logitech, italian).physical).toBe("iso");
  });

  it("guesses from the language when the system says nothing, and US as a last resort", async () => {
    const { keyboardLayout, layoutStatus, layoutSources } = await load();
    vi.spyOn(navigator, "language", "get").mockReturnValue("de-AT");
    const layout = keyboardLayout("1038:1622", null, null);
    expect([layout.physical, layout.legends, layout.legendsFrom]).toEqual(["iso", "de", "language"]);
    expect(layoutStatus(layout)).toBe("guessed from the system's language");
    vi.spyOn(navigator, "language", "get").mockReturnValue("ru-RU");
    const russian: SystemLayout = { legends: null, name: "ru", source: "GNOME", physical: null };
    const fallback = keyboardLayout("1038:1622", null, russian);
    expect([fallback.physical, fallback.legends, fallback.legendsFrom]).toEqual(["ansi", "us", "default"]);
    expect(layoutSources(fallback, null, russian)).toContain("no legends for the system's layout “ru”");
  });

  it("follows what the user set for that keyboard, and keeps it", async () => {
    const { keyboardLayout, layoutStatus, setLayoutChoice, layouts } = await load();
    setLayoutChoice("1038:1622", { legends: "fr" });
    // Legends alone: the shape follows them, since the keyboard did not tell its own.
    const layout = keyboardLayout("1038:1622", null, us);
    expect([layout.physical, layout.legends, layoutStatus(layout)]).toEqual(["iso", "fr", "set by you"]);
    // The keyboard's shape still wins over the legends' usual one.
    setLayoutChoice("1b1c:1b73", { legends: "uk" });
    expect(keyboardLayout("1b1c:1b73", { ...corsairIso, physical: "ansi" }, us).physical).toBe("ansi");
    setLayoutChoice("1b1c:1b73", { physical: "jis" });
    expect(keyboardLayout("1b1c:1b73", corsairIso, us).physical).toBe("jis");
    // Another keyboard is not affected.
    expect(keyboardLayout("1532:02b9", null, us).legends).toBe("us");
    expect(JSON.parse(localStorage.getItem("perihub.keyboardLayouts")!)).toEqual({
      "1038:1622": { legends: "fr" },
      "1b1c:1b73": { physical: "jis" },
    });
    setLayoutChoice("1038:1622", null);
    expect(layouts.choices["1038:1622"]).toBeUndefined();
    vi.resetModules();
    const again = await load();
    expect(again.layouts.choices).toEqual({ "1b1c:1b73": { physical: "jis" } });
  });

  it("keeps the layout chosen on the Keys tab before, for the Apex 3 TKL", async () => {
    localStorage.setItem("perihub.layout", "iso-de");
    const { layouts, APEX_3_TKL } = await load();
    expect(layouts.choices[APEX_3_TKL]).toEqual({ physical: "iso", legends: "de" });
    expect(localStorage.getItem("perihub.layout")).toBeNull();
  });

  it("ignores what it cannot read back", async () => {
    localStorage.setItem("perihub.keyboardLayouts", JSON.stringify({ a: { physical: "round", legends: "xx" }, b: 3 }));
    const { layouts } = await load();
    expect(layouts.choices).toEqual({});
  });
});

describe("the system's layout", () => {
  it("is asked of the app, and key names follow it", async () => {
    systemLayout.mockResolvedValue(italian);
    const { loadSystemLayout, layouts, systemLegends } = await load();
    await loadSystemLayout();
    expect(layouts.system).toEqual(italian);
    expect(systemLegends()).toBe("it");
    const { keyLabel } = await import("./keys");
    expect(keyLabel(0x33)).toBe("ò");
  });

  it("falls back to the language when the app cannot say", async () => {
    systemLayout.mockRejectedValue(new Error("no"));
    vi.spyOn(navigator, "language", "get").mockReturnValue("ja-JP");
    const { loadSystemLayout, layouts, systemLegends } = await load();
    await loadSystemLayout();
    expect(layouts.system).toBeNull();
    expect(systemLegends()).toBe("jp");
  });
});

describe("layoutReport", () => {
  it("names the layout and where it comes from, nothing else", async () => {
    const { layoutReport, setLayoutChoice } = await load();
    expect(layoutReport("1b1c:1b73", corsairIso, italian)).toBe(
      "Keyboard layout: Italian (ISO), detected; keyboard: Corsair property 0x41 = 2; system: it (localectl)",
    );
    setLayoutChoice("1b1c:1b73", { physical: "ansi" });
    expect(layoutReport("1b1c:1b73", corsairIso, italian)).toBe(
      "Keyboard layout: Italian (ANSI), set by you; detected: Italian (ISO); keyboard: Corsair property 0x41 = 2; system: it (localectl)",
    );
    const razer: DeviceLayout = { physical: "iso", region: "Italian", read: "Razer 00 86 = 11" };
    expect(layoutReport(null, razer, null)).toBe(
      "Keyboard layout: US English (ISO), shape detected, legends guessed; keyboard: Razer 00 86 = 11 (Italian); system: not read",
    );
  });

  it("goes into the Corsair test report for keyboards only", async () => {
    const { corsairTest } = await import("./testReport");
    const keyboard: CorsairInfo = {
      product_id: 0x1b73, model: "K70 RGB TKL", kind: "keyboard", tested: false, receiver: null, leds: 130, firmware: "1.2.3",
      mode: 1, polling_hz: 1000, dpi: null, read_errors: [], lighting: null, layout: corsairIso,
    };
    expect(corsairTest(keyboard, "0.10.0", "linux x86_64").report).toContain("Keyboard layout: US English (ISO)");
    const mouse: CorsairInfo = { ...keyboard, kind: "mouse", layout: null };
    expect(corsairTest(mouse, "0.10.0", "linux x86_64").report).not.toContain("Keyboard layout");
  });
});
