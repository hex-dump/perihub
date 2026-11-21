import { describe, expect, it } from "vitest";
import {
  G502_HERO, G502_HERO_WIRELESS, G502_X_WIRELESS, LAYOUT_HEIGHT, LAYOUT_WIDTH, bindingLabel, capLabel, isG502, keyActionLabel,
  keyLabel, layoutCaps, ledZoneName, mouseButtonName,
} from "./keys";
import { PHYSICALS, type Physical } from "./keyboardLayout.svelte";

describe("mouseButtonName", () => {
  it("recognizes G502 family members", () => {
    expect(isG502(G502_HERO)).toBe(true);
    expect(isG502(G502_X_WIRELESS)).toBe(true);
    expect(isG502(0xc08f)).toBe(false); // G403 HERO
  });

  it("names G502 HERO and G502 X Wireless buttons", () => {
    expect(mouseButtonName(G502_HERO, 0)).toBe("Left click");
    expect(mouseButtonName(G502_HERO, 5)).toBe("Sniper (DPI Shift)");
    expect(mouseButtonName(G502_X_WIRELESS, 0)).toBe("Left click");
    expect(mouseButtonName(G502_X_WIRELESS, 5)).toBe("Sniper (DPI Shift)");
    expect(mouseButtonName(0xc08f, 0)).toBe("Button 1");
  });
});

describe("ledZoneName", () => {
  it("names the G502 HERO zones, swapped on the wireless model", () => {
    expect(ledZoneName(G502_HERO, 1)).toBe("DPI indicator");
    expect(ledZoneName(G502_HERO, 2)).toBe("G logo");
    expect(ledZoneName(G502_HERO_WIRELESS, 1)).toBe("G logo");
    expect(ledZoneName(G502_HERO_WIRELESS, 2)).toBe("DPI indicator");
  });

  it("falls back to the HID++ location names", () => {
    expect(ledZoneName(0xc08f, 1)).toBe("Main lighting");
    expect(ledZoneName(0xc08f, 2)).toBe("Logo");
    expect(ledZoneName(0xc08f, 5)).toBe("Zone 5");
  });
});

describe("keyboard drawings", () => {
  const usages = (physical: Physical) => layoutCaps(physical).map((c) => c.usage);

  it("draw the keys of each shape", () => {
    expect(usages("ansi")).toContain(0x31);
    expect(usages("ansi")).not.toContain(0x64);
    expect(usages("ansi")).not.toContain(0x32);
    expect(usages("iso")).toEqual(expect.arrayContaining([0x32, 0x64]));
    expect(usages("iso")).not.toContain(0x31);
    expect(layoutCaps("iso").find((c) => c.usage === 0x28)!.h).toBe(2);
    expect(usages("abnt")).toEqual(expect.arrayContaining([0x32, 0x64, 0x87]));
    expect(usages("jis")).toEqual(expect.arrayContaining([0x32, 0x87, 0x88, 0x89, 0x8a, 0x8b]));
    expect(usages("jis")).not.toContain(0x64);
    expect(PHYSICALS.map((p) => layoutCaps(p.id).length)).toEqual([84, 85, 89, 86]);
  });

  it("never overlap and stay on the board", () => {
    for (const { id } of PHYSICALS) {
      const caps = layoutCaps(id);
      expect(new Set(caps.map((c) => c.usage)).size).toBe(caps.length);
      for (const [i, a] of caps.entries()) {
        expect(a.x + a.w).toBeLessThanOrEqual(LAYOUT_WIDTH);
        expect(a.y + a.h).toBeLessThanOrEqual(LAYOUT_HEIGHT);
        for (const b of caps.slice(i + 1)) {
          const apart = a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + a.h <= b.y || b.y + b.h <= a.y;
          expect(apart, `${id}: ${a.usage.toString(16)} and ${b.usage.toString(16)}`).toBe(true);
        }
      }
    }
  });

  it("label the keys with the legends of the layout", () => {
    expect(keyLabel(0x33, "it")).toBe("ò");
    expect(keyLabel(0x1c, "de")).toBe("Z");
    expect(keyLabel(0x89, "jp")).toBe("¥");
    expect(keyLabel(0x87, "br")).toBe("/");
    expect(capLabel(0x64, "it", "iso")).toBe("<");
    // An ANSI board's backslash key types what the ISO key next to Enter types.
    expect(capLabel(0x31, "it", "ansi")).toBe("ù");
    expect(capLabel(0x31, "us", "ansi")).toBe("\\");
    expect(bindingLabel({ type: "key", usage: 0x2f }, "de")).toBe("Ü");
  });
});

describe("keyActionLabel", () => {
  it("names a key table entry: a key, nothing, a modifier by its bit, a media usage, Fn and the rest by their bytes", () => {
    expect(keyActionLabel({ type: "key", usage: 0x29 }, "us")).toBe("Esc");
    expect(keyActionLabel({ type: "key", usage: 0 }, "us")).toBe("Disabled");
    expect(keyActionLabel({ type: "modifier", bit: 0x01 }, "us")).toBe("Ctrl");
    expect(keyActionLabel({ type: "modifier", bit: 0x20 }, "us")).toBe("R Shift");
    expect(keyActionLabel({ type: "modifier", bit: 0x40 }, "it")).toBe("Alt Gr");
    expect(keyActionLabel({ type: "modifier", bit: 0x03 }, "us")).toBe("Modifiers 0x03");
    expect(keyActionLabel({ type: "media", usage: 0xcd }, "us")).toBe("Play / Pause");
    expect(keyActionLabel({ type: "media", usage: 0x183 }, "us")).toBe("Media player");
    expect(keyActionLabel({ type: "media", usage: 0x999 }, "us")).toBe("Media 0x999");
    expect(keyActionLabel({ type: "fn" }, "us")).toBe("Fn");
    expect(keyActionLabel({ type: "other", bytes: [0xd0, 0xa2, 0x01] }, "us")).toBe("d0 a2 01");
  });
});
