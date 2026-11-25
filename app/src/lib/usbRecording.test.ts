import { describe, expect, it } from "vitest";
import type { ReportDevice } from "./api";
import { CAPABILITIES, MAX_STEP, PUT_BACK, recordingSteps, stepName } from "./usbRecording";
import { MAX_URL, RECORDING_HEADING, SHORTENED, deviceDetails, deviceRequestLink } from "./testReport";

const device: ReportDevice = {
  vendor_id: 0x1234,
  product_id: 0x00ab,
  manufacturer: "Maker",
  product: "Maker Pro Mouse",
  bus: "USB",
  collections: [{ interface: 1, usage_page: 0xff00, usage: 0x01 }],
  release: 0x0114,
  usb_interfaces: 2,
  descriptors: [{ interface: 1, hex: "0600ff0901a101".repeat(40), collections: null }],
  feature_reports: null,
};

describe("the steps of a recording", () => {
  it("follow the boxes in the form's order, then each line of something else, then putting back", () => {
    expect(recordingSteps(["lighting", "dpi"], "  Debounce time \n\n Lift-off distance")).toEqual([
      CAPABILITIES[0].step,
      CAPABILITIES[2].step,
      "Debounce time",
      "Lift-off distance",
      PUT_BACK,
    ]);
  });

  it("are none when nothing is ticked or written", () => {
    expect(recordingSteps([], "  ")).toEqual([]);
  });

  it("are named by their first words and the user's note, kept short", () => {
    expect(stepName(CAPABILITIES[0].step, " 800 to 1600 ")).toBe("DPI (800 to 1600)");
    expect(stepName(PUT_BACK, "")).toBe(PUT_BACK);
    expect(stepName("x", "y".repeat(500)).length).toBeLessThanOrEqual(MAX_STEP + 4);
    expect(recordingSteps([], "z".repeat(500))[0].length).toBe(MAX_STEP);
  });
});

describe("a recording in the device request", () => {
  const recording = "USB recording of 1234:00ab (usbmon, bus 1 address 7)\nStep 1: DPI (at 0.0 s)\n   +0.002 → SET_REPORT output id 10 if1: 10ff001a00005a";

  it("comes last in the details, under its heading", () => {
    const details = deviceDetails(device, "0.9.6", "linux x86_64", true, recording);
    expect(details.endsWith(RECORDING_HEADING + "\n" + recording)).toBe(true);
    expect(deviceDetails(device, "0.9.6", "linux x86_64")).not.toContain(RECORDING_HEADING);
  });

  it("goes whole into a link that has room for it", () => {
    const { url, shortened } = deviceRequestLink({ ...device, descriptors: [] }, "0.9.6", "linux x86_64", recording);
    expect(shortened).toBe(false);
    expect(new URL(url).searchParams.get("details")).toContain("SET_REPORT output id 10 if1: 10ff001a00005a");
  });

  it("is cut from its end when the link is too long, and says so", () => {
    const long = recording + "\n" + Array.from({ length: 400 }, (_, k) => `   +${k}.000 ← in ep82 if1: 11ff${k}`).join("\n");
    const { url, shortened } = deviceRequestLink(device, "0.9.6", "linux x86_64", long);
    expect(shortened).toBe(true);
    expect(url.length).toBeLessThanOrEqual(MAX_URL);
    const details = new URL(url).searchParams.get("details")!;
    expect(details).toContain(RECORDING_HEADING);
    expect(details.endsWith(SHORTENED)).toBe(true);
    // The details before the recording are all there: only the recording's end went.
    expect(details).toContain("USB ID: 1234:00ab");
  });

  it("is the first thing a long link drops: recording, then raw descriptors, then feature reports, then the end", () => {
    const rec = recording + "\n" + Array.from({ length: 400 }, (_, k) => `   +${k}.000 ← in ep82 if1: 11ff${k}`).join("\n");
    const details = (d: ReportDevice, r: string | null) => new URL(deviceRequestLink(d, "0.9.6", "linux x86_64", r).url).searchParams.get("details")!;
    const features = { interface: 1, outcome: "read" as const, reports: [{ id: 5, length: 3, hex: "050a0b" }] };
    const base = { ...device, feature_reports: [features] };
    // 1. Raw descriptors and feature reports fit: only the recording is cut.
    const first = details(base, rec);
    expect(first).toContain(RECORDING_HEADING);
    expect(first).toContain("0600ff0901a101".repeat(40));
    expect(first).toContain("id 05 (3 bytes): 050a0b");
    // 2. The raw descriptors are too long even without the recording: recording and raw go, features stay.
    const big = { ...base, descriptors: [{ ...device.descriptors[0], hex: "05".repeat(4200) }] };
    const second = details(big, rec);
    expect(second).not.toContain(RECORDING_HEADING);
    expect(second).not.toContain("05".repeat(4200));
    expect(second).toContain("id 05 (3 bytes): 050a0b");
    // 3. The feature reports too: they go as well, and the rest of the details stays.
    const huge = { ...big, feature_reports: [{ interface: 1, outcome: "read" as const, reports: Array.from({ length: 32 }, (_, i) => ({ id: i + 1, length: 200, hex: "ab".repeat(200) })) }] };
    const third = details(huge, rec);
    expect(third).not.toContain(RECORDING_HEADING);
    expect(third).toContain("Feature reports (read only):\n  left out of the link: use Copy report");
    expect(third).toContain("USB ID: 1234:00ab");
    // Copy report has all of it.
    const all = deviceDetails(huge, "0.9.6", "linux x86_64", true, rec);
    expect(all).toContain("05".repeat(4200));
    expect(all).toContain("ab".repeat(200));
    expect(all).toContain(RECORDING_HEADING);
  });
});
