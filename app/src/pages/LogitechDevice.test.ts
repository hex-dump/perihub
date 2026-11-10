import { cleanup, render, screen } from "@testing-library/svelte";
import { afterEach, describe, expect, it, vi } from "vitest";

const api = vi.hoisted(() => ({ appSettings: vi.fn(async () => ({ system: "linux x86_64" })), openTestForm: vi.fn() }));
vi.mock("../lib/api", async (original) => ({ ...(await original<typeof import("../lib/api")>()), api }));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: async () => "0.6.7" }));

import type { ReceiverReport, UnsupportedReceiverDevice } from "../lib/api";
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
  slots: [],
  unsupported: [device],
};

afterEach(cleanup);

describe("a Logitech device PeriHub only reads", () => {
  it("shows what it is and what it can do, and offers a test report", () => {
    render(LogitechDevice, { props: { receiver, device } });
    expect(screen.getByRole("heading", { name: "Logitech MX Master 3S" })).toBeTruthy();
    expect(screen.getByText(/Mouse · through its Logi Bolt receiver, slot 1/)).toBeTruthy();
    expect(screen.getByText("Read-only")).toBeTruthy();
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

  it("shows the onboard memory format of a device that has one", () => {
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
    render(LogitechDevice, { props: { receiver, device: { ...device, onboard } } });
    expect(screen.getByText(/5 profiles, profile format 5,\s+255-byte sectors/)).toBeTruthy();
  });

  it("says when the device is gone", () => {
    render(LogitechDevice, { props: { receiver: null, device: null } });
    expect(screen.getByText(/This device is not connected, or it is asleep/)).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Copy test report" })).toBeNull();
  });
});
