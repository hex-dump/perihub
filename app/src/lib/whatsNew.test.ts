import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ReleaseNotes } from "./changelog";
import { LAST_SEEN_KEY, notesToShow } from "./whatsNew";

const notes: ReleaseNotes = { version: "0.9.5", date: "2026-11-19", summary: "S", sections: [] };

beforeEach(() => localStorage.clear());
afterEach(() => vi.restoreAllMocks());

describe("notesToShow", () => {
  it("shows nothing on a fresh install, only remembers the version", () => {
    expect(notesToShow("0.9.5", notes)).toBeNull();
    expect(localStorage.getItem(LAST_SEEN_KEY)).toBe("0.9.5");
  });

  it("shows nothing for a version already seen", () => {
    localStorage.setItem(LAST_SEEN_KEY, "0.9.5");
    expect(notesToShow("0.9.5", notes)).toBeNull();
  });

  it("shows the notes once after a version change", () => {
    localStorage.setItem(LAST_SEEN_KEY, "0.9.4");
    expect(notesToShow("0.9.5", notes)).toBe(notes);
    expect(localStorage.getItem(LAST_SEEN_KEY)).toBe("0.9.5");
    expect(notesToShow("0.9.5", notes)).toBeNull();
  });

  it("shows nothing when the version has no notes, and still remembers it", () => {
    localStorage.setItem(LAST_SEEN_KEY, "0.9.4");
    expect(notesToShow("0.9.6", notes)).toBeNull();
    expect(localStorage.getItem(LAST_SEEN_KEY)).toBe("0.9.6");
    expect(notesToShow("0.9.6", null)).toBeNull();
  });

  it("shows nothing and does not fail when storage is unavailable", () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("denied");
    });
    expect(notesToShow("0.9.5", notes)).toBeNull();
    vi.restoreAllMocks();
    localStorage.setItem(LAST_SEEN_KEY, "0.9.4");
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("full");
    });
    expect(notesToShow("0.9.5", notes)).toBeNull();
  });
});
