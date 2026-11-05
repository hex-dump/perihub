import { describe, expect, it, vi } from "vitest";

vi.mock("./api", () => ({}));

import type { MouseInfo } from "./api";
import { testFormUrl, testReport } from "./testReport";

const cable: MouseInfo = {
  product_id: 0xc08b,
  model: "G502 HERO",
  name: "G502 HERO Gaming Mouse",
  verified: true,
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
};

const wireless: MouseInfo = {
  ...cable,
  product_id: 0x4093,
  model: "G502 X Lightspeed",
  verified: false,
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
    expect(report).toContain("DPI: 100-25600, step 50");
    expect(report).toContain("LEDs: zone 0 at 1, effects 0 1 3");
  });

  it("gives the receiver, the slot and the wireless id of a mouse behind a receiver", () => {
    const report = testReport(wireless, "0.5.2", "windows x86_64");
    expect(report).toContain("USB ID: 046d:c547");
    expect(report).toContain("Connection: LIGHTSPEED receiver 046d:c547, slot 1, wireless id 4093");
    expect(report).toContain("Support: read-only");
  });

  it("lists fixed DPI values and says when a mouse reports no LEDs or polling rates", () => {
    const report = testReport(
      { ...cable, dpi: { min: 400, max: 1600, step: 0, list: [400, 800, 1600] }, led_zones: [], report_rates_ms: [] },
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
