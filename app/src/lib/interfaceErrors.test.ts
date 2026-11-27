import { afterEach, describe, expect, it, vi } from "vitest";

const api = vi.hoisted(() => ({ logInterfaceError: vi.fn(async () => {}) }));
vi.mock("./api", () => ({ api }));

import { describeError, watchInterfaceErrors } from "./interfaceErrors";

describe("interface errors", () => {
  let stop = () => {};
  afterEach(() => {
    stop();
    document.body.innerHTML = "";
    api.logInterfaceError.mockClear();
  });

  it("logs an uncaught error and says it on screen", () => {
    stop = watchInterfaceErrors();
    const error = new TypeError("x is undefined");
    window.dispatchEvent(new ErrorEvent("error", { error, message: error.message }));
    expect(api.logInterfaceError).toHaveBeenCalledWith(expect.stringMatching(/^uncaught: TypeError: x is undefined/));
    expect(document.getElementById("interface-error")?.textContent).toContain("x is undefined");
  });

  it("leaves browser notices without an error alone", () => {
    stop = watchInterfaceErrors();
    window.dispatchEvent(new ErrorEvent("error", { message: "ResizeObserver loop completed with undelivered notifications." }));
    expect(api.logInterfaceError).not.toHaveBeenCalled();
    expect(document.getElementById("interface-error")).toBeNull();
  });

  it("describes errors on one line", () => {
    expect(describeError("device not connected")).toBe("device not connected");
    expect(describeError(new Error("boom"))).toMatch(/^Error: boom/);
    expect(describeError(new Error("boom"))).not.toContain("\n");
  });
});
