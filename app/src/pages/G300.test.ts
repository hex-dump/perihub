import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const api = vi.hoisted(() => ({
  g300Info: vi.fn(),
  appSettings: vi.fn(),
  experimentalMice: vi.fn(),
  setExperimental: vi.fn(),
  onExperimental: vi.fn(),
  keyCatalog: vi.fn(),
  g300Save: vi.fn(),
  g300KeepChanges: vi.fn(),
  g300UndoLastWrite: vi.fn(),
}));

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
  experimental: false,
  pending_undo: false,
  profiles: [
    {
      number: 1,
      color: "#ffffff",
      report_rate_hz: 1000,
      dpi_levels: [500, 1000, 1500, 2500],
      default_level: 1,
      shift_dpi: null,
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
beforeEach(() => {
  vi.clearAllMocks();
  api.appSettings.mockResolvedValue({ system: "linux x86_64" });
  api.experimentalMice.mockResolvedValue([]);
  api.onExperimental.mockResolvedValue(() => {});
  api.keyCatalog.mockResolvedValue({ targets: [{ usage: 0x04 }, { usage: 0x06 }, { usage: 0x2b }], sources: [] });
});

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

  it("offers experimental editing only behind a warning", async () => {
    render(G300, { props: { info, error: null } });
    expect(screen.queryByRole("button", { name: "Edit" })).toBeNull();
    expect(screen.queryAllByRole("combobox").length).toBe(0);
    await fireEvent.click(screen.getByRole("button", { name: "Edit anyway…" }));
    const confirm = screen.getByRole("button", { name: "Turn on experimental editing" }) as HTMLButtonElement;
    expect(confirm.disabled).toBe(true);
  });

  it("saves only what was changed, keeps left and right as they are, and asks to keep the save", async () => {
    const experimental = { ...info, experimental: true };
    api.g300Save.mockResolvedValue(experimental);
    render(G300, { props: { info: experimental, error: null } });
    await fireEvent.click(screen.getByRole("button", { name: "Edit" }));
    expect(screen.queryByRole("combobox", { name: "Left button function" })).toBeNull();
    await fireEvent.change(screen.getByRole("combobox", { name: "G4 function" }), { target: { value: "mouse" } });
    await fireEvent.change(screen.getByRole("combobox", { name: "G4 click" }), { target: { value: "4" } });
    await fireEvent.change(screen.getByRole("combobox", { name: "DPI level 1" }), { target: { value: "750" } });
    await fireEvent.click(screen.getByRole("button", { name: "Save to the mouse" }));
    expect(api.g300Save).toHaveBeenCalledTimes(1);
    const [number, settings] = api.g300Save.mock.calls[0];
    const profile = info.profiles[0];
    expect(number).toBe(1);
    expect(settings).toEqual({
      color: "#ffffff",
      report_rate_hz: 1000,
      dpi_levels: [750, 1000, 1500, 2500],
      default_level: 1,
      shift_dpi: null,
      buttons: [...profile.buttons.slice(0, 3), { type: "mouse", button: 4 }, ...profile.buttons.slice(4)],
    });
    expect(await screen.findByRole("button", { name: /keep/i })).toBeTruthy();
  });

  it("keeps asking to keep a save when a new status arrives for another device", async () => {
    const experimental = { ...info, experimental: true };
    api.g300Save.mockResolvedValue(experimental);
    const { rerender } = render(G300, { props: { info: experimental, error: null } });
    await fireEvent.click(screen.getByRole("button", { name: "Edit" }));
    await fireEvent.change(screen.getByRole("combobox", { name: "DPI level 1" }), { target: { value: "750" } });
    await fireEvent.click(screen.getByRole("button", { name: "Save to the mouse" }));
    await screen.findByRole("button", { name: /keep/i });
    await rerender({ info: { ...experimental }, error: null });
    expect(screen.queryByRole("button", { name: /keep/i })).toBeTruthy();
    // Experimental editing turned off: the mouse is opened again read-only.
    await rerender({ info: { ...info }, error: null });
    expect(screen.queryByRole("button", { name: /keep/i })).toBeNull();
  });
});
