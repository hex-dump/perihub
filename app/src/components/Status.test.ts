import { cleanup, render } from "@testing-library/svelte";
import { afterEach, describe, expect, it } from "vitest";
import Status from "./Status.svelte";
import { STATUS_ICONS, levelStatus, stateStatus, type StatusKind } from "../lib/status";

afterEach(cleanup);

const kinds = Object.keys(STATUS_ICONS) as StatusKind[];

describe("Status", () => {
  // The accent can be close to a status color (green and "connected"): the icon and the word tell them apart.
  it.each(kinds)("%s is an icon and a word", (kind) => {
    const { container } = render(Status, { kind, label: "Some state" });
    const status = container.querySelector(".status")!;
    expect(status.getAttribute("data-kind")).toBe(kind);
    expect(status.querySelectorAll("svg path").length).toBe(STATUS_ICONS[kind].length);
    expect(status.textContent?.trim()).toBe("Some state");
  });

  it("each kind has an icon of its own", () => {
    const drawn = kinds.map((k) => STATUS_ICONS[k].join(" "));
    expect(new Set(drawn).size).toBe(kinds.length);
  });

  it("is a neutral pill as a chip", () => {
    const { container } = render(Status, { kind: "ok", label: "Active", chip: true });
    expect(container.querySelector(".status.chip")).toBeTruthy();
  });

  it("shows support levels and states as their own kinds", () => {
    expect([levelStatus("supported"), levelStatus("experimental"), levelStatus("read_only"), levelStatus("recognized")]).toEqual([
      "ok",
      "experimental",
      "readonly",
      "idle",
    ]);
    expect(["Supported", "Experimental", "Read-only", "Not supported yet"].map(stateStatus)).toEqual([
      "ok",
      "experimental",
      "readonly",
      "idle",
    ]);
  });
});
