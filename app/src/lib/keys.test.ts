import { describe, expect, it } from "vitest";
import { G502_HERO, G502_HERO_WIRELESS, G502_X_WIRELESS, isG502, ledZoneName, mouseButtonName } from "./keys";

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
