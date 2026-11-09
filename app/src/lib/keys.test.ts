import { describe, expect, it } from "vitest";
import { G502_HERO, G502_X_WIRELESS, isG502, mouseButtonName } from "./keys";

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
