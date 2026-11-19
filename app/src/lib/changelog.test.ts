import { describe, expect, it } from "vitest";
import { itemTitle, parseRelease } from "./changelog";

const CHANGELOG = `# Changelog

## 0.9.5 — 2026-11-19

A summary
in two lines.

### Added
- **A one-time prompt**: when you connect a device, a notice offers to start **Test my device**.
- **Second entry** ([#12](https://example.org/12)): its details
  go on in an indented line.

### Fixed
- **A bug with \`code\`**: details.

## 0.9.4 — 2026-11-18

Older summary.

### Changed
- **Old thing**: details.

## 0.9.3 — 2026-11-17

### Removed
- No bold lead here.
`;

describe("parseRelease", () => {
  it("picks the section of the version, with titles only", () => {
    expect(parseRelease(CHANGELOG, "0.9.5")).toEqual({
      version: "0.9.5",
      date: "2026-11-19",
      summary: "A summary in two lines.",
      sections: [
        { name: "Added", titles: ["A one-time prompt", "Second entry"] },
        { name: "Fixed", titles: ["A bug with code"] },
      ],
    });
  });

  it("does not run into the next version", () => {
    const notes = parseRelease(CHANGELOG, "0.9.4");
    expect(notes?.sections).toEqual([{ name: "Changed", titles: ["Old thing"] }]);
    expect(notes?.summary).toBe("Older summary.");
  });

  it("gives null for a version without a section", () => {
    expect(parseRelease(CHANGELOG, "0.9.6")).toBeNull();
    expect(parseRelease("", "0.9.5")).toBeNull();
  });

  it("copes with no summary, and with an item without a bold lead", () => {
    const notes = parseRelease(CHANGELOG, "0.9.3");
    expect(notes?.summary).toBe("");
    expect(notes?.sections).toEqual([{ name: "Removed", titles: ["No bold lead here."] }]);
  });

  it("reads Windows line endings", () => {
    expect(parseRelease(CHANGELOG.replaceAll("\n", "\r\n"), "0.9.5")?.sections).toHaveLength(2);
  });

  it("shortens a long title that has no bold lead", () => {
    expect(itemTitle("word ".repeat(40)).length).toBeLessThanOrEqual(90);
  });
});
