import { cleanup, fireEvent, render, screen, within } from "@testing-library/svelte";
import { afterEach, describe, expect, it } from "vitest";

import SupportedDevices from "./SupportedDevices.svelte";
import { brands as generated, byLevel, type DeviceBrand } from "../lib/devices";

const brands: DeviceBrand[] = [
  {
    brand: "Acme",
    models: [
      { name: "Alpha", kind: "mouse", level: "supported" },
      { name: "Beta", kind: "mouse", level: "experimental" },
      { name: "Gamma", kind: "keyboard", level: "experimental" },
    ],
  },
  { brand: "Zed", models: [{ name: "Zee", kind: "mouse", level: "read_only" }] },
];

afterEach(cleanup);

describe("SupportedDevices", () => {
  it("shows each brand with how many models are at each level", () => {
    render(SupportedDevices, { brands });
    expect(screen.getByText(/4 models from 2 brands/)).toBeTruthy();
    const acme = screen.getByText("Acme").closest("details")!;
    expect(within(acme).getByText("1 supported")).toBeTruthy();
    expect(within(acme).getByText("2 experimental")).toBeTruthy();
    expect(within(acme).queryByText(/read-only/)).toBeNull();
    const zed = screen.getByText("Zed").closest("details")!;
    expect(within(zed).getByText("1 read-only")).toBeTruthy();
  });

  it("opens a brand to its models, grouped by level", async () => {
    render(SupportedDevices, { brands });
    const acme = screen.getByText("Acme").closest("details")!;
    expect(acme.open).toBe(false);
    await fireEvent.click(within(acme).getByText("Acme"));
    expect(acme.open).toBe(true);
    expect(within(acme).getByText(/Alpha/).textContent).toContain("Supported");
    expect(within(acme).getByText(/Beta, Gamma/).textContent).toContain("Experimental");
  });

  it("lists the generated brands by default", () => {
    render(SupportedDevices);
    expect(generated.length).toBeGreaterThan(5);
    for (const b of generated) expect(screen.getByText(b.brand)).toBeTruthy();
  });
});

describe("generated devices", () => {
  it("has every model once, under a known level", () => {
    for (const b of generated) {
      const names = b.models.map((m) => m.name);
      expect(new Set(names).size).toBe(names.length);
      expect(byLevel(b.models).reduce((n, l) => n + l.models.length, 0)).toBe(b.models.length);
    }
  });

  it("includes the tested devices", () => {
    const find = (brand: string, name: string) => generated.find((b) => b.brand === brand)?.models.find((m) => m.name === name);
    expect(find("SteelSeries", "Apex 3 TKL")?.level).toBe("supported");
    expect(find("Logitech", "G502 HERO")?.level).toBe("supported");
  });
});
