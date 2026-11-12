import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const api = vi.hoisted(() => ({
  peripheralInfo: vi.fn(),
  peripheralChange: vi.fn(),
  peripheralKeepChanges: vi.fn(),
  peripheralUndo: vi.fn(),
  peripheralSetBlind: vi.fn(),
  appSettings: vi.fn(),
}));

vi.mock("../lib/api", async (original) => ({
  ...(await original<typeof import("../lib/api")>()),
  api,
}));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: async () => "0.7.0" }));
api.appSettings.mockResolvedValue({ system: "linux x86_64" });

import type { PeripheralInfo } from "../lib/api";
import { peripheralTest } from "../lib/testReport";
import Peripheral from "./Peripheral.svelte";

const info: PeripheralInfo = {
  key: "0b05:195c",
  vendor_id: 0x0b05,
  product_id: 0x195c,
  brand: "ASUS",
  model: "ROG Keris",
  kind: "mouse",
  source: "libratbag (MIT)",
  profiles: 3,
  firmware: "1.02.03",
  battery: null,
  active_profile: 2,
  active_dpi: 2,
  readable: true,
  settings: {
    dpi_levels: [400, 800, 1600, 3200],
    polling_hz: 1000,
    options: { debounce: 1, angle_snapping: 0 },
    leds: [
      { mode: 0, brightness: 100, color: "#ff0000" },
      { mode: 1, brightness: 50, color: "#0000ff" },
    ],
  },
  limits: {
    dpi: { min: 100, max: 16000, step: 100 },
    polling_hz: [125, 250, 500, 1000],
    options: [
      {
        id: "debounce",
        label: "Button response",
        values: [
          [0, "4 ms"],
          [1, "8 ms"],
        ],
      },
      {
        id: "angle_snapping",
        label: "Angle snapping",
        values: [
          [0, "Off"],
          [1, "On"],
        ],
      },
    ],
    leds: [
      {
        name: "LED 1",
        modes: [
          [0, "Static"],
          [1, "Breathing"],
          [2, "Colour cycle"],
        ],
        brightness_step: 25,
        color: true,
      },
      {
        name: "LED 2",
        modes: [
          [0, "Static"],
          [1, "Breathing"],
          [2, "Colour cycle"],
        ],
        brightness_step: 25,
        color: true,
      },
    ],
  },
  buttons: [{ name: "Left", action: "Left" }],
  blind: [],
  editable: true,
  can_save: true,
  changes_stay: false,
  problem: null,
  pending_undo: false,
  raw: ["12 04 00 00 → 12 04 00 00 03 00"],
};

const wait = (ms: number) => vi.advanceTimersByTimeAsync(ms);

beforeEach(() => {
  vi.useFakeTimers();
  vi.clearAllMocks();
  localStorage.clear();
  api.peripheralInfo.mockResolvedValue(info);
  api.peripheralChange.mockImplementation(
    async (_key: string, settings: PeripheralInfo["settings"]) => ({
      ...info,
      settings,
    }),
  );
  api.peripheralKeepChanges.mockResolvedValue(undefined);
  api.peripheralUndo.mockResolvedValue(info);
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

async function setPolling(hz: number) {
  const select = screen.getByRole("combobox", {
    name: "Polling rate",
  }) as HTMLSelectElement;
  select.value = String(hz);
  await fireEvent.change(select);
}

describe("a mouse of another brand", () => {
  it("shows what was read, says nobody has tried it, and changes nothing by itself", async () => {
    render(Peripheral, { props: { info, error: null } });
    await wait(0);
    expect(screen.getByText("ASUS ROG Keris")).toBeTruthy();
    expect(document.body.textContent).toContain(
      "Nobody has tried PeriHub on the ASUS ROG Keris yet",
    );
    expect(document.body.textContent).toContain("libratbag (MIT)");
    expect(screen.getByText("1.02.03")).toBeTruthy();
    expect(api.peripheralChange).not.toHaveBeenCalled();
    expect(
      (screen.getByRole("button", { name: "Apply" }) as HTMLButtonElement)
        .disabled,
    ).toBe(true);
  });

  it("always shows the warning next to Apply, without asking for anything more", async () => {
    render(Peripheral, { props: { info, error: null } });
    expect(document.body.textContent).toContain(
      "sends the change without saving it to the device's memory",
    );
    await setPolling(500);
    await fireEvent.click(screen.getByRole("button", { name: "Apply" }));
    await wait(0);
    expect(api.peripheralChange).toHaveBeenCalledWith(
      "0b05:195c",
      { ...info.settings, polling_hz: 500 },
      false,
      2,
    );
    expect(screen.queryByRole("checkbox")).toBeNull();
  });

  it("puts the settings back after 15 seconds unless they are kept", async () => {
    render(Peripheral, { props: { info, error: null } });
    await setPolling(250);
    await fireEvent.click(screen.getByRole("button", { name: "Apply" }));
    await wait(0);
    expect(screen.getByRole("alertdialog")).toBeTruthy();
    await wait(15_000);
    expect(api.peripheralUndo).toHaveBeenCalledWith("0b05:195c");
    expect(api.peripheralKeepChanges).not.toHaveBeenCalled();
  });

  it("keeps the change when asked", async () => {
    render(Peripheral, { props: { info, error: null } });
    await setPolling(250);
    await fireEvent.click(screen.getByRole("button", { name: "Apply" }));
    await wait(0);
    await fireEvent.click(screen.getByRole("button", { name: "Keep changes" }));
    await wait(20_000);
    expect(api.peripheralKeepChanges).toHaveBeenCalledWith("0b05:195c");
    expect(api.peripheralUndo).not.toHaveBeenCalled();
  });

  it("saves to memory only after the No warranty box, asked once", async () => {
    render(Peripheral, { props: { info, error: null } });
    await fireEvent.click(
      screen.getByRole("button", { name: "Save to memory…" }),
    );
    const save = screen.getByRole("button", {
      name: "Save to memory",
    }) as HTMLButtonElement;
    expect(save.disabled).toBe(true);
    expect(screen.getByRole("note").textContent).toContain(
      "accepts no responsibility for any damage",
    );
    expect(api.peripheralChange).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByRole("checkbox"));
    await fireEvent.click(save);
    await wait(0);
    expect(api.peripheralChange).toHaveBeenCalledWith(
      "0b05:195c",
      info.settings,
      true,
      2,
    );
    await fireEvent.click(screen.getByRole("button", { name: "Keep changes" }));
    await wait(0);
    await fireEvent.click(
      screen.getByRole("button", { name: "Save to memory…" }),
    );
    await wait(0);
    expect(api.peripheralChange).toHaveBeenCalledTimes(2);
    expect(screen.queryByRole("checkbox")).toBeNull();
  });

  it("offers no change on a device that answered something unexpected", () => {
    render(Peripheral, {
      props: {
        info: {
          ...info,
          editable: false,
          problem: "unexpected answer: polling rate index 9",
        },
        error: null,
      },
    });
    expect(document.body.textContent).toContain("polling rate index 9");
    expect(screen.queryByRole("button", { name: "Apply" })).toBeNull();
    expect(
      screen.queryByRole("button", { name: "Save to memory…" }),
    ).toBeNull();
    expect(screen.queryByRole("combobox")).toBeNull();
    expect(document.body.textContent).toContain("PeriHub reads only this one");
    expect(document.body.textContent).not.toContain("changes only this one");
  });

  it("does not offer to change the DPI where the device's DPI is not changed", () => {
    render(Peripheral, {
      props: {
        info: { ...info, limits: { ...info.limits, dpi: null } },
        error: null,
      },
    });
    expect(screen.queryByRole("spinbutton")).toBeNull();
    expect(document.body.textContent).toContain("not changed on this device");
  });

  it("explains when the device could not be opened", () => {
    render(Peripheral, {
      props: { info: null, error: "ASUS ROG Keris: permission denied" },
    });
    expect(screen.getByText("ASUS ROG Keris: permission denied")).toBeTruthy();
  });

  it("puts every request and answer in the test report", () => {
    const report = peripheralTest(info, "0.7.0", "Linux").report;
    expect(report).toContain(
      "ASUS ROG Keris (mouse), USB ID 0b05:195c, protocol from libratbag (MIT)",
    );
    expect(report).toContain(
      "DPI levels: 400 / 800 / 1600 / 3200, polling: 1000 Hz",
    );
    expect(report).toContain("Button response: 8 ms, Angle snapping: Off");
    expect(report).toContain("  12 04 00 00 → 12 04 00 00 03 00");
  });
});

const rival3: PeripheralInfo = {
  ...info,
  key: "1038:1824",
  vendor_id: 0x1038,
  product_id: 0x1824,
  brand: "SteelSeries",
  model: "Rival 3",
  source: "rivalcfg (WTFPL)",
  profiles: 1,
  firmware: "1.37",
  active_profile: null,
  active_dpi: null,
  readable: false,
  can_save: false,
  settings: { dpi_levels: [], polling_hz: null, options: {}, leds: [] },
  limits: { dpi: null, polling_hz: [], options: [], leds: [] },
  buttons: [],
  blind: [
    {
      id: "sensitivity",
      label: "Sensitivity presets",
      kind: { type: "dpi", min: 200, max: 8500, step: 100, max_presets: 5 },
      last: null,
    },
    {
      id: "polling_rate",
      label: "Polling rate",
      kind: { type: "choice", values: ["125", "250", "500", "1000"] },
      last: null,
    },
    {
      id: "logo_color",
      label: "Logo LED color",
      kind: { type: "color" },
      last: null,
    },
  ],
};

describe("a mouse that cannot report its settings", () => {
  it("always warns that nothing can be read, checked or undone, and how to go back", () => {
    render(Peripheral, { props: { info: rival3, error: null } });
    expect(document.body.textContent).toContain("cannot report its settings");
    expect(document.body.textContent).toContain(
      "unplug it and plug it back in",
    );
    expect(screen.queryByRole("button", { name: "Apply" })).toBeNull();
    expect(
      screen.queryByRole("button", { name: "Save to memory…" }),
    ).toBeNull();
    expect(screen.getAllByText("Last sent: not sent yet").length).toBe(3);
  });

  it("sends one setting at a time, without asking for anything more", async () => {
    api.peripheralSetBlind.mockResolvedValue(rival3);
    render(Peripheral, { props: { info: rival3, error: null } });
    const select = screen.getByRole("combobox", {
      name: "Polling rate",
    }) as HTMLSelectElement;
    select.value = "500";
    await fireEvent.change(select);
    await fireEvent.click(screen.getAllByRole("button", { name: "Send" })[1]);
    await wait(0);
    expect(api.peripheralSetBlind).toHaveBeenCalledWith(
      "1038:1824",
      "polling_rate",
      { type: "choice", value: "500" },
    );
    expect(screen.queryByRole("checkbox")).toBeNull();
    expect(screen.queryByRole("alertdialog")).toBeNull();
  });

  it("refuses DPI presets the mouse does not take before sending anything", async () => {
    render(Peripheral, { props: { info: rival3, error: null } });
    const dpi = screen.getByRole("textbox", {
      name: "Sensitivity presets",
    }) as HTMLInputElement;
    dpi.value = "800, 1650";
    await fireEvent.change(dpi);
    await fireEvent.click(screen.getAllByRole("button", { name: "Send" })[0]);
    expect(document.body.textContent).toContain(
      "each from 200 to 8500 DPI in steps of 100",
    );
    dpi.value = "400, 800, 1600, 3200, 6400, 8000";
    await fireEvent.change(dpi);
    await fireEvent.click(screen.getAllByRole("button", { name: "Send" })[0]);
    expect(document.body.textContent).toContain("at most 5 presets");
    expect(api.peripheralSetBlind).not.toHaveBeenCalled();
    dpi.value = "800, 1600";
    await fireEvent.change(dpi);
    await fireEvent.click(screen.getAllByRole("button", { name: "Send" })[0]);
    expect(api.peripheralSetBlind).toHaveBeenCalledWith(
      "1038:1824",
      "sensitivity",
      { type: "dpi", value: [800, 1600] },
    );
  });

  it("offers nothing to send on a read-only connection", () => {
    render(Peripheral, {
      props: { info: { ...rival3, editable: false }, error: null },
    });
    expect(screen.queryByRole("button", { name: "Send" })).toBeNull();
  });

  it("reports what was last sent and the battery", () => {
    const report = peripheralTest(
      {
        ...rival3,
        battery: { level: 80, charging: true },
        blind: [{ ...rival3.blind[1], last: { type: "choice", value: "500" } }],
      },
      "0.7.0",
      "Linux",
    ).report;
    expect(report).toContain("Battery: 80 %, charging");
    expect(report).toContain("Sent: Polling rate = 500");
    expect(report).toContain("cannot be read back");
  });
});

describe("a mouse that stores every change", () => {
  const razer: PeripheralInfo = {
    ...info,
    key: "1532:0084",
    vendor_id: 0x1532,
    product_id: 0x0084,
    brand: "Razer",
    model: "DeathAdder V2",
    source: "OpenRazer (facts only)",
    profiles: 1,
    active_profile: null,
    active_dpi: null,
    can_save: false,
    changes_stay: true,
    settings: { dpi_levels: [800], polling_hz: 1000, options: {}, leds: [] },
    limits: {
      dpi: { min: 100, max: 20000, step: 50 },
      polling_hz: [125, 500, 1000],
      options: [],
      leds: [],
    },
    buttons: [],
  };

  it("offers only Save to memory, after the No warranty box, and the change is undone unless kept", async () => {
    render(Peripheral, { props: { info: razer, error: null } });
    expect(screen.queryByRole("button", { name: "Apply" })).toBeNull();
    expect(document.body.textContent).toContain(
      "stores every change in its memory by itself",
    );
    await setPolling(500);
    await fireEvent.click(
      screen.getByRole("button", { name: "Save to memory…" }),
    );
    expect(api.peripheralChange).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByRole("checkbox"));
    await fireEvent.click(
      screen.getByRole("button", { name: "Save to memory" }),
    );
    await wait(0);
    expect(api.peripheralChange).toHaveBeenCalledWith(
      "1532:0084",
      { ...razer.settings, polling_hz: 500 },
      false,
      null,
    );
    await wait(15_000);
    expect(api.peripheralUndo).toHaveBeenCalledWith("1532:0084");
  });
});

describe("a QMK keyboard", () => {
  const keyboard: PeripheralInfo = {
    ...info,
    key: "3434:0311",
    vendor_id: 0x3434,
    product_id: 0x0311,
    brand: "Keychron",
    model: "Q1 Pro",
    kind: "keyboard",
    source: "the QMK and Vial firmware sources (no code copied)",
    profiles: 1,
    firmware: "VIA protocol 13, 4 layers, 16 macros",
    active_profile: null,
    active_dpi: null,
    settings: {
      dpi_levels: [],
      polling_hz: null,
      options: { rgb_matrix_speed: 128 },
      leds: [{ mode: 4, brightness: 80, color: "#ff0000" }],
    },
    limits: {
      dpi: null,
      polling_hz: [],
      options: [
        {
          id: "rgb_matrix_speed",
          label: "Per-key RGB speed",
          values: [
            [0, "0 of 255"],
            [128, "128 of 255"],
          ],
        },
      ],
      leds: [
        {
          name: "Per-key RGB",
          modes: [
            [0, "Off"],
            [4, "Effect 4"],
          ],
          brightness_step: 10,
          color: true,
        },
      ],
    },
    buttons: [],
  };

  it("shows its lighting and no DPI or polling rate", async () => {
    render(Peripheral, { props: { info: keyboard, error: null } });
    await wait(0);
    expect(screen.getByText("Keychron Q1 Pro")).toBeTruthy();
    expect(
      screen.getByText("VIA protocol 13, 4 layers, 16 macros"),
    ).toBeTruthy();
    expect(screen.queryByText("DPI levels")).toBeNull();
    expect(screen.queryByText("Polling rate")).toBeNull();
    expect(screen.getByText("Per-key RGB")).toBeTruthy();
    expect(
      screen.getByRole("combobox", { name: "Per-key RGB speed" }),
    ).toBeTruthy();
  });

  it("sends a test report without mouse lines", () => {
    const report = peripheralTest(keyboard, "0.7.0", "linux x86_64").report;
    expect(report).toContain("Firmware: VIA protocol 13, 4 layers, 16 macros");
    expect(report).toContain("LED 1: mode 4, brightness 80, #ff0000");
    expect(report).not.toContain("DPI");
    expect(report).not.toContain("polling");
  });

  it("asks to look at the lighting before keeping a change", async () => {
    render(Peripheral, { props: { info: keyboard, error: null } });
    const mode = screen.getByRole("combobox", {
      name: "Per-key RGB mode",
    }) as HTMLSelectElement;
    mode.value = "0";
    await fireEvent.change(mode);
    await fireEvent.click(screen.getByRole("button", { name: "Apply" }));
    await wait(0);
    expect(api.peripheralChange.mock.calls[0][1].leds[0].mode).toBe(0);
    expect(screen.getByRole("alertdialog").textContent).toContain(
      "look at its lighting",
    );
  });
});
