import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const api = vi.hoisted(() => ({
  usbRecordStart: vi.fn(),
  usbRecordStop: vi.fn(),
  usbRecordCancel: vi.fn(),
  onUsbRecord: vi.fn(),
}));

vi.mock("../lib/api", async (original) => ({ ...(await original<typeof import("../lib/api")>()), api }));

import type { ReportDevice, UsbRecordProgress, UsbRecorderInfo } from "../lib/api";
import { PUT_BACK } from "../lib/usbRecording";
import UsbRecorder from "./UsbRecorder.svelte";

const device: ReportDevice = {
  vendor_id: 0x1234,
  product_id: 0x00ab,
  manufacturer: "Maker",
  product: "Maker Pro Mouse",
  bus: "USB",
  collections: [],
  release: null,
  usb_interfaces: null,
  descriptors: [],
    feature_reports: null,
};

const windows: UsbRecorderInfo = { available: true, reason: null, tool: "USBPcap", windows: true, leftover: false };

let send: (p: UsbRecordProgress) => void = () => {};
const flush = () => new Promise((r) => setTimeout(r, 0));

beforeEach(() => {
  vi.clearAllMocks();
  api.usbRecordStart.mockResolvedValue(undefined);
  api.usbRecordStop.mockResolvedValue(undefined);
  api.usbRecordCancel.mockResolvedValue(undefined);
  api.onUsbRecord.mockImplementation(async (handler: (p: UsbRecordProgress) => void) => {
    send = handler;
    return () => {};
  });
});

afterEach(cleanup);

function setup(info = windows) {
  const onrecorded = vi.fn();
  render(UsbRecorder, { device, info, onrecorded, onclose: vi.fn() });
  return onrecorded;
}

describe("Record the official software", () => {
  it("says what is installed and what is never recorded, and waits for consent and a step", async () => {
    setup();
    expect(screen.getByText(/for this recording only/)).toBeTruthy();
    expect(screen.getByText(/disconnect for a couple of seconds twice/)).toBeTruthy();
    expect(screen.getByText(/Never what you/)).toBeTruthy();
    const start = screen.getByRole("button", { name: "Start recording" }) as HTMLButtonElement;
    expect(start.disabled).toBe(true);
    await fireEvent.click(screen.getByLabelText("DPI"));
    expect(start.disabled).toBe(true);
    await fireEvent.click(screen.getByLabelText(/I understand that USBPcap is installed/));
    expect(start.disabled).toBe(false);
  });

  it("goes through the steps, marks when each one started, and hands over the text", async () => {
    const onrecorded = setup();
    await fireEvent.click(screen.getByLabelText("Lighting"));
    await fireEvent.click(screen.getByLabelText(/I understand/));
    await fireEvent.click(screen.getByRole("button", { name: "Start recording" }));
    await flush();
    expect(api.usbRecordStart).toHaveBeenCalledWith(device);
    send({ state: "status", text: "Installing USBPcap…" });
    await flush();
    expect(screen.getByText("Installing USBPcap…")).toBeTruthy();
    send({ state: "recording", text: "" });
    await flush();
    expect(screen.getByText("Step 1 of 2")).toBeTruthy();
    await fireEvent.input(screen.getByLabelText(/What you changed/), { target: { value: "Breathing" } });
    await fireEvent.click(screen.getByRole("button", { name: "Done, next step" }));
    expect(screen.getByText("Step 2 of 2")).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Done, finish" }));
    await flush();
    const steps = api.usbRecordStop.mock.calls[0][0] as { at_ms: number; name: string }[];
    expect(steps.map((s) => s.name)).toEqual(["Lighting (Breathing)", PUT_BACK]);
    expect(steps[0].at_ms).toBeLessThanOrEqual(steps[1].at_ms);
    send({ state: "cleanup", text: "USBPcap was removed: nothing of it is left." });
    send({ state: "done", text: "USB recording of 1234:00ab" });
    await flush();
    expect(onrecorded).toHaveBeenCalledWith("USB recording of 1234:00ab");
    expect(screen.getByText("USBPcap was removed: nothing of it is left.")).toBeTruthy();
  });

  it("can be cancelled, and says why it cannot record here", async () => {
    setup();
    await fireEvent.click(screen.getByLabelText("DPI"));
    await fireEvent.click(screen.getByLabelText(/I understand/));
    await fireEvent.click(screen.getByRole("button", { name: "Start recording" }));
    await flush();
    send({ state: "recording", text: "" });
    await flush();
    await fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(api.usbRecordCancel).toHaveBeenCalled();
    send({ state: "cancelled", text: "" });
    await flush();
    expect(screen.getByText(/nothing was kept/)).toBeTruthy();
    cleanup();
    setup({ ...windows, windows: false, available: false, reason: "PeriHub runs as a Flatpak" });
    expect(screen.getByText(/not available here: PeriHub runs as a Flatpak/)).toBeTruthy();
  });
});
