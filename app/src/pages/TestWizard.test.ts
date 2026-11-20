import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

/** Every command of the app: the wizard may call the readings, and nothing else. */
const readings = vi.hoisted(() => ({
  devicesForReport: vi.fn(),
  appSettings: vi.fn(),
  status: vi.fn(),
  mouseLive: vi.fn(),
  g300Info: vi.fn(),
  peripheralInfo: vi.fn(),
  logitechLive: vi.fn(),
  writeLog: vi.fn(),
  openTestForm: vi.fn(),
  usbRecordInfo: vi.fn(),
  onUsbRecord: vi.fn(),
}));
const writes = vi.hoisted(() => ({
  peripheralChange: vi.fn(),
  peripheralSetBlind: vi.fn(),
  peripheralKeepChanges: vi.fn(),
  peripheralUndo: vi.fn(),
  g300Save: vi.fn(),
  saveMouseProfile: vi.fn(),
  setLighting: vi.fn(),
  corsairSetLighting: vi.fn(),
  setLogitechDpi: vi.fn(),
  previewDpi: vi.fn(),
  applyKeymap: vi.fn(),
}));
const api = vi.hoisted(() => ({}) as Record<string, unknown>);
Object.assign(api, readings, writes);

vi.mock("../lib/api", async (original) => ({ ...(await original<typeof import("../lib/api")>()), api }));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: async () => "0.9.0" }));

import type { DeviceStatus, PeripheralInfo } from "../lib/api";
import TestWizard from "./TestWizard.svelte";

const peripheral = {
  key: "0b05:195c", vendor_id: 0x0b05, product_id: 0x195c, brand: "ASUS", model: "ROG Keris", kind: "mouse",
  source: "libratbag (MIT)", profiles: 3, firmware: "1.02.03", battery: null, active_profile: 1, active_dpi: 1,
  readable: true,
  settings: { dpi_levels: [800], polling_hz: 1000, options: {}, leds: [] },
  limits: { dpi: { min: 100, max: 16000, step: 100 }, polling_hz: [1000], options: [], leds: [] },
  buttons: [], details: [["Saved in", "/home/ann/.config/perihub"]], blind: [], editable: true, can_save: true,
  changes_stay: false, problem: null, pending_undo: false, raw: ["12 04 00 00 → 12 04 00 00 03 00"],
} as unknown as PeripheralInfo;

const status = (part: Partial<DeviceStatus> = {}) =>
  ({ mouse: null, g300: null, corsair: [], peripherals: [peripheral], receivers: [], qmk: [], scanned: true, ...part }) as DeviceStatus;

const clipboard = { writeText: vi.fn().mockResolvedValue(undefined) };

beforeEach(() => {
  vi.clearAllMocks();
  Object.defineProperty(navigator, "clipboard", { value: clipboard, configurable: true });
  readings.devicesForReport.mockResolvedValue([]);
  readings.appSettings.mockResolvedValue({
    system: "linux x86_64", config_dir: "/home/ann/.config/perihub", log_dir: "/home/ann/.local/state/perihub",
  });
  readings.status.mockResolvedValue(status());
  readings.writeLog.mockResolvedValue([]);
  readings.peripheralInfo.mockResolvedValue(peripheral);
  readings.openTestForm.mockResolvedValue(undefined);
  readings.usbRecordInfo.mockResolvedValue({ available: true, reason: null, tool: "usbmon", windows: false, leftover: false });
  readings.onUsbRecord.mockResolvedValue(() => {});
});
afterEach(cleanup);

const click = (name: string | RegExp) => fireEvent.click(screen.getByRole("button", { name }));
const settle = () => new Promise((r) => setTimeout(r, 0));

/** Picks the device and goes through the reading to the preview. */
async function toPreview() {
  render(TestWizard, { props: { status: status() } });
  await settle();
  await fireEvent.click(screen.getByRole("radio", { name: /ASUS ROG Keris/ }));
  await click("Next");
  await settle();
  await click("Next");
}

describe("the test wizard", () => {
  it("lists the connected devices and waits for a pick", async () => {
    render(TestWizard, { props: { status: status() } });
    await settle();
    expect(screen.getByText("ASUS ROG Keris")).toBeTruthy();
    expect((screen.getByRole("button", { name: "Next" }) as HTMLButtonElement).disabled).toBe(true);
    await fireEvent.click(screen.getByRole("radio", { name: /ASUS ROG Keris/ }));
    expect((screen.getByRole("button", { name: "Next" }) as HTMLButtonElement).disabled).toBe(false);
  });

  it("starts with the device it was asked for, already read, and goes back to the list from there", async () => {
    render(TestWizard, { props: { status: status(), start: "peripheral:0b05:195c" } });
    await settle();
    expect(readings.peripheralInfo).toHaveBeenCalledWith("0b05:195c");
    expect(document.body.textContent).toContain("PeriHub only reads ASUS ROG Keris here");
    await click("Back");
    expect(screen.getByRole("radio", { name: /ASUS ROG Keris/ })).toBeTruthy();
  });

  it("starts at the list when the device it was asked for is not there", async () => {
    render(TestWizard, { props: { status: status(), start: "peripheral:1532:0099" } });
    await settle();
    expect(readings.peripheralInfo).not.toHaveBeenCalled();
    expect(screen.getByRole("radio", { name: /ASUS ROG Keris/ })).toBeTruthy();
  });

  it("says so when no device is found", async () => {
    render(TestWizard, { props: { status: status({ peripherals: [] }) } });
    await settle();
    expect(document.body.textContent).toContain("PeriHub found no device");
  });

  it("reads the device, shows the report with the private details hidden, and then the checklist", async () => {
    await toPreview();
    expect(readings.peripheralInfo).toHaveBeenCalledWith("0b05:195c");
    expect(document.body.textContent).toContain("Saved in: /home/[user]/.config/perihub");
    expect(document.body.textContent).not.toContain("/home/ann");
    expect(document.body.textContent).toContain("Hidden before sharing: 1 user name in a path");
    await click("Next");
    const select = screen.getByLabelText("The DPI PeriHub shows matches the mouse") as HTMLSelectElement;
    select.value = "worked";
    await fireEvent.change(select);
    await click("Next");
    expect(document.body.textContent).toContain("What I tried:");
    expect(document.body.textContent).toContain("worked: The DPI PeriHub shows matches the mouse");
  });

  it("copies the report and opens the form of PeriHub with it filled in", async () => {
    await toPreview();
    await click("Next");
    await click("Next");
    await click("Copy report");
    expect(clipboard.writeText.mock.calls[0][0]).toContain("PeriHub 0.9.0 on linux x86_64");
    await click("Open the form");
    await settle();
    const url = readings.openTestForm.mock.calls[0][0] as string;
    expect(url.startsWith("https://github.com/hex-dump/perihub/issues/new?template=")).toBe(true);
    const query = new URL(url).searchParams;
    expect(query.get("model")).toBe("ASUS ROG Keris");
    expect(query.get("report")).toContain("/home/[user]/.config/perihub");
    expect(document.body.textContent).toContain("The form is open in your browser");
  });

  it("copies a report that is too long for a link, and opens the form without it", async () => {
    const raw = Array.from({ length: 400 }, (_, i) => `12 04 00 ${i % 100} → 12 04 00 00 03 00 8f 91 aa bb cc dd ee ff`);
    const long = { ...peripheral, raw };
    readings.status.mockResolvedValue(status({ peripherals: [long] }));
    readings.peripheralInfo.mockResolvedValue(long);
    await toPreview();
    await click("Next");
    await click("Next");
    expect(document.body.textContent).toContain("too long for a link");
    await click("Copy report and open the form");
    await settle();
    expect(clipboard.writeText.mock.calls[0][0]).toContain("12 04 00 99");
    const url = readings.openTestForm.mock.calls[0][0] as string;
    expect(url.length).toBeLessThan(7000);
    expect(new URL(url).searchParams.get("report")).toContain("Paste the report here");
  });

  it("does not open the form when the report cannot be copied", async () => {
    clipboard.writeText.mockRejectedValueOnce("denied");
    const raw = Array.from({ length: 400 }, () => "12 04 00 00 → 12 04 00 00 03 00 8f 91 aa bb cc dd ee ff");
    const long = { ...peripheral, raw };
    readings.status.mockResolvedValue(status({ peripherals: [long] }));
    readings.peripheralInfo.mockResolvedValue(long);
    await toPreview();
    await click("Next");
    await click("Next");
    await click("Copy report and open the form");
    await settle();
    expect(readings.openTestForm).not.toHaveBeenCalled();
    expect(document.body.textContent).toContain("denied");
  });

  it("stays on the reading when the device is gone", async () => {
    render(TestWizard, { props: { status: status() } });
    await settle();
    await fireEvent.click(screen.getByRole("radio", { name: /ASUS ROG Keris/ }));
    readings.status.mockResolvedValue(status({ peripherals: [] }));
    await click("Next");
    await settle();
    expect(document.body.textContent).toContain("not there any more");
    expect((screen.getByRole("button", { name: "Next" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("only reads: nothing that writes to a device is ever called", async () => {
    await toPreview();
    await click("Next");
    await click("Next");
    await click("Copy report");
    await click("Open the form");
    await settle();
    for (const [name, fn] of Object.entries(writes)) expect(fn, name).not.toHaveBeenCalled();
    // And only the readings of the list were used.
    const used = Object.entries(readings).filter(([, fn]) => fn.mock.calls.length > 0).map(([name]) => name);
    expect(used.sort()).toEqual(["appSettings", "devicesForReport", "openTestForm", "peripheralInfo", "status", "usbRecordInfo", "writeLog"]);
  });

  it("adds the writes of this session to the report, private details hidden", async () => {
    readings.writeLog.mockResolvedValue([
      { time: 0, device: "ROG Keris", kind: "dpi", data: "01", bytes: 1, result: "ok" },
      { time: 0, device: "ROG Keris", kind: "save", data: "02", bytes: 1, result: "refused (read-only mode)" },
      { time: 0, device: "ROG Keris", kind: "backup", data: "03", bytes: 1, result: "cannot write /home/ann/x" },
    ]);
    await toPreview();
    await settle();
    const text = document.querySelector("pre.report")?.textContent ?? "";
    expect(text).toContain("Writes this session: 2 sent, 1 refused:");
    expect(text).toContain("ROG Keris: save, 1 bytes, refused (read-only mode)");
    expect(text).toContain("cannot write /home/[user]/x");
    expect(text).not.toContain("/home/ann");
  });

  it("offers a recording of the official software for a USB device PeriHub does not know", async () => {
    const zap = {
      vendor_id: 0x048d, product_id: 0x5702, manufacturer: "ITE Tech. Inc.", product: "ITE Device", bus: "USB",
      collections: [{ interface: 0, usage_page: 0xff89, usage: 0xcc }], release: null, usb_interfaces: null,
      descriptors: [{ interface: 0, hex: "0689ff09cca1018507150026ff00", collections: null }], feature_reports: null,
    };
    readings.devicesForReport.mockResolvedValue([zap]);
    readings.status.mockResolvedValue(status({ peripherals: [] }));
    render(TestWizard, { props: { status: status({ peripherals: [] }) } });
    await settle();
    await fireEvent.click(screen.getByRole("radio", { name: /ITE Device/ }));
    await click("Next");
    await settle();
    expect(document.body.textContent).toContain("Record its software");
    await click("Next");
    expect(document.body.textContent).toContain("is what lets it be added");
    await click("Record the official software");
    await settle();
    expect(screen.getByRole("region", { name: "Record the official software" })).toBeTruthy();
    expect((screen.getByRole("button", { name: "Next" }) as HTMLButtonElement).disabled).toBe(true);
    await click("Close");
    // Skipped: the request goes on without a recording, and its raw descriptor is not taken for an id.
    await click("Next");
    await settle();
    const text = document.querySelector("pre.report")?.textContent ?? "";
    expect(text).toContain("interface 0: 0689ff09cca1018507150026ff00");
    expect(text).not.toContain("[id hidden]");
  });
});
