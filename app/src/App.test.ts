import { cleanup, render, screen } from "@testing-library/svelte";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// Every call the test does not set up stays pending, so the pages just wait.
const calls = vi.hoisted(() => ({}) as Record<string, ReturnType<typeof vi.fn>>);
const api = vi.hoisted(
  () => new Proxy({}, { get: (_, name: string) => calls[name] ?? (() => new Promise(() => {})) }),
);
vi.mock("./lib/api", async (original) => ({ ...(await original<typeof import("./lib/api")>()), api }));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: async () => "0.8.0" }));
vi.mock("virtual:perihub-release-notes", () => ({
  default: {
    version: "0.8.0",
    date: "2026-11-12",
    summary: "A new look.",
    sections: [{ name: "Fixed", titles: ["Something fixed"] }],
  },
}));
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
  scanned: true,
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
  release: 0x0100,
  usb_interfaces: 1,
  descriptors: [{ interface: 0, hex: null, collections: null }],
  feature_reports: null,
};

const wait = (ms = 0) => vi.advanceTimersByTimeAsync(ms);
const nav = () => document.querySelector("nav") as HTMLElement;

beforeEach(() => {
  localStorage.clear();
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

describe("What's new", () => {
  it("opens once after a version change, and replaces the updated notice", async () => {
    localStorage.setItem("perihub.lastSeenVersion", "0.7.0");
    calls.status = vi.fn(async () => empty());
    calls.updateJustInstalled = vi.fn(async () => "0.8.0");
    render(App);
    await wait();
    expect(screen.getByRole("dialog", { name: "What's new in PeriHub 0.8.0" })).toBeTruthy();
    expect(screen.getByText("Something fixed")).toBeTruthy();
    expect(screen.queryByText("PeriHub was updated to 0.8.0.")).toBeNull();
    (screen.getByRole("button", { name: "Close" }) as HTMLButtonElement).click();
    await wait();
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.queryByText("PeriHub was updated to 0.8.0.")).toBeNull();
    cleanup();
    render(App);
    await wait();
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("is not shown on a fresh install", async () => {
    calls.status = vi.fn(async () => empty());
    render(App);
    await wait();
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("closes with Esc", async () => {
    localStorage.setItem("perihub.lastSeenVersion", "0.7.0");
    calls.status = vi.fn(async () => empty());
    render(App);
    await wait();
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    await wait();
    expect(screen.queryByRole("dialog")).toBeNull();
  });
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

  // The backend answers get_status at once, with empty lists, before its first pass has looked at devices.
  it("offers no report for the empty status sent before the first scan, then lists the device", async () => {
    const early = { ...empty(), scanned: false };
    let send: (s: DeviceStatus) => void = () => {};
    calls.status = vi.fn(async () => early);
    calls.onStatus = vi.fn(async (handler: (s: DeviceStatus) => void) => {
      send = handler;
      return () => {};
    });
    location.hash = "#report";
    render(App);
    await wait();
    expect(nav().textContent).not.toContain("Send a report");
    expect(screen.queryByRole("heading", { name: "Send a report for your device" })).toBeNull();
    expect(document.body.textContent).not.toContain("found no mouse or keyboard");
    send({ ...empty(), mouse_asleep: "G502 X PLUS" });
    await wait();
    expect(nav().textContent).toContain("G502 X PLUS");
    location.hash = "";
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

  it("shows each device's state with an icon and a word, never the connected look for experimental support", async () => {
    calls.status = vi.fn(async () => ({
      ...empty(),
      keyboard: { name: "SteelSeries Apex 3 TKL", firmware: "3.0.4" },
      mouse_asleep: "G502 X PLUS",
      corsair: [
        {
          product_id: 0x1b7a,
          model: "M65 RGB ULTRA",
          kind: "mouse" as const,
          receiver: null,
          leds: 3,
          firmware: null,
          mode: null,
          polling_hz: null,
          dpi: null,
          read_errors: [],
          lighting: null,
        },
      ],
    }));
    render(App);
    await wait();
    const states = [...nav().querySelectorAll(".status")].map((s) => [s.getAttribute("data-kind"), s.textContent?.trim()]);
    expect(states).toEqual([
      ["ok", "Connected"],
      ["asleep", "Asleep"],
      ["experimental", "Experimental"],
    ]);
    for (const s of nav().querySelectorAll(".status")) expect(s.querySelector("svg path")).toBeTruthy();
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
    expect(url.searchParams.get("details")).toContain(
      [
        "PeriHub 0.8.0 on Windows 11 x86_64",
        "Device: ITE Device",
        "Kind: Other",
        "USB ID: 048d:5702 (USB)",
        "Release number (bcdDevice): 0x0100",
        "USB interfaces: 1",
        "HID collections:",
        "  interface 0, usage page ff89, usage 0010",
        "  interface 0, usage page ff89, usage 00cc",
        "Report descriptors (lengths in bytes, with the id byte):",
        "  interface 0: not read",
      ].join("\n"),
    );
    expect([...url.searchParams.keys()]).toEqual(["template", "title", "model", "brand", "usb-id", "details"]);
  });

  it("keeps a name that already starts with its maker", () => {
    const device = { ...ite, manufacturer: "Logitech", product: "Logitech G Pro" };
    expect(new URL(deviceRequestUrl(device, "0.8.0", "linux")).searchParams.get("model")).toBe("Logitech G Pro");
  });
});

describe("Test report prompt", () => {
  const mouse = (verified: boolean) =>
    ({ product_id: 0xc08d, model: "G403", name: "G403", verified, experimental: false, receiver: false, link: null,
      firmware: [], onboard: {}, led_zones: [], features: [], report_rates_ms: [], dpi: { list: [800], min: 0, max: 0, step: 0 },
      profile_list: "own" }) as unknown as DeviceStatus["mouse"];
  // The prompt sits above any page: open one that needs no mouse data.
  beforeEach(() => (location.hash = "#profiles"));
  const prompt = () => screen.queryByText(/isn't confirmed yet/);

  it("appears once for an unverified mouse, and not again after it is dismissed and the app restarts", async () => {
    calls.status = vi.fn(async () => ({ ...empty(), mouse: mouse(false), scanned: true }));
    const first = render(App);
    await wait();
    expect(prompt()?.textContent).toContain("Your Logitech G403 isn't confirmed yet. Sending a test report takes a couple of minutes");
    (screen.getByRole("button", { name: "Dismiss" }) as HTMLButtonElement).click();
    await wait();
    expect(prompt()).toBeNull();
    first.unmount();
    render(App);
    await wait();
    expect(prompt()).toBeNull();
  });

  it("is not shown again after a restart even if it was never dismissed", async () => {
    calls.status = vi.fn(async () => ({ ...empty(), mouse: mouse(false), scanned: true }));
    const first = render(App);
    await wait();
    expect(prompt()).not.toBeNull();
    first.unmount();
    render(App);
    await wait();
    expect(prompt()).toBeNull();
  });

  it("starts the Test my device wizard with that device, already read, and hides the notice", async () => {
    calls.status = vi.fn(async () => ({ ...empty(), mouse: mouse(false), scanned: true }));
    render(App);
    await wait();
    (screen.getByRole("button", { name: "Send test report" }) as HTMLButtonElement).click();
    await wait();
    expect(location.hash).toBe("#wizard");
    expect(document.body.textContent).toContain("PeriHub only reads Logitech G403 here");
    expect(prompt()).toBeNull();
    expect(calls.openTestForm).not.toHaveBeenCalled();
  });

  it("does not appear for a verified mouse", async () => {
    calls.status = vi.fn(async () => ({ ...empty(), mouse: mouse(true), scanned: true }));
    render(App);
    await wait();
    expect(prompt()).toBeNull();
  });

  it("does not appear before the first scan is over, nor use up the device", async () => {
    calls.status = vi.fn(async () => ({ ...empty(), mouse: mouse(false), scanned: false }));
    render(App);
    await wait();
    expect(prompt()).toBeNull();
    expect(localStorage.getItem("perihub.confirmPrompted")).toBeNull();
  });

  it("opens the wizard from the sidebar without a device picked", async () => {
    calls.status = vi.fn(async () => ({ ...empty(), mouse: mouse(false), scanned: true }));
    render(App);
    await wait();
    (screen.getByRole("button", { name: "Test my device" }) as HTMLButtonElement).click();
    await wait();
    expect(document.body.textContent).toContain("Pick the mouse, keyboard or receiver you want to test");
  });
});

describe("Problems at start-up", () => {
  it("tells the user when the settings file could not be read, until it is closed", async () => {
    calls.status = vi.fn(async () => empty());
    calls.configProblem = vi.fn(async () => "PeriHub could not read its settings file. The old file was kept as /x/config.json.bad-1.");
    render(App);
    await wait();
    expect(screen.getByText(/The old file was kept as \/x\/config.json.bad-1/)).toBeTruthy();
    (screen.getByRole("button", { name: "Close" }) as HTMLButtonElement).click();
    await wait();
    expect(screen.queryByText(/The old file was kept/)).toBeNull();
  });

  it("says nothing about the settings when they were read", async () => {
    calls.status = vi.fn(async () => empty());
    calls.configProblem = vi.fn(async () => null);
    render(App);
    await wait();
    expect(document.querySelector(".config-problem")).toBeNull();
  });

  it("shows why read-only mode could not be turned off", async () => {
    calls.status = vi.fn(async () => empty());
    calls.readOnly = vi.fn(async () => true);
    calls.setReadOnly = vi.fn(async () => {
      throw "file error: disk full";
    });
    render(App);
    await wait();
    (screen.getByRole("button", { name: "Turn off" }) as HTMLButtonElement).click();
    await wait();
    expect(screen.getByText(/Could not turn read-only mode off: file error: disk full/)).toBeTruthy();
  });
});
