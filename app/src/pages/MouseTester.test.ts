import { cleanup, render, screen } from "@testing-library/svelte";
import { afterEach, describe, expect, it, vi } from "vitest";

const detect = vi.hoisted(() => ({ source: "frames" }));
vi.mock("../lib/mouseTest", async (original) => ({
  ...(await original<typeof import("../lib/mouseTest")>()),
  pollingSource: () => detect.source,
}));

import MouseTester from "./MouseTester.svelte";

afterEach(cleanup);

/** A press at a chosen time: events made in a test have no usable clock. */
function press(el: HTMLElement, time: number, button = 0) {
  const e = new MouseEvent("mousedown", { bubbles: true, cancelable: true, button });
  Object.defineProperty(e, "timeStamp", { value: time });
  el.dispatchEvent(e);
}

describe("MouseTester", () => {
  it("shows no polling rate where the window cannot measure it", () => {
    detect.source = "frames";
    render(MouseTester);
    expect(screen.getByText(/cannot measure the polling rate/)).toBeTruthy();
    expect(screen.queryByText("Average")).toBeNull();
  });

  it("shows the rate boxes where the window reports each movement", () => {
    detect.source = "raw";
    render(MouseTester);
    expect(screen.queryByText(/cannot measure the polling rate/)).toBeNull();
    expect(screen.getByText("Average")).toBeTruthy();
  });

  it("says nothing is sent to the mouse", () => {
    render(MouseTester);
    expect(screen.getByText(/Nothing is sent to the mouse/)).toBeTruthy();
  });

  it("flags a press that follows the last one too fast", async () => {
    render(MouseTester);
    const pad = screen.getByLabelText("Click here");
    press(pad, 1000);
    expect(await screen.findByText(/No unwanted double clicks/)).toBeTruthy();
    press(pad, 1020);
    expect(await screen.findByText(/Left button: 1/)).toBeTruthy();
  });

  it("does not flag slow clicks", async () => {
    render(MouseTester);
    const pad = screen.getByLabelText("Click here");
    press(pad, 1000);
    press(pad, 1200);
    expect(await screen.findByText(/No unwanted double clicks/)).toBeTruthy();
  });
});
