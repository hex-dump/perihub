import { describe, expect, it, vi } from "vitest";

vi.mock("./api", () => ({}));

import type { UnsupportedReceiverDevice } from "./api";
import { capabilities, kindIcon, kindName } from "./logitechFeatures";

/** The feature table of the MX Master 3S, as Solaar's device documentation lists it (ids only). */
const MX_MASTER_3S = [
  0x0000, 0x0001, 0x0003, 0x0005, 0x1d4b, 0x0020, 0x0021, 0x0007, 0x1004, 0x1b04, 0x1814, 0x1815, 0x2250, 0x2201,
  0x2110, 0x2121, 0x2150, 0x2251, 0x00c3, 0x1802, 0x1803,
];

const device = (ids: number[], hidden: number[] = []): UnsupportedReceiverDevice => ({
  slot: 1,
  name: "MX Master 3S",
  kind: 3,
  protocol: "4.5",
  wireless_id: null,
  device_info: null,
  firmware: [],
  features: ids.map((id) => ({ id, kind: hidden.includes(id) ? 0x40 : 0, version: 0 })),
  dpi: null,
  report_rates_ms: [],
  onboard: null,
  read_errors: [],
});

describe("capabilities", () => {
  it("names what an MX Master 3S can do, in a fixed order", () => {
    const { named } = capabilities(device(MX_MASTER_3S, [0x1802, 0x1803]));
    expect(named).toEqual([
      "Battery level",
      "Adjustable DPI",
      "SmartShift wheel",
      "High-resolution wheel",
      "Thumb wheel",
      "Programmable buttons and keys",
      "Easy-Switch between computers",
    ]);
  });

  it("counts the other visible features, but not the basic or hidden ones", () => {
    // 1d4b, 0020, 0021, 0007, 1815, 2250, 2251, 00c3 are not named; 1802 and 1803 are hidden.
    expect(capabilities(device(MX_MASTER_3S, [0x1802, 0x1803])).others).toBe(8);
    expect(capabilities(device([0x0000, 0x0001, 0x0003, 0x0005])).others).toBe(0);
  });

  it("names a capability once, whichever of its ids the device has", () => {
    expect(capabilities(device([0x1000, 0x1004, 0x8060, 0x8061])).named).toEqual(["Battery level", "Polling rate"]);
  });
});

describe("kinds", () => {
  it("follows Logitech's HID++ 2.0 specification, and numbers the kinds it does not list", () => {
    expect(kindName(0)).toBe("Keyboard");
    expect(kindName(3)).toBe("Mouse");
    expect(kindName(5)).toBe("Trackball");
    expect(kindName(9)).toBe("Type 9");
    expect(kindName(null)).toBe("Device");
    expect(kindIcon(0)).toBe("keyboard");
    expect(kindIcon(2)).toBe("keyboard");
    expect(kindIcon(3)).toBe("mouse");
  });
});
