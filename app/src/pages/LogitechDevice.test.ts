import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const api = vi.hoisted(() => ({
  appSettings: vi.fn(async () => ({ system: "linux x86_64" })),
  openTestForm: vi.fn(),
  logitechLive: vi.fn(),
  logitechSetDpi: vi.fn(),
  logitechResetDpi: vi.fn(),
  logitechDriveLights: vi.fn(),
  logitechGiveBackLights: vi.fn(),
  logitechSetBrightness: vi.fn(),
}));
vi.mock("../lib/api", async (original) => ({ ...(await original<typeof import("../lib/api")>()), api }));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: async () => "0.6.7" }));

import type { LogitechLive, ReceiverReport, UnsupportedReceiverDevice } from "../lib/api";
import { acceptNoWarranty } from "../lib/noWarranty";
import LogitechDevice from "./LogitechDevice.svelte";

const device: UnsupportedReceiverDevice = {
  slot: 1,
  name: "MX Master 3S",
  kind: 3,
  protocol: "4.5",
  wireless_id: null,
  device_info: null,
  firmware: ["RBM 12.01.0008"],
  features: [0x0001, 0x1004, 0x1b04, 0x2201, 0x2110, 0x2121, 0x2150, 0x1815].map((id) => ({ id, kind: 0, version: 0 })),
  dpi: { min: 200, max: 8000, step: 50, list: [] },
  report_rates_ms: [],
  onboard: null,
  lighting: null,
  read_errors: [],
};

const receiver: ReceiverReport = {
  product_id: 0xc548,
  name: "Logi Bolt receiver",
  collections: [],
  bolt: true,
  unifying: false,
  reachable: true,
  denied: false,
  open_slots: [1],
  slots: [],
  unsupported: [device],
};

let live: LogitechLive;
const wait = (ms: number) => vi.advanceTimersByTimeAsync(ms);

async function open(props: Record<string, unknown> = {}) {
  render(LogitechDevice, { props: { receiver, device, ...props } });
  await wait(0);
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.clearAllMocks();
  live = { battery: { percent: 80, estimated: false, state: "discharging" }, dpi: 1000, saved_dpi: null, lighting: null, host: null, gave_back: null };
  api.logitechLive.mockImplementation(async () => live);
  api.logitechSetDpi.mockImplementation(async (_r: number, _s: number, dpi: number) => {
    live = { ...live, dpi, saved_dpi: dpi };
  });
  api.logitechResetDpi.mockImplementation(async () => {
    live = { ...live, dpi: 1000, saved_dpi: null, lighting: null };
  });
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

function dpiBox() {
  return screen.getByRole("spinbutton", { name: "DPI" }) as HTMLInputElement;
}

describe("a Logitech keyboard's lighting", () => {
  const lighting = {
    brightness: { min: 0, max: 100, steps: 5, on_off: true },
    effects: {
      zones: [
        {
          index: 0,
          location: 1,
          effects: [
            { index: 0, id: 0, caps: 0, period_ms: 0 },
            { index: 1, id: 1, caps: 1, period_ms: 0 },
            { index: 2, id: 0x15, caps: 4, period_ms: 10000 },
          ],
        },
      ],
      startup: { enabled: true, colors: ["#ff0081", "#80aaff"] as [string, string] },
      shutdown: null,
      idle_s: 60,
      sleep_s: 300,
    },
    keys: [4, 5, 6, 7],
    layout: 1,
    raw: ["8071 0 (ff ff 00): ff ff 01 00 00 00 01", "8040 0: 00 64 05 04"],
    errors: ["8071 function 3 (00 00 40): HID++ error 0x02"],
  };
  const keyboard = { ...device, name: "G915 X LS", kind: 0, dpi: null, features: [], lighting };

  it("is shown as read, with nothing to change", async () => {
    live = { battery: null, dpi: null, saved_dpi: null, lighting: { brightness: 50, on: true, control: 0 }, host: null, gave_back: null };
    await open({ device: keyboard, receiver: { ...receiver, unsupported: [keyboard] } });
    const card = screen.getByRole("heading", { name: /Lighting/ }).closest("section")!;
    expect(card.textContent).toContain("Read only");
    expect(card.textContent).toContain("50 of 100");
    expect(card.textContent).toContain("The keyboard itself, with its own effects");
    expect(card.textContent).toContain("Off, Fixed colour, Colour cycle");
    expect(card.textContent).toContain("4 keys");
    expect(card.textContent).toContain("Dims after 1 min, off 5 min later");
    expect(card.textContent).toContain("code 1");
    expect(card.querySelectorAll("button, input, select").length).toBe(0);
  });

  it("says when a program drives the lights", async () => {
    live = { battery: null, dpi: null, saved_dpi: null, lighting: { brightness: 50, on: false, control: 3 }, host: null, gave_back: null };
    await open({ device: keyboard, receiver: { ...receiver, unsupported: [keyboard] } });
    const card = screen.getByRole("heading", { name: /Lighting/ }).closest("section")!;
    expect(card.textContent).toContain("A program, such as G HUB");
    expect(card.textContent).toContain("lights off");
  });
});

describe("a Logitech keyboard whose lights PeriHub can drive", () => {
  const effect = (index: number, id: number, caps: number, period_ms: number) => ({ index, id, caps, period_ms });
  const keys = [...Array.from({ length: 111 }, (_, i) => i + 1), 153, 155, 156, 157, 158, 180, 181, 210];
  const lighting = {
    brightness: { min: 0, max: 100, steps: 5, on_off: false },
    effects: {
      zones: [
        {
          index: 0,
          location: 1,
          effects: [
            effect(0, 0x00, 0, 0),
            effect(1, 0x01, 0x0001, 0),
            effect(2, 0x0a, 0xc001, 7691),
            effect(3, 0x15, 0xc005, 992),
            effect(4, 0x16, 0xdce1, 30),
            effect(5, 0x17, 0, 50),
          ],
        },
        { index: 1, location: 2, effects: [effect(0, 0x00, 0, 0), effect(1, 0x01, 0x0001, 0), effect(2, 0x0a, 0xc001, 7691)] },
      ],
      startup: null,
      shutdown: null,
      idle_s: 60,
      sleep_s: 270,
    },
    keys,
    layout: 4,
    raw: [],
    errors: [],
  };
  const features = [0x8040, 0x8071, 0x8081, 0x8101].map((id) => ({ id, kind: 0, version: 0 }));
  const keyboard = { ...device, name: "G915 X LS", kind: 0, dpi: null, features, lighting };
  const props = { device: keyboard, receiver: { ...receiver, product_id: 0xc547, unsupported: [keyboard] } };
  const host = (look: LogitechLive["host"] extends infer H ? (H extends { look: infer L } ? L : never) : never) => ({
    look,
    power: "full" as const,
    idle_events: 0,
    active_events: 0,
    retaken: 0,
  });

  beforeEach(() => {
    live = {
      battery: { percent: 89, estimated: false, state: "discharging" },
      dpi: null,
      saved_dpi: null,
      lighting: { brightness: 100, on: null, control: 0 },
      host: null,
      gave_back: null,
    };
    api.logitechDriveLights.mockImplementation(async (_r: number, _s: number, look) => {
      live = { ...live, host: host(look), lighting: { brightness: 100, on: null, control: 3 } };
    });
    api.logitechGiveBackLights.mockImplementation(async () => {
      live = { ...live, host: null, lighting: { brightness: 100, on: null, control: 0 } };
    });
  });

  function select(name: string) {
    return screen.getByRole("combobox", { name }) as HTMLSelectElement;
  }

  it("takes the lights over after the No warranty box, with a fixed colour on each zone", async () => {
    await open(props);
    const card = screen.getByRole("heading", { name: /Lighting/ }).closest("section")!;
    expect(card.textContent).toContain("Experimental");
    expect(screen.getByText(/It can drive its lights/)).toBeTruthy();
    // The keys offer per-key colours; the wave has no inward or outward direction on this keyboard.
    expect([...select("Effect, Keys").options].map((o) => o.text)).toEqual([
      "Off",
      "Fixed colour",
      "Colours per key",
      "Breathing",
      "Colour cycle",
      "Colour wave",
      "Ripple",
    ]);
    await fireEvent.click(screen.getByRole("button", { name: "Drive the lights" }));
    expect(api.logitechDriveLights).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByRole("checkbox", { name: /I have read this/ }));
    await fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    await wait(0);
    expect(api.logitechDriveLights).toHaveBeenCalledWith(0xc547, 1, {
      zones: [
        { zone: 0, effect: 1, color: "#ffffff", period_ms: null, direction: null },
        { zone: 1, effect: 1, color: "#ffffff", period_ms: null, direction: null },
      ],
      keys: [],
    });
    expect(screen.getByText("PeriHub drives the lights")).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Give the lights back" }));
    await wait(0);
    expect(api.logitechGiveBackLights).toHaveBeenCalledWith(0xc547, 1);
    expect(screen.queryByText("PeriHub drives the lights")).toBeNull();
  });

  it("sends a wave with its period and direction, and colours per group of keys", async () => {
    acceptNoWarranty("lights:G915 X LS");
    await open(props);
    await fireEvent.change(select("Effect, Logo"), { target: { value: "10" } });
    await fireEvent.input(screen.getByRole("spinbutton", { name: "Seconds" }), { target: { value: "3" } });
    await fireEvent.change(select("Effect, Keys"), { target: { value: "-1" } });
    await fireEvent.input(screen.getByLabelText("Colour, Keys"), { target: { value: "#ff0000" } });
    await fireEvent.input(screen.getByLabelText("Letters"), { target: { value: "#0000ff" } });
    await fireEvent.click(screen.getByRole("button", { name: "Drive the lights" }));
    await wait(0);
    const look = api.logitechDriveLights.mock.calls[0][2];
    expect(look.zones).toEqual([
      { zone: 0, effect: 1, color: "#ff0000", period_ms: null, direction: null },
      { zone: 1, effect: 0x0a, color: "#ffffff", period_ms: 3000, direction: null },
    ]);
    expect(look.keys).toHaveLength(keys.length);
    expect(look.keys.filter(([, c]: [number, string]) => c === "#0000ff").map(([k]: [number, string]) => k)).toEqual(
      Array.from({ length: 26 }, (_, i) => i + 1),
    );
    expect(look.keys.find(([k]: [number, string]) => k === 210)).toEqual([210, "#ff0000"]);

    await fireEvent.change(select("Effect, Keys"), { target: { value: "22" } });
    expect([...select("Direction").options].map((o) => o.value)).toEqual(["0", "1", "6", "2", "7", "3", "8"]);
  });

  it("starts from what PeriHub shows, and says when it gave the lights back", async () => {
    const look = {
      zones: [
        { zone: 0, effect: 1, color: "#00ff00", period_ms: null, direction: null },
        { zone: 1, effect: 0, color: null, period_ms: null, direction: null },
      ],
      keys: keys.map((k) => [k, "#00ff00"] as [number, string]),
    };
    live = { ...live, host: { ...host(look), power: "dimmed", retaken: 1 } };
    await open(props);
    expect(select("Effect, Keys").value).toBe("-1");
    expect(select("Effect, Logo").value).toBe("0");
    expect(screen.getByText(/dimmed: nobody is using it/)).toBeTruthy();
    expect(screen.getByText(/taken again 1 time/)).toBeTruthy();
    cleanup();

    live = { ...live, host: null, gave_back: "Logitech G HUB started" };
    await open(props);
    expect(screen.getByText(/PeriHub gave the G915 X LS its own lights back: Logitech G HUB started/)).toBeTruthy();
  });

  it("sets the brightness", async () => {
    acceptNoWarranty("lights:G915 X LS");
    await open(props);
    await fireEvent.input(screen.getByRole("slider", { name: "Brightness" }), { target: { value: "60" } });
    await fireEvent.click(screen.getByRole("button", { name: "Set brightness" }));
    await wait(0);
    expect(api.logitechSetBrightness).toHaveBeenCalledWith(0xc547, 1, 60);
  });

  it("waits while the keyboard is asleep", async () => {
    await open({ ...props, receiver: { ...props.receiver, open_slots: [] } });
    expect(screen.getByText(/press a key or turn it on to change its lights/)).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Drive the lights" })).toBeNull();
  });
});

describe("a Logitech device PeriHub knows by its features", () => {
  it("shows what it is, what it can do, its battery and DPI now, and offers a test report", async () => {
    await open();
    expect(screen.getByRole("heading", { name: "Logitech MX Master 3S" })).toBeTruthy();
    expect(screen.getByText(/Mouse · through its Logi Bolt receiver, slot 1/)).toBeTruthy();
    expect(screen.getByText("80%")).toBeTruthy();
    expect(screen.getAllByText("1000")[0]).toBeTruthy();
    const items = screen.getAllByRole("listitem").map((li) => li.textContent);
    expect(items).toEqual([
      "Battery level",
      "Adjustable DPI",
      "SmartShift wheel",
      "High-resolution wheel",
      "Thumb wheel",
      "Programmable buttons and keys",
    ]);
    expect(screen.getByText(/And 1 more feature PeriHub has no name for yet/)).toBeTruthy();
    expect(screen.getByText("200 to 8000, in steps of 50")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Copy test report" })).toBeTruthy();
  });

  it("asks for the No warranty box before the first DPI change only", async () => {
    await open({ device: { ...device, name: "MX Anywhere 3" } });
    await fireEvent.input(dpiBox(), { target: { value: "1600" } });
    await fireEvent.click(screen.getByRole("button", { name: "Set DPI" }));
    expect(api.logitechSetDpi).not.toHaveBeenCalled();
    expect(screen.getByRole("note").textContent).toContain("accepts no responsibility for any damage");
    const confirm = screen.getAllByRole("button", { name: "Set DPI" }).at(-1) as HTMLButtonElement;
    expect(confirm.disabled).toBe(true);
    await fireEvent.click(screen.getByRole("checkbox", { name: /I have read this/ }));
    await fireEvent.click(confirm);
    await wait(0);
    expect(api.logitechSetDpi).toHaveBeenCalledWith(0xc548, 1, 1600);
    expect(screen.getByText("DPI set to 1600.")).toBeTruthy();
    expect(screen.getByText(/PeriHub sets 1600 DPI every time the MX Anywhere 3 connects/)).toBeTruthy();

    // The second change goes straight to the device.
    await fireEvent.input(dpiBox(), { target: { value: "2400" } });
    await fireEvent.click(screen.getByRole("button", { name: "Set DPI" }));
    await wait(0);
    expect(api.logitechSetDpi).toHaveBeenLastCalledWith(0xc548, 1, 2400);
    expect(screen.queryByRole("note")).toBeNull();
  });

  it("puts the device's own DPI back", async () => {
    acceptNoWarranty("dpi:MX Master 3S");
    live = { ...live, dpi: 1600, saved_dpi: 1600 };
    await open();
    await fireEvent.click(screen.getByRole("button", { name: "Back to the device's own DPI" }));
    await wait(0);
    expect(api.logitechResetDpi).toHaveBeenCalledWith(0xc548, 1);
    expect(screen.queryByText(/PeriHub sets 1600 DPI/)).toBeNull();
  });

  it("waits for a device that is asleep", async () => {
    await open({ receiver: { ...receiver, open_slots: [] } });
    expect(api.logitechLive).not.toHaveBeenCalled();
    expect(screen.getByText(/The MX Master 3S is asleep: move it or turn it on/)).toBeTruthy();
  });

  it("warns that Solaar may undo what PeriHub sets", async () => {
    await open({ programs: ["Solaar"] });
    expect(screen.getByText("Solaar is running.")).toBeTruthy();
  });

  it("only reads a device without adjustable DPI", async () => {
    await open({ device: { ...device, dpi: null } });
    expect(screen.getByText(/Not supported yet/)).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Set DPI" })).toBeNull();
  });

  it("shows the onboard memory format of a device that has one", async () => {
    const onboard = {
      memory_model: 1,
      profile_format: 5,
      macro_format: 1,
      profile_count: 5,
      rom_profile_count: 5,
      button_count: 13,
      sector_count: 16,
      sector_size: 255,
      has_gshift: false,
      has_dpi_shift: true,
    };
    await open({ device: { ...device, onboard } });
    expect(screen.getByText(/5 profiles, profile format 5,\s+255-byte sectors/)).toBeTruthy();
    // Its active profile sets the DPI: PeriHub does not change it.
    expect(screen.queryByRole("button", { name: "Set DPI" })).toBeNull();
    expect(screen.getByText(/Not supported yet/)).toBeTruthy();
  });

  it("says when the device is gone", async () => {
    await open({ receiver: null, device: null });
    expect(screen.getByText(/This device is not connected, or it is asleep/)).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Copy test report" })).toBeNull();
  });
});
