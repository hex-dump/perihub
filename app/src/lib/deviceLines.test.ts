import { describe, expect, it } from "vitest";
import type { DeviceStatus } from "./api";
import { deviceLines } from "./deviceLines";

const none = {
  keyboard: null, mouse: null, keyboard_error: null, mouse_error: null, mouse_asleep: null, unsupported_keyboard: null,
  conflicts: [], g300: null, g300_error: null, corsair: [], corsair_error: null, peripherals: [], peripheral_error: null,
  qmk: [], receivers: [], logitech_programs: [], sandboxed: false, scanned: true,
} as DeviceStatus;

describe("deviceLines", () => {
  it("does not name a keyboard that is not there", () => {
    expect(deviceLines(none)).toEqual(["No supported device connected."]);
    expect(deviceLines({ ...none, scanned: false })).toEqual(["Looking for devices…"]);
  });

  it("lists devices of every kind by their own names", () => {
    const lines = deviceLines({
      ...none,
      keyboard: { name: "SteelSeries Apex 3 TKL", firmware: "3.0.4" },
      corsair: [{ model: "K70", firmware: "1.2", kind: "keyboard" } as never],
      peripherals: [{ brand: "ASUS", model: "ROG Keris", firmware: null, kind: "mouse" } as never],
      g300: { model: "Logitech G300" } as never,
    });
    expect(lines).toEqual([
      "SteelSeries Apex 3 TKL: firmware 3.0.4",
      "Logitech G300: connected",
      "K70: firmware 1.2",
      "ASUS ROG Keris: connected",
    ]);
  });

  it("keeps the errors", () => {
    expect(deviceLines({ ...none, mouse_error: "G502: permission denied" })).toEqual(["G502: permission denied"]);
  });
});
