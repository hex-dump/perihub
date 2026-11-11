import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const api = vi.hoisted(() => ({
  peripheralInfo: vi.fn(),
  peripheralChange: vi.fn(),
  peripheralKeepChanges: vi.fn(),
  peripheralUndo: vi.fn(),
  appSettings: vi.fn(),
}));

vi.mock("../lib/api", async (original) => ({ ...(await original<typeof import("../lib/api")>()), api }));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: async () => "0.7.0" }));
api.appSettings.mockResolvedValue({ system: "linux x86_64" });

import type { PeripheralInfo } from "../lib/api";
import { peripheralTest } from "../lib/testReport";
import Peripheral from "./Peripheral.svelte";

const info: PeripheralInfo = {
  key: "0b05:195c",
  vendor_id: 0x0b05,
  product_id: 0x195c,
  brand: "ASUS",
  model: "ROG Keris",
  kind: "mouse",
  source: "libratbag (MIT)",
  profiles: 3,
  firmware: "1.02.03",
  active_profile: 2,
  active_dpi: 2,
  settings: {
    dpi_levels: [400, 800, 1600, 3200],
    polling_hz: 1000,
    options: { debounce: 1, angle_snapping: 0 },
    leds: [
      { mode: 0, brightness: 100, color: "#ff0000" },
      { mode: 1, brightness: 50, color: "#0000ff" },
    ],
  },
  limits: {
    dpi: { min: 100, max: 16000, step: 100 },
    polling_hz: [125, 250, 500, 1000],
    options: [
      { id: "debounce", label: "Button response", values: [[0, "4 ms"], [1, "8 ms"]] },
      { id: "angle_snapping", label: "Angle snapping", values: [[0, "Off"], [1, "On"]] },
    ],
    leds: [
      { name: "LED 1", modes: [[0, "Static"], [1, "Breathing"], [2, "Colour cycle"]], brightness_step: 25, color: true },
      { name: "LED 2", modes: [[0, "Static"], [1, "Breathing"], [2, "Colour cycle"]], brightness_step: 25, color: true },
    ],
  },
  buttons: [{ name: "Left", action: "Left" }],
  editable: true,
  can_save: true,
  problem: null,
  pending_undo: false,
  raw: ["12 04 00 00 → 12 04 00 00 03 00"],
};

const wait = (ms: number) => vi.advanceTimersByTimeAsync(ms);

beforeEach(() => {
  vi.useFakeTimers();
  vi.clearAllMocks();
  localStorage.clear();
  api.peripheralInfo.mockResolvedValue(info);
  api.peripheralChange.mockImplementation(async (_key: string, settings: PeripheralInfo["settings"]) => ({
    ...info,
    settings,
  }));
  api.peripheralKeepChanges.mockResolvedValue(undefined);
  api.peripheralUndo.mockResolvedValue(info);
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

async function setPolling(hz: number) {
  const select = screen.getByRole("combobox", { name: "Polling rate" }) as HTMLSelectElement;
  select.value = String(hz);
  await fireEvent.change(select);
}

describe("a mouse of another brand", () => {
  it("shows what was read, says nobody has tried it, and changes nothing by itself", async () => {
    render(Peripheral, { props: { info, error: null } });
    await wait(0);
    expect(screen.getByText("ASUS ROG Keris")).toBeTruthy();
    expect(document.body.textContent).toContain("Nobody has tried PeriHub on the ASUS ROG Keris yet");
    expect(document.body.textContent).toContain("libratbag (MIT)");
    expect(screen.getByText("1.02.03")).toBeTruthy();
    expect(api.peripheralChange).not.toHaveBeenCalled();
    expect((screen.getByRole("button", { name: "Apply" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("always shows the warning next to Apply, without asking for anything more", async () => {
    render(Peripheral, { props: { info, error: null } });
    expect(document.body.textContent).toContain("sends the change without saving it to the device's memory");
    await setPolling(500);
    await fireEvent.click(screen.getByRole("button", { name: "Apply" }));
    await wait(0);
    expect(api.peripheralChange).toHaveBeenCalledWith("0b05:195c", { ...info.settings, polling_hz: 500 }, false);
    expect(screen.queryByRole("checkbox")).toBeNull();
  });

  it("puts the settings back after 15 seconds unless they are kept", async () => {
    render(Peripheral, { props: { info, error: null } });
    await setPolling(250);
    await fireEvent.click(screen.getByRole("button", { name: "Apply" }));
    await wait(0);
    expect(screen.getByRole("alertdialog")).toBeTruthy();
    await wait(15_000);
    expect(api.peripheralUndo).toHaveBeenCalledWith("0b05:195c");
    expect(api.peripheralKeepChanges).not.toHaveBeenCalled();
  });

  it("keeps the change when asked", async () => {
    render(Peripheral, { props: { info, error: null } });
    await setPolling(250);
    await fireEvent.click(screen.getByRole("button", { name: "Apply" }));
    await wait(0);
    await fireEvent.click(screen.getByRole("button", { name: "Keep changes" }));
    await wait(20_000);
    expect(api.peripheralKeepChanges).toHaveBeenCalledWith("0b05:195c");
    expect(api.peripheralUndo).not.toHaveBeenCalled();
  });

  it("saves to memory only after the No warranty box, asked once", async () => {
    render(Peripheral, { props: { info, error: null } });
    await fireEvent.click(screen.getByRole("button", { name: "Save to memory…" }));
    const save = screen.getByRole("button", { name: "Save to memory" }) as HTMLButtonElement;
    expect(save.disabled).toBe(true);
    expect(screen.getByRole("note").textContent).toContain("accepts no responsibility for any damage");
    expect(api.peripheralChange).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByRole("checkbox"));
    await fireEvent.click(save);
    await wait(0);
    expect(api.peripheralChange).toHaveBeenCalledWith("0b05:195c", info.settings, true);
    await fireEvent.click(screen.getByRole("button", { name: "Keep changes" }));
    await wait(0);
    await fireEvent.click(screen.getByRole("button", { name: "Save to memory…" }));
    await wait(0);
    expect(api.peripheralChange).toHaveBeenCalledTimes(2);
    expect(screen.queryByRole("checkbox")).toBeNull();
  });

  it("offers no change on a device that answered something unexpected", () => {
    render(Peripheral, {
      props: { info: { ...info, editable: false, problem: "unexpected answer: polling rate index 9" }, error: null },
    });
    expect(document.body.textContent).toContain("polling rate index 9");
    expect(screen.queryByRole("button", { name: "Apply" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Save to memory…" })).toBeNull();
    expect(screen.queryByRole("combobox")).toBeNull();
  });

  it("does not offer to change the DPI where the device's DPI is not changed", () => {
    render(Peripheral, { props: { info: { ...info, limits: { ...info.limits, dpi: null } }, error: null } });
    expect(screen.queryByRole("spinbutton")).toBeNull();
    expect(document.body.textContent).toContain("not changed on this device");
  });

  it("explains when the device could not be opened", () => {
    render(Peripheral, { props: { info: null, error: "ASUS ROG Keris: permission denied" } });
    expect(screen.getByText("ASUS ROG Keris: permission denied")).toBeTruthy();
  });

  it("puts every request and answer in the test report", () => {
    const report = peripheralTest(info, "0.7.0", "Linux").report;
    expect(report).toContain("ASUS ROG Keris (mouse), USB ID 0b05:195c, protocol from libratbag (MIT)");
    expect(report).toContain("DPI levels: 400 / 800 / 1600 / 3200, polling: 1000 Hz");
    expect(report).toContain("Button response: 8 ms, Angle snapping: Off");
    expect(report).toContain("  12 04 00 00 → 12 04 00 00 03 00");
  });
});
