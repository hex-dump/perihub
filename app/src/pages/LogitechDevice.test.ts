import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const api = vi.hoisted(() => ({
  appSettings: vi.fn(async () => ({ system: "linux x86_64" })),
  openTestForm: vi.fn(),
  logitechLive: vi.fn(),
  logitechSetDpi: vi.fn(),
  logitechResetDpi: vi.fn(),
}));
vi.mock("../lib/api", async (original) => ({ ...(await original<typeof import("../lib/api")>()), api }));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: async () => "0.6.7" }));

import type { LogitechLive, ReceiverReport, UnsupportedReceiverDevice } from "../lib/api";
import { acceptNoWarranty } from "../lib/noWarranty";
import LogitechDevice from "./LogitechDevice.svelte";

const device: UnsupportedReceiverDevice = {
  slot: 1,
  name: "MX Master 3S",
  kind: 3,
  protocol: "4.5",
  wireless_id: null,
  device_info: null,
  firmware: ["RBM 12.01.0008"],
  features: [0x0001, 0x1004, 0x1b04, 0x2201, 0x2110, 0x2121, 0x2150, 0x1815].map((id) => ({ id, kind: 0, version: 0 })),
  dpi: { min: 200, max: 8000, step: 50, list: [] },
  report_rates_ms: [],
  onboard: null,
  read_errors: [],
};

const receiver: ReceiverReport = {
  product_id: 0xc548,
  name: "Logi Bolt receiver",
  collections: [],
  bolt: true,
  unifying: false,
  reachable: true,
  denied: false,
  open_slots: [1],
  slots: [],
  unsupported: [device],
};

let live: LogitechLive;
const wait = (ms: number) => vi.advanceTimersByTimeAsync(ms);

async function open(props: Record<string, unknown> = {}) {
  render(LogitechDevice, { props: { receiver, device, ...props } });
  await wait(0);
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.clearAllMocks();
  live = { battery: { percent: 80, estimated: false, state: "discharging" }, dpi: 1000, saved_dpi: null };
  api.logitechLive.mockImplementation(async () => live);
  api.logitechSetDpi.mockImplementation(async (_r: number, _s: number, dpi: number) => {
    live = { ...live, dpi, saved_dpi: dpi };
  });
  api.logitechResetDpi.mockImplementation(async () => {
    live = { ...live, dpi: 1000, saved_dpi: null };
  });
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

function dpiBox() {
  return screen.getByRole("spinbutton", { name: "DPI" }) as HTMLInputElement;
}

describe("a Logitech device PeriHub knows by its features", () => {
  it("shows what it is, what it can do, its battery and DPI now, and offers a test report", async () => {
    await open();
    expect(screen.getByRole("heading", { name: "Logitech MX Master 3S" })).toBeTruthy();
    expect(screen.getByText(/Mouse · through its Logi Bolt receiver, slot 1/)).toBeTruthy();
    expect(screen.getByText("80%")).toBeTruthy();
    expect(screen.getAllByText("1000")[0]).toBeTruthy();
    const items = screen.getAllByRole("listitem").map((li) => li.textContent);
    expect(items).toEqual([
      "Battery level",
      "Adjustable DPI",
      "SmartShift wheel",
      "High-resolution wheel",
      "Thumb wheel",
      "Programmable buttons and keys",
    ]);
    expect(screen.getByText(/And 1 more feature PeriHub has no name for yet/)).toBeTruthy();
    expect(screen.getByText("200 to 8000, in steps of 50")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Copy test report" })).toBeTruthy();
  });

  it("asks for the No warranty box before the first DPI change only", async () => {
    await open({ device: { ...device, name: "MX Anywhere 3" } });
    await fireEvent.input(dpiBox(), { target: { value: "1600" } });
    await fireEvent.click(screen.getByRole("button", { name: "Set DPI" }));
    expect(api.logitechSetDpi).not.toHaveBeenCalled();
    expect(screen.getByRole("note").textContent).toContain("accepts no responsibility for any damage");
    const confirm = screen.getAllByRole("button", { name: "Set DPI" }).at(-1) as HTMLButtonElement;
    expect(confirm.disabled).toBe(true);
    await fireEvent.click(screen.getByRole("checkbox", { name: /I have read this/ }));
    await fireEvent.click(confirm);
    await wait(0);
    expect(api.logitechSetDpi).toHaveBeenCalledWith(0xc548, 1, 1600);
    expect(screen.getByText("DPI set to 1600.")).toBeTruthy();
    expect(screen.getByText(/PeriHub sets 1600 DPI every time the MX Anywhere 3 connects/)).toBeTruthy();

    // The second change goes straight to the device.
    await fireEvent.input(dpiBox(), { target: { value: "2400" } });
    await fireEvent.click(screen.getByRole("button", { name: "Set DPI" }));
    await wait(0);
    expect(api.logitechSetDpi).toHaveBeenLastCalledWith(0xc548, 1, 2400);
    expect(screen.queryByRole("note")).toBeNull();
  });

  it("puts the device's own DPI back", async () => {
    acceptNoWarranty("dpi:MX Master 3S");
    live = { ...live, dpi: 1600, saved_dpi: 1600 };
    await open();
    await fireEvent.click(screen.getByRole("button", { name: "Back to the device's own DPI" }));
    await wait(0);
    expect(api.logitechResetDpi).toHaveBeenCalledWith(0xc548, 1);
    expect(screen.queryByText(/PeriHub sets 1600 DPI/)).toBeNull();
  });

  it("waits for a device that is asleep", async () => {
    await open({ receiver: { ...receiver, open_slots: [] } });
    expect(api.logitechLive).not.toHaveBeenCalled();
    expect(screen.getByText(/The MX Master 3S is asleep: move it or turn it on/)).toBeTruthy();
  });

  it("warns that Solaar may undo what PeriHub sets", async () => {
    await open({ programs: ["Solaar"] });
    expect(screen.getByText("Solaar is running.")).toBeTruthy();
  });

  it("only reads a device without adjustable DPI", async () => {
    await open({ device: { ...device, dpi: null } });
    expect(screen.getByText(/Not supported yet/)).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Set DPI" })).toBeNull();
  });

  it("shows the onboard memory format of a device that has one", async () => {
    const onboard = {
      memory_model: 1,
      profile_format: 5,
      macro_format: 1,
      profile_count: 5,
      rom_profile_count: 5,
      button_count: 13,
      sector_count: 16,
      sector_size: 255,
      has_gshift: false,
      has_dpi_shift: true,
    };
    await open({ device: { ...device, onboard } });
    expect(screen.getByText(/5 profiles, profile format 5,\s+255-byte sectors/)).toBeTruthy();
    // Its active profile sets the DPI: PeriHub does not change it.
    expect(screen.queryByRole("button", { name: "Set DPI" })).toBeNull();
    expect(screen.getByText(/Not supported yet/)).toBeTruthy();
  });

  it("says when the device is gone", async () => {
    await open({ receiver: null, device: null });
    expect(screen.getByText(/This device is not connected, or it is asleep/)).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Copy test report" })).toBeNull();
  });
});
