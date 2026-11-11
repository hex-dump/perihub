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
    const live = { battery: null, dpi: 1600, saved_dpi: 1600 };
    const report = receiverTest(bolt, bolt.unsupported[0], "0.6.7", "linux x86_64", live).report;
    expect(report).toContain("DPI now: 1600");
    expect(report).toContain("DPI set by PeriHub at every connection: 1600");
    expect(receiverTest(bolt, bolt.unsupported[0], "0.6.7", "linux x86_64").report).not.toContain("DPI now");
  });

  it("says when the system refused to open a Unifying receiver", () => {
    const unifying = { ...bolt, product_id: 0xc52b, name: "Unifying receiver", bolt: false, unifying: true };
    const fields = receiverTest({ ...unifying, denied: true, unsupported: [] }, null, "0.6.7", "linux x86_64");
    expect(fields.usbId).toBe("046d:c52b");
    expect(fields.report).toContain("PeriHub reaches its paired devices: no (the system refused to open it)");
  });
});
