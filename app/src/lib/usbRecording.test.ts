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
};

describe("the steps of a recording", () => {
  it("follow the boxes in the form's order, then each line of something else, then putting back", () => {
    expect(recordingSteps(["lighting", "dpi"], "  Sidetone \n\n Lift-off distance")).toEqual([
      CAPABILITIES[0].step,
      CAPABILITIES[2].step,
      "Sidetone",
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
});
