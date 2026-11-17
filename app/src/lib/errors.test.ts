import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

import { errorText, rawErrorText } from "./api";

describe("errorText", () => {
  it("explains the causes people meet often and keeps the raw text in brackets", () => {
    const denied = errorText("HID error: hidapi error: Permission denied");
    expect(denied).toContain("not allowed to open this device");
    expect(denied).toContain("(HID error: hidapi error: Permission denied)");
    expect(errorText("the device did not respond in time")).toContain("did not answer in time");
    expect(errorText("the mouse is turned off or asleep")).toContain("Wake it");
    expect(errorText("device not connected")).toContain("not connected any more");
    expect(errorText("the mouse rejected the command: device busy (0x08)")).toContain("busy");
    expect(errorText("HID error: Device or resource busy")).toContain("Another program");
  });

  it("does not repeat a short message that says the same", () => {
    expect(errorText("Read-only mode is on")).toBe(
      "Read-only mode is on, so nothing was sent to the device. Turn it off first if you want to change it.",
    );
  });

  it("leaves an unknown error as it came", () => {
    expect(errorText("unexpected response from the device: 12 34")).toBe("unexpected response from the device: 12 34");
    expect(errorText(new Error("boom"))).toBe("boom");
    expect(errorText({ code: 3 })).toBe('{"code":3}');
    expect(rawErrorText("device not connected")).toBe("device not connected");
  });
});
