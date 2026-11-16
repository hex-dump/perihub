import { describe, expect, it } from "vitest";
import type { WriteEntry } from "./api";
import { REPORT_WRITES, withWriteLog, writeLogLines, writeTime } from "./readOnly.svelte";

const at = new Date(2026, 10, 16, 14, 3, 7).getTime();

function entry(change: Partial<WriteEntry> = {}): WriteEntry {
  return {
    time: at,
    device: "G502 HERO",
    kind: "HID++ 0x8100 function 7",
    data: "04 00 04 00 05 00",
    bytes: 16,
    result: "ok",
    ...change,
  };
}

describe("write log in test reports", () => {
  it("says when nothing was written", () => {
    expect(writeLogLines([], false)).toEqual(["Writes this session: 0 sent, 0 refused"]);
    expect(writeLogLines([], true)).toEqual(["Writes this session: 0 sent, 0 refused (read-only mode on)"]);
  });

  it("lists what was written and refused, never the bytes", () => {
    const lines = writeLogLines([entry(), entry({ result: "refused (read-only mode)", kind: "brightness" })], true);
    expect(lines).toEqual([
      "Writes this session: 1 sent, 1 refused (read-only mode on):",
      "  14:03:07 G502 HERO: HID++ 0x8100 function 7, 16 bytes, ok",
      "  14:03:07 G502 HERO: brightness, 16 bytes, refused (read-only mode)",
    ]);
    expect(lines.join("\n")).not.toContain("04 00 04");
  });

  it("keeps only the last writes, so the form's link stays short", () => {
    const many = Array.from({ length: 40 }, (_, i) => entry({ kind: `write ${i}` }));
    const lines = writeLogLines(many, false);
    expect(lines).toHaveLength(REPORT_WRITES + 1);
    expect(lines[0]).toBe(`Writes this session: 40 sent, 0 refused, last ${REPORT_WRITES}:`);
    expect(lines.at(-1)).toContain("write 39");
  });

  it("goes at the end of the report", () => {
    expect(withWriteLog("PeriHub 0.9.0 on linux x86_64", [], false)).toBe(
      "PeriHub 0.9.0 on linux x86_64\nWrites this session: 0 sent, 0 refused",
    );
  });

  it("shows local time with seconds", () => {
    expect(writeTime(at)).toBe("14:03:07");
  });
});
