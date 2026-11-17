import { describe, expect, it, vi } from "vitest";

vi.mock("./api", () => ({}));

import type { CorsairInfo, MouseInfo, ReceiverReport } from "./api";
import { corsairTest, deviceTestFormUrl, receiverTest, testFormUrl, testReport } from "./testReport";

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

// Issues #32 and #33: PRO X 2 style mice report 0x2202 and 0x8061 instead of 0x2201 and 0x8060, and format 7.
describe("a mouse with the extended DPI and polling rate features", () => {
  const pro: MouseInfo = {
    ...wireless,
    model: "PRO X 2 DEX",
    name: "PRO X 2 DEX",
    verified: false,
    writable_format: false,
    link: { receiver_id: 0xc54d, slot: 1, wireless_id: 0x40b8 },
    firmware: [
      { kind: 0, name: "BL2", version: "38.01.0005" },
      { kind: 3, name: "MPM", version: "37.03.0007" },
    ],
    report_rates_ms: [],
    led_zones: [],
    // The feature list of the PRO X 2 DEX in issue #32, hidden features included.
    features: [0x0001, 0x0003, 0x0005, 0x1d4b, 0x0020, 0x1004, 0x2250, 0x2251, 0x2202, 0x8090, 0x80e0, 0x8061, 0x8100]
      .concat([0x8110, 0x1500, 0x1801, 0x1802, 0x1803, 0x1806, 0x1817, 0x1805, 0x1830, 0x1875, 0x1861, 0x1890])
      .concat([0x18a1, 0x1e00, 0x1e02, 0x1e22, 0x1602, 0x1eb0, 0x18b1])
      .map((id) => ({ id, kind: id >= 0x1800 && id < 0x1900 ? 0x70 : 0, version: 0 })),
    onboard: { ...wireless.onboard, profile_format: 7, profile_count: 5, button_count: 16, sector_size: 255 },
    extended: {
      dpi_y: { min: 100, max: 44000, step: 50, list: [] },
      dpi_now: { x: 1200, default_x: 800, y: 1200, default_y: 800, lod: 2 },
      rates_wired_hz: [125, 250, 500, 1000],
      rates_wireless_hz: [125, 250, 500, 1000, 2000, 4000, 8000],
      rate_now_hz: 2000,
      raw: ["rates cable: 00 0f", "rates receiver: 00 7f", "rate in use: index 4"],
      profiles: [1, 2, 3, 4, 5].map(
        (n) =>
          `Profile ${n} "Profile ${n}": DPI 400 800 1600 3200 6400 (level 2 default, 1 shift; lift-off 2 2 2 2 2), polling 2000 Hz wireless / 1000 Hz cable, BHOP 0, timeouts 120 / 900`,
      ),
      errors: [],
    },
    dpi: { min: 100, max: 44000, step: 50, list: [] },
  };

  it("shows the DPI, the polling rates and each profile, for the tester to compare with G HUB", () => {
    const lines = testReport(pro, "0.8.5", "windows x86_64").split("\n");
    expect(lines).toContain("DPI: 100-44000, step 50");
    expect(lines).toContain("DPI Y axis: 100-44000, step 50");
    expect(lines).toContain("DPI now: X 1200 (default 800), Y 1200 (default 800), lift-off distance 2");
    expect(lines).toContain("Polling (Hz): cable 125 250 500 1000; receiver 125 250 500 1000 2000 4000 8000; in use 2000");
    expect(lines).toContain("Polling answers: rates cable: 00 0f; rates receiver: 00 7f; rate in use: index 4");
    expect(lines.filter((l) => /^Profile \d/.test(l))).toHaveLength(5);
    expect(lines).toContain("Memory: model 1, profile format 7, macro format 1, 16 sectors of 255 bytes, 5 factory profiles");
    expect(lines.some((l) => l.startsWith("Polling (ms)"))).toBe(false);
    expect(lines).toContain("Support: read-only (memory format PeriHub cannot write)");
  });

  it("says what could not be read, and leaves the lines out of a mouse that has none", () => {
    const report = testReport({ ...pro, extended: { ...pro.extended!, errors: ["DPI now: HID++ error 2"] } }, "0.8.5", "linux x86_64");
    expect(report).toContain("Could not read: DPI now: HID++ error 2");
    expect(testReport({ ...pro, extended: null, report_rates_ms: [1, 2] }, "0.8.5", "linux x86_64")).toContain("Polling (ms): 1, 2");
    expect(testReport(cable, "0.8.5", "linux x86_64")).not.toContain("Polling (Hz)");
  });

  // GitHub refuses a very long link: the whole form link, with every character of the report
  // escaped, has to stay under about 8 kB.
  it("keeps the test form link short enough", () => {
    const url = testFormUrl(pro, "0.8.5", "windows x86_64");
    expect(url.length).toBeLessThan(5000);
    expect(new URL(url).searchParams.get("report")).toBe(testReport(pro, "0.8.5", "windows x86_64"));
  });
});

describe("testFormUrl", () => {
  const fields = (url: string) => new URL(url).searchParams;

  it("opens the test report form with the fields filled in", () => {
    const url = testFormUrl(wireless, "0.5.2", "windows x86_64");
    expect(url.startsWith("https://github.com/hex-dump/perihub/issues/new?template=01_test_report.yml&")).toBe(true);
    const query = fields(url);
    // GitHub wants a title before the report can be sent.
    expect(query.get("title")).toBe("G502 X Lightspeed test report");
    expect(query.get("device-type")).toBe("Mouse");
    expect(query.get("brand")).toBe("Logitech");
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

  it("names the extended DPI and polling rate features of a device that PeriHub does not support yet", () => {
    const device = { ...bolt.unsupported[0], dpi: null, dpi_extended: { min: 100, max: 25600, step: 50, list: [] }, report_rates_hz: [125, 8000] };
    const report = receiverTest(bolt, device, "0.8.5", "linux x86_64").report.split("\n");
    expect(report).toContain("DPI: 100-25600, step 50 (0x2202)");
    expect(report).toContain("Polling (Hz, 0x8061): 125 8000");
    expect(report.indexOf("Polling (Hz, 0x8061): 125 8000")).toBe(report.indexOf("Onboard memory: none") - 1);
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

// docs/replay.md: HID++ and Corsair test reports end with the transcript of the reading, as the
// reports of the other brands do, when the core gives one.
describe("the transcript at the end of a test report", () => {
  const transcript = '{\n  "format": "perihub-transcript",\n  "version": 1,\n  "events": [\n    {"op":"write","data":"11 01 00 1b"}\n  ]\n}\n';
  const heading = "Transcript (every request and answer, for a replay test):";

  it("ends a mouse report, and stays out when an older core gives none", () => {
    const lines = testReport({ ...cable, transcript }, "0.9.3", "linux x86_64").split("\n");
    expect(lines.slice(-8, -6)).toEqual([heading, "{"]);
    expect(lines.at(-1)).toBe("}");
    expect(testReport(cable, "0.9.3", "linux x86_64")).not.toContain(heading);
  });

  it("ends the report of a device behind a receiver and of a Corsair device", () => {
    const bolt: ReceiverReport = {
      product_id: 0xc548,
      name: "Logi Bolt receiver",
      collections: [],
      bolt: true,
      unifying: false,
      reachable: true,
      denied: false,
      open_slots: [1],
      slots: [],
      unsupported: [],
    };
    const device = {
      slot: 1,
      name: "MX Master 3S",
      kind: 3,
      protocol: "4.5",
      wireless_id: null,
      device_info: null,
      firmware: [],
      features: [],
      dpi: null,
      report_rates_ms: [],
      onboard: null,
      read_errors: [],
      transcript,
    };
    expect(receiverTest(bolt, device, "0.9.3", "linux x86_64").report).toContain(`${heading}\n{\n  "format"`);
    expect(receiverTest(bolt, { ...device, transcript: undefined }, "0.9.3", "linux x86_64").report).not.toContain(heading);
    const katar: CorsairInfo = {
      product_id: 0x1b93,
      model: "KATAR PRO",
      kind: "mouse",
      receiver: null,
      leds: 1,
      firmware: "5.2.65",
      mode: 1,
      polling_hz: 1000,
      dpi: 800,
      read_errors: [],
      lighting: null,
      transcript,
    };
    const report = corsairTest(katar, "0.9.3", "linux x86_64").report;
    expect(report.endsWith(`Experimental lighting: off\n${heading}\n${transcript.trimEnd()}`)).toBe(true);
  });
});
