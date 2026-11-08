import { cleanup, render, screen } from "@testing-library/svelte";
import { afterEach, describe, expect, it, vi } from "vitest";

const api = vi.hoisted(() => ({ g300Info: vi.fn(), appSettings: vi.fn() }));

vi.mock("../lib/api", async (original) => ({ ...(await original<typeof import("../lib/api")>()), api }));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: async () => "0.6.0" }));
api.appSettings.mockResolvedValue({ system: "linux x86_64" });

import type { G300Info } from "../lib/api";
import G300 from "./G300.svelte";

/** Profile 1 of the G300 in issue #16, as the core reads it. */
const info: G300Info = {
  product_id: 0xc246,
  model: "G300",
  active_profile: 1,
  active_level: 1,
  active_raw: "f0 02 00 00",
  profiles: [
    {
      number: 1,
      color: "#ffffff",
      report_rate_hz: 1000,
      dpi_levels: [500, 1000, 1500, 2500],
      default_level: 1,
      shift_byte: 0x40,
      buttons: [
        { type: "mouse", button: 1 },
        { type: "mouse", button: 2 },
        { type: "mouse", button: 3 },
        { type: "key", modifiers: 0x03, key: 0x2b },
        { type: "key", modifiers: 0x01, key: 0x2b },
        { type: "key", modifiers: 0x01, key: 0x19 },
        { type: "key", modifiers: 0x01, key: 0x06 },
        { type: "profile_cycle" },
        { type: "dpi_default" },
      ],
      raw: "f3 07 00 02 84 06 0a 40",
    },
  ],
};

afterEach(cleanup);

describe("the G300 page", () => {
  it("shows the stored profile with the names printed on the mouse, and nothing to edit", () => {
    render(G300, { props: { info, error: null } });
    expect(screen.getByText("White")).toBeTruthy();
    expect(screen.getByText("1000 Hz")).toBeTruthy();
    const row = (name: string) => screen.getByText(name).closest("tr")!.textContent;
    expect(row("G4")).toContain("Ctrl + Shift + Tab");
    expect(row("G7")).toContain("Ctrl + C");
    expect(row("G8")).toContain("Next profile");
    expect(screen.queryAllByRole("textbox").length + screen.queryAllByRole("spinbutton").length).toBe(0);
  });
});
