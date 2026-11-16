import { describe, expect, it, vi } from "vitest";

vi.mock("./api", () => ({}));

import type { UnsupportedReceiverDevice } from "./api";
import { capabilities, kindIcon, kindName, keyGroups, lightsDrivable, perKeyKeys, waveDirections } from "./logitechFeatures";

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
  lighting: null,
  led_zones: [],
  battery: null,
  battery_reply: null,
  read_only: false,
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
    // A headset (issue #41): its battery voltage, sidetone and equalizer.
    expect(capabilities(device([0x1f20, 0x8300, 0x8310])).named).toEqual(["Battery level", "Sidetone", "Equalizer"]);
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

describe("driving a keyboard's lights", () => {
  it("puts the keys in groups by their ids", () => {
    const keys = [...Array.from({ length: 111 }, (_, i) => i + 1), 153, 155, 156, 157, 158, 180, 181, 210];
    const groups = keyGroups(keys);
    expect(groups.map((g) => [g.name, g.keys.length])).toEqual([
      ["Letters", 26],
      ["Numbers", 10],
      ["F keys", 12],
      ["Arrows and navigation", 13],
      ["Numpad", 17],
      ["Other keys", 25],
      ["Ctrl, Shift, Alt and Windows", 8],
      ["Media and brightness", 5],
      ["G keys", 2],
      ["Logo", 1],
    ]);
    expect(groups.flatMap((g) => g.keys).sort((a, b) => a - b)).toEqual(keys);
    expect(keyGroups([1, 2]).map((g) => g.name)).toEqual(["Letters"]);
  });

  it("leaves the logo out of the colours per key when the keyboard has a logo zone", () => {
    const zones = (locations: number[]) => ({ zones: locations.map((location) => ({ location })) });
    expect(perKeyKeys({ keys: [1, 2, 210], effects: zones([1, 2]) })).toEqual([1, 2]);
    expect(perKeyKeys({ keys: [1, 2, 210], effects: zones([1]) })).toEqual([1, 2, 210]);
    expect(keyGroups(perKeyKeys({ keys: [1, 210], effects: zones([1, 2]) })).map((g) => g.name)).toEqual(["Letters"]);
  });

  it("offers only the wave directions the keyboard lists", () => {
    // 0xdce1: a G915 X LS has no inward and outward wave.
    expect(waveDirections(0xdce1).map((d) => d.value)).toEqual([0, 1, 6, 2, 7, 3, 8]);
    expect(waveDirections(0).map((d) => d.value)).toEqual([0]);
  });

  it("drives the lights of a keyboard with 0x8071 and 0x8101 only", () => {
    const zones = [{ index: 0, location: 1, effects: [] }];
    const lighting = { brightness: null, effects: { zones, startup: null, shutdown: null, idle_s: null, sleep_s: null }, keys: [], layout: null, raw: [], errors: [] };
    const base = {
      slot: 1,
      name: "G915 X LS",
      kind: 0,
      protocol: "4.2",
      wireless_id: null,
      device_info: null,
      firmware: [],
      dpi: null,
      report_rates_ms: [],
      onboard: null,
      read_errors: [],
      lighting,
      led_zones: [],
      battery: null,
      battery_reply: null,
      read_only: false,
    };
    const features = (ids: number[]) => ids.map((id) => ({ id, kind: 0, version: 0 }));
    expect(lightsDrivable({ ...base, features: features([0x8071, 0x8101]) })).toBe(true);
    expect(lightsDrivable({ ...base, features: features([0x8071]) })).toBe(false);
    expect(lightsDrivable({ ...base, features: features([0x8071, 0x8101]), lighting: { ...lighting, effects: null } })).toBe(false);
    const dpi = { min: 200, max: 8000, step: 50, list: [] };
    expect(lightsDrivable({ ...base, features: features([0x8071, 0x8101]), dpi })).toBe(false);
    // A device PeriHub only reads (on its own USB connection, issue #40).
    expect(lightsDrivable({ ...base, features: features([0x8071, 0x8101]), read_only: true })).toBe(false);
  });
});
