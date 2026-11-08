import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const api = vi.hoisted(() => ({
  corsairSetLighting: vi.fn(),
  corsairLightingOff: vi.fn(),
  corsairKeepLighting: vi.fn(),
  appSettings: vi.fn(),
}));

vi.mock("../lib/api", async (original) => ({ ...(await original<typeof import("../lib/api")>()), api }));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: async () => "0.6.0" }));

import type { CorsairInfo } from "../lib/api";
import CorsairDevice from "./CorsairDevice.svelte";

const info: CorsairInfo = {
  product_id: 0x1b70,
  model: "M55 RGB PRO",
  kind: "mouse",
  leds: 2,
  firmware: "4.5.12",
  mode: 1,
  polling_hz: 1000,
  dpi: 1600,
  read_errors: [],
  lighting: null,
};

const wait = (ms: number) => vi.advanceTimersByTimeAsync(ms);

beforeEach(() => {
  vi.useFakeTimers();
  vi.clearAllMocks();
  api.corsairSetLighting.mockImplementation(async (color: string) => ({ ...info, lighting: color }));
  api.corsairLightingOff.mockResolvedValue(undefined);
  api.corsairKeepLighting.mockResolvedValue(undefined);
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("a Corsair mouse", () => {
  it("shows what was read and changes nothing by itself", async () => {
    render(CorsairDevice, { props: { info, error: null } });
    await wait(0);
    expect(screen.getByText("4.5.12")).toBeTruthy();
    expect(screen.getByText("1000 Hz")).toBeTruthy();
    expect(api.corsairSetLighting).not.toHaveBeenCalled();
  });

  it("turns lighting on only after the risk is acknowledged", async () => {
    render(CorsairDevice, { props: { info, error: null } });
    await fireEvent.click(screen.getByRole("button", { name: "Try experimental lighting…" }));
    const turnOn = screen.getByRole("button", { name: "Turn on for this session" }) as HTMLButtonElement;
    expect(turnOn.disabled).toBe(true);
    expect(screen.getByRole("note").textContent).toContain("accepts no responsibility for any damage");
    await fireEvent.click(screen.getByRole("checkbox"));
    await fireEvent.click(turnOn);
    await wait(0);
    expect(api.corsairSetLighting).toHaveBeenCalledWith(0x1b70, "#ff6a00");
  });

  it("gives the mouse back after 15 seconds unless it still works", async () => {
    const page = render(CorsairDevice, { props: { info, error: null } });
    await fireEvent.click(screen.getByRole("button", { name: "Try experimental lighting…" }));
    await fireEvent.click(screen.getByRole("checkbox"));
    await fireEvent.click(screen.getByRole("button", { name: "Turn on for this session" }));
    await wait(0);
    // The status reports the lighting on.
    await page.rerender({ info: { ...info, lighting: "#ff6a00" }, error: null });
    expect(screen.getByRole("alertdialog").textContent).toContain("gives the mouse back to its own settings");
    await wait(15_000);
    expect(api.corsairLightingOff).toHaveBeenCalledOnce();
  });

  it("keeps the lighting when the mouse still works", async () => {
    const page = render(CorsairDevice, { props: { info, error: null } });
    await fireEvent.click(screen.getByRole("button", { name: "Try experimental lighting…" }));
    await fireEvent.click(screen.getByRole("checkbox"));
    await fireEvent.click(screen.getByRole("button", { name: "Turn on for this session" }));
    await wait(0);
    await page.rerender({ info: { ...info, lighting: "#ff6a00" }, error: null });
    await fireEvent.click(screen.getByRole("button", { name: "Keep changes" }));
    await wait(20_000);
    expect(api.corsairKeepLighting).toHaveBeenCalledOnce();
    expect(api.corsairLightingOff).not.toHaveBeenCalled();
  });

  it("offers no lighting on a mouse without LEDs", () => {
    render(CorsairDevice, { props: { info: { ...info, model: "SABRE V2 PRO", leds: 0 }, error: null } });
    expect(screen.queryByRole("button", { name: "Try experimental lighting…" })).toBeNull();
    expect(screen.getByText("The SABRE V2 PRO has no lighting.")).toBeTruthy();
  });

  it("talks about keys on a keyboard", async () => {
    render(CorsairDevice, { props: { info: { ...info, model: "K70 CORE RGB", kind: "keyboard", dpi: null }, error: null } });
    await fireEvent.click(screen.getByRole("button", { name: "Try experimental lighting…" }));
    expect(document.body.textContent).toContain("it is not known whether every key still types");
    expect(screen.queryByText("DPI")).toBeNull();
  });

  it("explains when the mouse could not be opened", () => {
    render(CorsairDevice, { props: { info: null, error: "Corsair M75: permission denied" } });
    expect(screen.getByText("Corsair M75: permission denied")).toBeTruthy();
  });
});
