// tools/triage.py decides from these lines of the test reports whether a report confirms a device
// (tools/test_triage.py has the reports it is tested with): if one changes here, change it there.
import { describe, expect, it, vi } from "vitest";

vi.mock("./api", () => ({}));

import type { CorsairInfo, MouseInfo, PeripheralInfo } from "./api";
import { corsairTest, peripheralTest, reportVersion, testReport } from "./testReport";
import { checklistText } from "./reportWizard";
import { writeLogLines } from "./readOnly.svelte";

const peripheral = {
  key: "0b05:195c", brand: "ASUS", model: "ROG Keris", kind: "mouse", source: "libratbag (MIT)", tested: false, profiles: 3,
  firmware: "1.02.03", battery: null, active_profile: 1, active_dpi: 2, readable: true,
  settings: { dpi_levels: [800], polling_hz: 1000, options: {}, leds: [] }, limits: { options: [], leds: [] },
  buttons: [], details: [], blind: [], editable: true, problem: null, raw: ["12 04 00 00 → 12 04 00 00 03 00"],
} as unknown as PeripheralInfo;
const corsair = {
  product_id: 0x1b9e, model: "M65 RGB ULTRA", kind: "mouse", tested: false, receiver: null, leds: 3, firmware: "3.4.5", mode: 1,
  polling_hz: 1000, dpi: 1600, read_errors: [], lighting: null,
} as unknown as CorsairInfo;
const mouse = {
  product_id: 0xc08f, model: "G403 HERO", name: "G403 HERO Gaming Mouse", verified: false, experimental: true,
  receiver: false, link: null, host_mode: true, firmware: [{ kind: 0, name: "U1", version: "88.5.6" }],
  dpi: { min: 100, max: 25600, step: 50, list: [] }, report_rates_ms: [1], led_zones: [], features: [{ id: 1, kind: 0, version: 1 }],
  onboard: {
    memory_model: 1, profile_format: 2, macro_format: 1, profile_count: 5, rom_profile_count: 5, button_count: 11,
    sector_count: 16, sector_size: 255, has_gshift: true, has_dpi_shift: true,
  },
  profile_list: "own",
} as unknown as MouseInfo;

const lines = (report: string) => report.split("\n");

describe("a test build in the first line", () => {
  it("adds the branch and commit after the version, before 'on'", () => {
    expect(reportVersion("0.9.5")).toBe("0.9.5");
    expect(reportVersion("0.9.5", null)).toBe("0.9.5");
    const version = reportVersion("0.9.5", "test/viper-v3-pro-wireless@02fd962");
    expect(version).toBe("0.9.5 (test build test/viper-v3-pro-wireless@02fd962)");
    expect(lines(peripheralTest(peripheral, version, "windows x86_64").report)[0]).toBe(
      "PeriHub 0.9.5 (test build test/viper-v3-pro-wireless@02fd962) on windows x86_64",
    );
    expect(lines(corsairTest(corsair, version, "linux x86_64").report)[0]).toBe(
      "PeriHub 0.9.5 (test build test/viper-v3-pro-wireless@02fd962) on linux x86_64",
    );
    expect(lines(testReport(mouse, version, "linux x86_64"))[0]).toBe(
      "PeriHub 0.9.5 (test build test/viper-v3-pro-wireless@02fd962) on linux x86_64",
    );
  });
});

describe("the lines the triage workflow reads", () => {
  it("a device of another brand: its Device line and how its changes stand", () => {
    const report = peripheralTest(peripheral, "0.9.0", "linux x86_64").report;
    expect(lines(report)[0]).toBe("PeriHub 0.9.0 on linux x86_64");
    expect(lines(report)[1]).toBe("Device: ASUS ROG Keris (mouse), USB ID 0b05:195c, protocol from libratbag (MIT)");
    expect(lines(report)).toContain("Changes: available (experimental)");
    expect(lines(peripheralTest({ ...peripheral, editable: false }, "0.9.0", "linux").report)).toContain("Changes: off (read-only)");
    expect(peripheralTest({ ...peripheral, problem: "unexpected answer" }, "0.9.0", "linux").report).toContain(
      "Changes: off (unexpected answer)",
    );
    expect(peripheralTest({ ...peripheral, readable: false }, "0.9.0", "linux").report).toContain(
      "Settings: this device cannot be read back; PeriHub sends them one at a time",
    );
  });

  it("a Corsair device: the Device line, what could not be read, and the lighting", () => {
    const report = corsairTest(corsair, "0.9.0", "linux x86_64").report;
    expect(lines(report)[1]).toBe("Device: Corsair M65 RGB ULTRA (mouse), USB ID 1b1c:1b9e");
    expect(lines(report)).toContain("Could not read: nothing");
    expect(lines(report)).toContain("Experimental lighting: off");
    expect(corsairTest({ ...corsair, read_errors: ["polling: no answer"] }, "0.9.0", "linux").report).toContain(
      "Could not read: polling: no answer",
    );
    expect(corsairTest({ ...corsair, lighting: "#ff0000" }, "0.9.0", "linux").report).toContain("Experimental lighting: on");
  });

  it("a mouse: the support, the memory, the profile list and the connection", () => {
    const report = lines(testReport(mouse, "0.9.0", "linux x86_64"));
    expect(report).toContain("USB ID: 046d:c08f");
    expect(report).toContain("Connection: USB cable");
    expect(report).toContain("Support: experimental (memory format not tested yet)");
    expect(report).toContain("Memory: model 1, profile format 2, macro format 1, 16 sectors of 255 bytes, 5 factory profiles");
    expect(report).toContain("Profile list: saved in the mouse");
    expect(report).toContain("Features: 0001 v1");
  });

  it("what the wizard adds: the header of the writes, and the checklist the tester answered", () => {
    expect(writeLogLines([], false)[0]).toBe("Writes this session: 0 sent, 0 refused");
    const items = [{ id: "name", label: "The name and USB ID PeriHub shows are right" }, { id: "dpi", label: "The DPI PeriHub shows matches the mouse" }];
    expect(checklistText(items, { name: "worked", dpi: "failed" })).toBe(
      "What I tried:\n  worked: The name and USB ID PeriHub shows are right\n  did not work: The DPI PeriHub shows matches the mouse",
    );
  });
});
