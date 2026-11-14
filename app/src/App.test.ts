import { cleanup, render, screen } from "@testing-library/svelte";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// Every call the test does not set up stays pending, so the pages just wait.
const calls = vi.hoisted(() => ({}) as Record<string, ReturnType<typeof vi.fn>>);
const api = vi.hoisted(
  () => new Proxy({}, { get: (_, name: string) => calls[name] ?? (() => new Promise(() => {})) }),
);
vi.mock("./lib/api", async (original) => ({ ...(await original<typeof import("./lib/api")>()), api }));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: async () => "0.8.0" }));
// jsdom has no matchMedia, which svelte/motion reads when the keyboard page is imported.
vi.hoisted(() => {
  window.matchMedia ??= ((query: string) =>
    ({ matches: false, media: query, addEventListener() {}, removeEventListener() {} }) as unknown as MediaQueryList);
});

import type { DeviceStatus, ReportDevice } from "./lib/api";
import { deviceRequestUrl } from "./lib/testReport";
import App from "./App.svelte";

const empty = (): DeviceStatus => ({
  keyboard: null,
  mouse: null,
  keyboard_error: null,
  mouse_error: null,
  mouse_asleep: null,
  unsupported_keyboard: null,
  conflicts: [],
  corsair: [],
  g300: null,
  g300_error: null,
  corsair_error: null,
  peripherals: [],
  peripheral_error: null,
  qmk: [],
  receivers: [],
  logitech_programs: [],
  sandboxed: false,
});

const ite: ReportDevice = {
  vendor_id: 0x048d,
  product_id: 0x5702,
  manufacturer: "ITE Tech. Inc.",
  product: "ITE Device",
  bus: "USB",
  collections: [
    { interface: 0, usage_page: 0xff89, usage: 0x10 },
    { interface: 0, usage_page: 0xff89, usage: 0xcc },
  ],
};

const wait = (ms = 0) => vi.advanceTimersByTimeAsync(ms);
const nav = () => document.querySelector("nav") as HTMLElement;

beforeEach(() => {
  vi.useFakeTimers();
  for (const name of Object.keys(calls)) delete calls[name];
  calls.devicesForReport = vi.fn(async () => [ite]);
  calls.appSettings = vi.fn(async () => ({ system: "linux x86_64" }));
  calls.openTestForm = vi.fn(async () => {});
  location.hash = "";
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("Sidebar", () => {
  it("says once that an update was installed, until the message is closed", async () => {
    calls.status = vi.fn(async () => empty());
    calls.updateJustInstalled = vi.fn(async () => "0.8.2");
    render(App);
    await wait();
    expect(screen.getByText("PeriHub was updated to 0.8.2.")).toBeTruthy();
    (screen.getByRole("button", { name: "Close" }) as HTMLButtonElement).click();
    await wait();
    expect(screen.queryByText("PeriHub was updated to 0.8.2.")).toBeNull();
  });

  it("says nothing about updates after a normal start", async () => {
    calls.status = vi.fn(async () => empty());
    calls.updateJustInstalled = vi.fn(async () => null);
    render(App);
    await wait();
    expect(document.body.textContent).not.toContain("was updated to");
  });

  it("lists nothing until PeriHub has looked for devices", async () => {
    calls.status = vi.fn(() => new Promise(() => {}));
    render(App);
    await wait();
    expect(nav().textContent).not.toContain("Send a report");
    expect(nav().textContent).not.toContain("Apex 3 TKL");
  });

  it("with no device, offers only the report, and opens it", async () => {
    calls.status = vi.fn(async () => empty());
    render(App);
    await wait();
    expect(nav().textContent).toContain("Send a report");
    expect(nav().textContent).not.toContain("Apex 3 TKL");
    expect(nav().textContent).not.toContain("Logitech mouse");
    expect(screen.getByRole("heading", { name: "Send a report for your device" })).toBeTruthy();
    expect(await screen.findByText("ITE Device")).toBeTruthy();
  });

  it("with a device, lists it alone and opens it", async () => {
    calls.status = vi.fn(async () => ({ ...empty(), mouse_asleep: "G502 X PLUS" }));
    render(App);
    await wait();
    expect(nav().textContent).toContain("G502 X PLUS");
    expect(nav().textContent).toContain("Asleep");
    expect(nav().textContent).not.toContain("Apex 3 TKL");
    expect(nav().textContent).not.toContain("Send a report");
    expect(location.hash).toBe("");
  });

  it("lists a device that is there but could not be opened, with why on its page", async () => {
    calls.status = vi.fn(async () => ({ ...empty(), keyboard_error: "permission denied" }));
    render(App);
    await wait();
    expect(nav().textContent).toContain("Apex 3 TKL");
    expect(nav().textContent).toContain("Not opened");
  });

  // Issue #4: a POWERPLAY receiver whose mouse never answered left only "No device found".
  const receiver = {
    product_id: 0xc53a,
    name: "LIGHTSPEED receiver",
    collections: ["interface 2, usage page ff00, usage 0001", "interface 2, usage page ff00, usage 0002"],
    bolt: false,
    unifying: false,
    reachable: true,
    denied: false,
    slots: ["slot 1: no answer", "slot 2: no answer"],
    open_slots: [],
    unsupported: [],
  };

  it("lists a receiver nothing answered through, under the mouse", async () => {
    calls.status = vi.fn(async () => ({ ...empty(), receivers: [receiver] }));
    render(App);
    await wait();
    expect(nav().textContent).toContain("Logitech mouse");
    expect(nav().textContent).not.toContain("Send a report");
  });

  it("does not add a mouse for a Unifying receiver Linux keeps closed", async () => {
    const unifying = { ...receiver, product_id: 0xc52b, name: "Unifying receiver", unifying: true, reachable: false, denied: true };
    calls.status = vi.fn(async () => ({ ...empty(), receivers: [unifying] }));
    render(App);
    await wait();
    expect(nav().textContent).not.toContain("Logitech mouse");
    expect(nav().textContent).toContain("Send a report");
  });

  it("does not add a mouse for a receiver whose devices have pages of their own", async () => {
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
    };
    const bolt = { ...receiver, product_id: 0xc548, name: "Logi Bolt receiver", bolt: true, unsupported: [device] };
    calls.status = vi.fn(async () => ({ ...empty(), receivers: [bolt] }));
    render(App);
    await wait();
    expect(nav().textContent).toContain("Logitech MX Master 3S");
    expect(nav().textContent).not.toContain("Logitech mouse");
  });
});

describe("Device request", () => {
  it("fills in the form with the name, USB id and collections, and nothing else", () => {
    const url = new URL(deviceRequestUrl(ite, "0.8.0", "Windows 11 x86_64"));
    expect(url.searchParams.get("template")).toBe("device_request.yml");
    expect(url.searchParams.get("model")).toBe("ITE Device");
    expect(url.searchParams.get("usb-id")).toBe("048d:5702");
    expect(url.searchParams.get("details")).toBe(
      [
        "PeriHub 0.8.0 on Windows 11 x86_64",
        "Device: ITE Device",
        "USB ID: 048d:5702 (USB)",
        "HID collections:",
        "  interface 0, usage page ff89, usage 0010",
        "  interface 0, usage page ff89, usage 00cc",
      ].join("\n"),
    );
    expect([...url.searchParams.keys()]).toEqual(["template", "title", "model", "usb-id", "details"]);
  });

  it("keeps a name that already starts with its maker", () => {
    const device = { ...ite, manufacturer: "Logitech", product: "Logitech G Pro" };
    expect(new URL(deviceRequestUrl(device, "0.8.0", "linux")).searchParams.get("model")).toBe("Logitech G Pro");
  });
});
