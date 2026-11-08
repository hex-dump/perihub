import { describe, expect, it } from "vitest";
import type { CorsairInfo } from "./api";
import { corsairTest, deviceTestFormUrl } from "./testReport";

const info: CorsairInfo = {
  product_id: 0x1b4c,
  model: "IRONCLAW RGB WIRELESS",
  kind: "mouse",
  receiver: 0x1ba6,
  leds: 6,
  firmware: "5.6.7",
  mode: 1,
  polling_hz: 1000,
  dpi: 1800,
  read_errors: [],
  lighting: null,
};

describe("the device test form", () => {
  it("is the device_test form, filled in with what PeriHub read", () => {
    const fields = corsairTest(info, "0.6.1", "windows x86_64");
    const url = new URL(deviceTestFormUrl(fields, "0.6.1", "windows x86_64"));
    expect(url.origin + url.pathname).toBe("https://github.com/hex-dump/perihub/issues/new");
    expect(url.searchParams.get("template")).toBe("device_test.yml");
    expect(url.searchParams.get("model")).toBe("Corsair IRONCLAW RGB WIRELESS");
    expect(url.searchParams.get("usb-id")).toBe("1b1c:1b4c");
    expect(url.searchParams.get("connection")).toBe("wireless receiver");
    expect(url.searchParams.get("os")).toBe("Windows");
    const report = url.searchParams.get("report")!;
    expect(report).toContain("Firmware: 5.6.7");
    expect(report).toContain("wireless receiver 1b1c:1ba6");
    expect(report).toContain("DPI: 1800");
  });
});
