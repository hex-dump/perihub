import { describe, expect, it, vi } from "vitest";

vi.mock("./api", () => ({}));

import type { MouseInfo, ReceiverReport } from "./api";
import { deviceTestFormUrl, receiverTest, testFormUrl, testReport } from "./testReport";

const cable: MouseInfo = {
  product_id: 0xc08b,
  model: "G502 HERO",
  name: "G502 HERO Gaming Mouse",
  verified: true,
  experimental: false,
  pending_undo: false,
  receiver: false,
  link: null,
  host_mode: false,
  firmware: [{ kind: 0, name: "U1", version: "27.2.1" }],
  dpi: { min: 100, max: 25600, step: 50, list: [] },
  report_rates_ms: [1, 2, 4, 8],
  led_zones: [{ index: 0, location: 1, effects: [0, 1, 3] }],
  features: [
    { id: 0x0001, kind: 0, version: 1 },
    { id: 0x8071, kind: 0x40, version: 4 },
  ],
  onboard: {
    memory_model: 1,
    profile_format: 2,
    macro_format: 1,
    profile_count: 5,
    rom_profile_count: 5,
    button_count: 11,
    sector_count: 16,
    sector_size: 256,
    has_gshift: true,
    has_dpi_shift: true,
  },
  writable: true,
  writable_format: true,
  switches_back: true,
  profile_list: "own",
};

const wireless: MouseInfo = {
  ...cable,
  product_id: 0x4093,
  model: "G502 X Lightspeed",
  verified: false,
  writable: false,
  receiver: true,
  link: { receiver_id: 0xc547, slot: 1, wireless_id: 0x4093 },
};

describe("testReport", () => {
  it("describes a mouse on its cable", () => {
    const report = testReport(cable, "0.5.2", "linux x86_64");
    expect(report).toContain("PeriHub 0.5.2 on linux x86_64");
    expect(report).toContain("USB ID: 046d:c08b");
    expect(report).toContain("Connection: USB cable");
    expect(report).toContain("Support: tested");
    expect(report).toContain("Firmware: U1 27.2.1");
    expect(report).toContain("Profiles: 5, buttons: 11");
    expect(report).toContain(
      "Memory: model 1, profile format 2, macro format 1, 16 sectors of 256 bytes, 5 factory profiles",
    );
    expect(report).toContain("Profile list: saved in the mouse");
    expect(report).toContain("DPI: 100-25600, step 50");
    expect(report).toContain("LEDs: zone 0 at 1, effects 0 1 3");
  });

  it("says when the mouse had no profile list of its own", () => {
    const report = testReport({ ...cable, profile_list: "factory" }, "0.6.3", "linux x86_64");
    expect(report).toContain("Profile list: none when connected (factory profiles in use)");
    const damaged = testReport({ ...cable, profile_list: "damaged" }, "0.6.3", "linux x86_64");
    expect(damaged).toContain("Profile list: damaged when connected (checksum does not match)");
  });

  it("gives the receiver, the slot and the wireless id of a mouse behind a receiver", () => {
    const report = testReport(wireless, "0.5.2", "windows x86_64");
    expect(report).toContain("USB ID: 046d:c547");
    expect(report).toContain("Connection: LIGHTSPEED receiver 046d:c547, slot 1, wireless id 4093");
    expect(report).toContain("Support: read-only on this connection");
  });

  it("says how far PeriHub goes with the mouse's memory format", () => {
    const support = (mouse: MouseInfo) => testReport(mouse, "0.6.7", "linux x86_64");
    expect(support({ ...cable, verified: false, experimental: true })).toContain(
      "Support: experimental (memory format not tested yet)",
    );
    expect(support({ ...cable, receiver: true, writable: false })).toContain(
      "Support: tested, no saves on this connection",
    );
    expect(support({ ...cable, verified: false, writable: false, writable_format: false })).toContain(
      "Support: read-only (memory format PeriHub cannot write)",
    );
  });

  it("lists fixed DPI values and says when a mouse reports no LEDs or polling rates", () => {
    const report = testReport(
      {
        ...cable,
        dpi: { min: 400, max: 1600, step: 0, list: [400, 800, 1600] },
        led_zones: [],
        report_rates_ms: [],
      },
      "0.5.2",
      "linux x86_64",
    );
    expect(report).toContain("DPI: 400, 800, 1600");
    expect(report).toContain("LEDs: none reported");
    expect(report).toContain("Polling (ms): none reported");
  });

  // Issue #22: "LEDs: none reported" on a G903 HERO; the feature list shows what it has instead.
  it("lists the mouse's HID++ features, or says they were not read", () => {
    expect(testReport(cable, "0.8.3", "linux x86_64")).toContain("Features: 0001 v1, 8071 v4 (flags 40)");
    expect(testReport({ ...cable, features: [] }, "0.8.3", "linux x86_64")).toContain("Features: not read");
  });
});

describe("testFormUrl", () => {
  const fields = (url: string) => new URL(url).searchParams;

  it("opens the mouse test form with the fields filled in", () => {
    const url = testFormUrl(wireless, "0.5.2", "windows x86_64");
    expect(url.startsWith("https://github.com/hex-dump/perihub/issues/new?template=mouse_test.yml&")).toBe(true);
    const query = fields(url);
    // GitHub wants a title before the report can be sent.
    expect(query.get("title")).toBe("G502 X Lightspeed test report");
    expect(query.get("model")).toBe("G502 X Lightspeed");
    expect(query.get("usb-id")).toBe("046d:c547");
    expect(query.get("connection")).toBe("LIGHTSPEED receiver");
    expect(query.get("version")).toBe("0.5.2");
    expect(query.get("os")).toBe("Windows");
    expect(query.get("report")).toBe(testReport(wireless, "0.5.2", "windows x86_64"));
  });

  it("names the cable and Linux", () => {
    const query = fields(testFormUrl(cable, "0.5.2", "linux x86_64"));
    expect(query.get("connection")).toBe("USB cable");
    expect(query.get("os")).toBe("Linux");
  });

  it("keeps a name with symbols inside its own field", () => {
    const query = fields(testFormUrl({ ...cable, name: 'a&model=b "c"' }, "0.5.2", "linux x86_64"));
    expect(query.get("model")).toBe("G502 HERO");
    expect(query.get("report")).toContain('reports itself as "a&model=b "c""');
  });
});

describe("receiverTest", () => {
  const bolt: ReceiverReport = {
    product_id: 0xc548,
    name: "Logi Bolt receiver",
    collections: ["interface 2, usage page ff00, usage 0001", "interface 2, usage page ff00, usage 0002"],
    bolt: true,
    unifying: false,
    reachable: true,
    denied: false,
    open_slots: [1],
    slots: ["slot 1: HID++ 4.5", "slot 2: HID++ 1.0 error 0x08"],
    unsupported: [
      {
        slot: 1,
        name: "MX Master 3S",
        kind: 3,
        protocol: "4.5",
        wireless_id: null,
        device_info: "02 xx xx xx xx 00 02 12 34 00 00 00 00 00 00 00",
        firmware: ["RBM 12.01.0008"],
        features: [
          { id: 0x0001, kind: 0, version: 1 },
          { id: 0x2201, kind: 0x40, version: 2 },
        ],
        dpi: { min: 200, max: 8000, step: 50, list: [] },
        report_rates_ms: [],
        onboard: null,
        lighting: null,
        read_errors: [],
      },
    ],
  };

  it("describes the receiver and the device paired with it", () => {
    const fields = receiverTest(bolt, bolt.unsupported[0], "0.6.6", "windows x86_64");
    expect(fields.model).toBe("Logitech MX Master 3S");
    expect(fields.usbId).toBe("046d:c548");
    expect(fields.connection).toBe("Logi Bolt receiver");
    expect(fields.report.split("\n")).toEqual([
      "PeriHub 0.6.6 on windows x86_64",
      "Receiver: Logi Bolt receiver, USB ID 046d:c548",
      "HID collections: interface 2, usage page ff00, usage 0001; interface 2, usage page ff00, usage 0002",
      "PeriHub reaches its paired devices: yes",
      "Slots: slot 1: HID++ 4.5; slot 2: HID++ 1.0 error 0x08",
      "Slot 1: MX Master 3S, HID++ 4.5",
      "Kind: Mouse (3)",
      "Device info (0x0003): 02 xx xx xx xx 00 02 12 34 00 00 00 00 00 00 00",
      "Firmware: RBM 12.01.0008",
      "Features: 0001 v1, 2201 v2 (flags 40)",
      "DPI: 200-8000, step 50",
      "Polling (ms): none reported",
      "Onboard memory: none",
      "Could not read: nothing",
    ]);
    const query = new URL(deviceTestFormUrl(fields, "0.6.6", "windows x86_64")).searchParams;
    expect(query.get("title")).toBe("Logitech MX Master 3S test report");
  });

  it("still describes a receiver PeriHub cannot reach", () => {
    const fields = receiverTest(
      { ...bolt, collections: [], reachable: false, slots: [], unsupported: [] },
      null,
      "0.6.6",
      "linux x86_64",
    );
    expect(fields.model).toBe("Logi Bolt receiver");
    expect(fields.report).toContain("HID collections: none");
    expect(fields.report).toContain("PeriHub reaches its paired devices: no");
    expect(fields.report).toContain("Slots: not asked");
    expect(fields.report).toContain("Paired device: none answered");
  });

  it("adds the DPI now and the DPI PeriHub sets, for a device that is open", () => {
    const live = { battery: null, dpi: 1600, saved_dpi: 1600, lighting: null };
    const report = receiverTest(bolt, bolt.unsupported[0], "0.6.7", "linux x86_64", live).report;
    expect(report).toContain("DPI now: 1600");
    expect(report).toContain("DPI set by PeriHub at every connection: 1600");
    expect(receiverTest(bolt, bolt.unsupported[0], "0.6.7", "linux x86_64").report).not.toContain("DPI now");
  });

  it("adds a keyboard's lighting, with every reply as it came, and stays short enough for the form", () => {
    const lighting = {
      brightness: { min: 0, max: 100, steps: 5, on_off: true },
      effects: {
        zones: [
          {
            index: 0,
            location: 1,
            effects: [
              { index: 0, id: 0, caps: 0, period_ms: 0 },
              { index: 1, id: 1, caps: 1, period_ms: 0 },
              { index: 2, id: 0x15, caps: 4, period_ms: 10000 },
            ],
          },
        ],
        startup: { enabled: true, colors: ["#ff0081", "#80aaff"] as [string, string] },
        shutdown: null,
        idle_s: 60,
        sleep_s: 300,
      },
      keys: [4, 5, 6, 7],
      layout: 1,
      raw: ["8071 0 (ff ff 00): ff ff 01 00 00 00 01", "8040 0: 00 64 05 04"],
      errors: ["8071 function 3 (00 00 40): HID++ error 0x02"],
    };
    const keyboard = { ...bolt.unsupported[0], name: "G915 X LS", kind: 0, dpi: null, lighting };
    const live = { battery: null, dpi: null, saved_dpi: null, lighting: { brightness: 50, on: true, control: 0 } };
    const report = receiverTest(bolt, keyboard, "0.8.5", "windows x86_64", live).report.split("\n");
    expect(report).toContain("Brightness: 0-100, 5 levels, can be turned off");
    expect(report).toContain(
      "Lighting zones: Keys (location 1): Off, Fixed colour, Colour cycle",
    );
    expect(report).toContain("Startup and shutdown: startup on (#ff0081 #80aaff), shutdown not read");
    expect(report).toContain("Lights dim after: 60 s, off after 300 s");
    expect(report).toContain("Per-key lighting: 4 keys (4-7)");
    expect(report).toContain("Layout (0x4540): 1");
    expect(report).toContain("Lighting replies: 8071 0 (ff ff 00): ff ff 01 00 00 00 01 | 8040 0: 00 64 05 04");
    expect(report).toContain("Lighting now: brightness 50, on, driven by the device (0)");
    expect(report).toContain("Could not read: 8071 function 3 (00 00 40): HID++ error 0x02");

    // A full-size keyboard: 2 zones of 16 effects, 110 keys and every reply. The form's address stays
    // well under the 8000 characters browsers and GitHub take.
    const zone = (index: number) => ({
      index,
      location: index + 1,
      effects: Array.from({ length: 16 }, (_, i) => ({ index: i, id: i, caps: 0x8005, period_ms: 5000 })),
    });
    const raw = Array.from({ length: 40 }, (_, i) => `8071 0 (00 ${i.toString(16).padStart(2, "0")} 00): 00 01 00 0a 80 05 13 88`);
    const big = { ...lighting, effects: { ...lighting.effects, zones: [zone(0), zone(1)] }, keys: Array.from({ length: 110 }, (_, i) => i + 1), raw };
    // The G915 X LS of issue #30: its 34 features, firmware and device info.
    const g915 = {
      ...keyboard,
      device_info: "05 xx xx xx xx 00 0e b3 8a 40 b5 c3 56 00 01 00",
      firmware: ["BL1 84.00.0012", " 00.00.0123", "MPK 26.00.0012"],
      features: [
        { id: 0x0001, kind: 0x0, version: 2 },
        { id: 0x0003, kind: 0x0, version: 6 },
        { id: 0x0005, kind: 0x0, version: 5 },
        { id: 0x1d4b, kind: 0x0, version: 0 },
        { id: 0x0020, kind: 0x0, version: 0 },
        { id: 0x0007, kind: 0x0, version: 0 },
        { id: 0x0011, kind: 0x0, version: 0 },
        { id: 0x1004, kind: 0x0, version: 5 },
        { id: 0x8071, kind: 0x0, version: 4 },
        { id: 0x8081, kind: 0x0, version: 0 },
        { id: 0x1b10, kind: 0x0, version: 0 },
        { id: 0x4523, kind: 0x0, version: 1 },
        { id: 0x4540, kind: 0x0, version: 1 },
        { id: 0x8040, kind: 0x0, version: 0 },
        { id: 0x8101, kind: 0x0, version: 0 },
        { id: 0x1b05, kind: 0x0, version: 1 },
        { id: 0x8051, kind: 0x0, version: 0 },
        { id: 0x00d0, kind: 0x0, version: 3 },
        { id: 0x1802, kind: 0x70, version: 0 },
        { id: 0x1803, kind: 0x70, version: 0 },
        { id: 0x1807, kind: 0x70, version: 3 },
        { id: 0x1817, kind: 0x70, version: 0 },
        { id: 0x1805, kind: 0x60, version: 0 },
        { id: 0x1830, kind: 0x70, version: 0 },
        { id: 0x1890, kind: 0x68, version: 9 },
        { id: 0x1891, kind: 0x68, version: 9 },
        { id: 0x18a1, kind: 0x70, version: 0 },
        { id: 0x1e00, kind: 0x40, version: 0 },
        { id: 0x1e02, kind: 0x60, version: 0 },
        { id: 0x1602, kind: 0x0, version: 0 },
        { id: 0x1eb0, kind: 0x70, version: 0 },
        { id: 0x1861, kind: 0x70, version: 1 },
        { id: 0x18b0, kind: 0x70, version: 1 },
        { id: 0x1801, kind: 0x70, version: 0 },
      ],
      lighting: big,
    };
    const fields = receiverTest(bolt, g915, "0.8.5", "windows x86_64", live);
    expect(deviceTestFormUrl(fields, "0.8.5", "windows x86_64").length).toBeLessThan(7000);
  });

  it("says when the system refused to open a Unifying receiver", () => {
    const unifying = { ...bolt, product_id: 0xc52b, name: "Unifying receiver", bolt: false, unifying: true };
    const fields = receiverTest({ ...unifying, denied: true, unsupported: [] }, null, "0.6.7", "linux x86_64");
    expect(fields.usbId).toBe("046d:c52b");
    expect(fields.report).toContain("PeriHub reaches its paired devices: no (the system refused to open it)");
  });
});
