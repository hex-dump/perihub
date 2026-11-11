import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const api = vi.hoisted(() => ({
  qmkSetEnabled: vi.fn(),
  qmkAccess: vi.fn(),
  grantQmkAccess: vi.fn(),
  revokeQmkAccess: vi.fn(),
}));

vi.mock("../lib/api", async (original) => ({
  ...(await original<typeof import("../lib/api")>()),
  api,
}));

import type { QmkKeyboard } from "../lib/api";
import QmkKeyboards from "./QmkKeyboards.svelte";

const vial: QmkKeyboard = {
  key: "4653:0001",
  vendor_id: 0x4653,
  product_id: 0x0001,
  brand: "foostan",
  name: "Corne",
  vial: true,
  enabled: true,
  open: true,
  denied: false,
};

const via: QmkKeyboard = {
  key: "3434:0311",
  vendor_id: 0x3434,
  product_id: 0x0311,
  brand: "Keychron",
  name: "Q1 Pro",
  vial: false,
  enabled: false,
  open: false,
  denied: false,
};

const flush = () => new Promise((r) => setTimeout(r, 0));

beforeEach(() => {
  vi.clearAllMocks();
  api.qmkAccess.mockResolvedValue({
    allowed: [],
    supported: true,
    manual_install: [],
    manual_remove: "sudo rm",
  });
  api.qmkSetEnabled.mockResolvedValue(undefined);
  api.grantQmkAccess.mockResolvedValue(undefined);
  api.revokeQmkAccess.mockResolvedValue(undefined);
});

afterEach(cleanup);

describe("QMK keyboards", () => {
  it("opens the page of a Vial keyboard and turns nothing on by itself", async () => {
    const onopen = vi.fn();
    render(QmkKeyboards, { props: { keyboards: [vial, via], onopen } });
    await flush();
    expect(screen.getByText("foostan Corne")).toBeTruthy();
    await fireEvent.click(
      screen.getByRole("button", { name: "Open its page" }),
    );
    expect(onopen).toHaveBeenCalledWith("4653:0001");
    expect(api.qmkSetEnabled).not.toHaveBeenCalled();
    expect(api.grantQmkAccess).not.toHaveBeenCalled();
  });

  it("reads a possible VIA keyboard only after the user says it works with VIA", async () => {
    render(QmkKeyboards, { props: { keyboards: [via], onopen: () => {} } });
    await flush();
    await fireEvent.click(
      screen.getByRole("button", { name: "Read it as a VIA keyboard…" }),
    );
    const read = screen.getByRole("button", {
      name: "Read it",
    }) as HTMLButtonElement;
    expect(read.disabled).toBe(true);
    await fireEvent.click(screen.getByRole("checkbox"));
    await fireEvent.click(read);
    await flush();
    expect(api.qmkSetEnabled).toHaveBeenCalledWith("3434:0311", true);
  });

  it("on Linux, offers the rule for a keyboard the system refused, and nothing else", async () => {
    render(QmkKeyboards, {
      props: {
        keyboards: [{ ...vial, open: false, denied: true }],
        onopen: () => {},
      },
    });
    await flush();
    await fireEvent.click(
      screen.getByRole("button", { name: "Allow PeriHub to open it" }),
    );
    await flush();
    expect(api.grantQmkAccess).toHaveBeenCalledWith("4653:0001");
    expect(document.body.textContent).toContain("Permission given");
  });

  it("shows the commands for a terminal when the password window fails", async () => {
    api.qmkAccess.mockResolvedValue({
      allowed: [],
      supported: true,
      manual_install: [
        ["4653:0001", "sudo tee /etc/udev/rules.d/73-perihub-qmk.rules"],
      ],
      manual_remove: "sudo rm",
    });
    api.grantQmkAccess.mockRejectedValue("not authorized");
    render(QmkKeyboards, {
      props: {
        keyboards: [{ ...vial, open: false, denied: true }],
        onopen: () => {},
      },
    });
    await flush();
    await fireEvent.click(
      screen.getByRole("button", { name: "Allow PeriHub to open it" }),
    );
    await flush();
    expect(document.body.textContent).toContain("not authorized");
    expect(document.body.textContent).toContain("73-perihub-qmk.rules");
  });

  it("removes the rule of an allowed keyboard", async () => {
    api.qmkAccess.mockResolvedValue({
      allowed: ["4653:0001"],
      supported: true,
      manual_install: [],
      manual_remove: "sudo rm",
    });
    render(QmkKeyboards, { props: { keyboards: [vial], onopen: () => {} } });
    await flush();
    await fireEvent.click(
      screen.getByRole("button", { name: "Remove permission" }),
    );
    await flush();
    expect(api.revokeQmkAccess).toHaveBeenCalledWith("4653:0001");
  });
});
