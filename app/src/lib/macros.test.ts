import { describe, expect, it } from "vitest";
import type { Macro } from "./api";
import { macroSummary } from "./macros";

describe("macroSummary", () => {
  // Issue #4: a G HUB macro for Ctrl+Shift+Tab read as "Ctrl+Ctrl Shift+Shift Tab".
  it("does not repeat a modifier key's own modifier", () => {
    const macro = {
      kind: "once",
      press: [
        { type: "key_down", modifiers: 0x01, key: 0xe0 },
        { type: "key_down", modifiers: 0x02, key: 0xe1 },
        { type: "key_down", modifiers: 0x00, key: 0x2b },
      ],
      release: [],
    } as unknown as Macro;
    expect(macroSummary(macro)).toBe("No repeat: Ctrl Shift Tab · 0 ms");
  });

  it("keeps the other modifiers held with a key", () => {
    const macro = {
      kind: "once",
      press: [{ type: "key_down", modifiers: 0x03, key: 0xe1 }, { type: "key_down", modifiers: 0x01, key: 0x06 }],
      release: [],
    } as unknown as Macro;
    expect(macroSummary(macro)).toBe("No repeat: Ctrl+Shift Ctrl+C · 0 ms");
  });
});
