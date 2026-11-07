import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const api = vi.hoisted(() => ({
  mouseState: vi.fn(),
  mouseLive: vi.fn(),
  previewDpi: vi.fn(),
  previewLed: vi.fn(),
  revertPreview: vi.fn(),
  saveProfile: vi.fn(),
  restoreFactory: vi.fn(),
  setActiveProfile: vi.fn(),
  setProfileEnabled: vi.fn(),
  appSettings: vi.fn(),
  openTestForm: vi.fn(),
  mouseBackups: vi.fn(),
  restoreMouseBackup: vi.fn(),
}));

vi.mock("../lib/api", async (original) => ({ ...(await original<typeof import("../lib/api")>()), api }));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: async () => "0.5.2" }));

import type { MouseInfo, MouseState, ProfileSettings } from "../lib/api";
import Mouse from "./Mouse.svelte";

const info: MouseInfo = {
  product_id: 0xc08b,
  model: "G502 HERO",
  name: "G502 HERO Gaming Mouse",
  verified: true,
  receiver: false,
  link: null,
  host_mode: false,
  firmware: [{ kind: 0, name: "U1", version: "27.2.1" }],
  dpi: { min: 100, max: 25600, step: 50, list: [] },
  report_rates_ms: [1, 2, 4, 8],
  led_zones: [],
  onboard: {
    memory_model: 1,
    profile_format: 2,
    macro_format: 1,
    profile_count: 2,
    rom_profile_count: 2,
    button_count: 11,
    sector_count: 16,
    sector_size: 256,
    has_gshift: true,
    has_dpi_shift: true,
  },
  writable: true,
};

const settings = (): ProfileSettings => ({
  report_rate_ms: 1,
  dpi_levels: [800, 1600],
  default_dpi_index: 0,
  shift_dpi_index: 0,
  buttons: [{ type: "mouse", button: 1 }],
  gshift_buttons: [],
  leds: [],
});

const state = (): MouseState => ({
  active_profile: 1,
  active_dpi_index: 0,
  current_dpi: 800,
  macro_space: 2032,
  profiles: [
    { number: 1, enabled: true, settings: settings() },
    { number: 2, enabled: true, settings: settings() },
  ],
});

/** Longer than the 150 ms the DPI preview waits for more changes. */
const PREVIEW_WAIT = 300;
const wait = (ms: number) => vi.advanceTimersByTimeAsync(ms);

async function open(mouse = info) {
  const page = render(Mouse, { props: { info: mouse, error: null, asleep: null } });
  await wait(0);
  return page;
}

function firstLevel() {
  return screen.getAllByRole("spinbutton")[0] as HTMLInputElement;
}

async function changeFirstLevel(value: number) {
  await fireEvent.change(firstLevel(), { target: { value: String(value) } });
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.clearAllMocks();
  api.mouseState.mockImplementation(async () => state());
  api.mouseLive.mockImplementation(async () => ({ active_profile: 1, active_dpi_index: 0, current_dpi: 800, battery: null }));
  api.previewDpi.mockResolvedValue(undefined);
  api.revertPreview.mockResolvedValue(undefined);
  api.saveProfile.mockImplementation(async () => state());
  api.setActiveProfile.mockImplementation(async () => ({ ...state(), active_profile: 2 }));
  api.appSettings.mockResolvedValue({ system: "linux x86_64" });
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("Discard", () => {
  it("goes back to the saved values and drops a preview that was not sent", async () => {
    await open();
    await changeFirstLevel(1000);
    expect(firstLevel().value).toBe("1000");

    await fireEvent.click(screen.getByRole("button", { name: "Discard" }));
    await wait(PREVIEW_WAIT);

    expect(firstLevel().value).toBe("800");
    expect(api.previewDpi).not.toHaveBeenCalled();
    expect(api.revertPreview).not.toHaveBeenCalled();
  });

  it("takes back a preview that is already on the mouse", async () => {
    await open();
    await changeFirstLevel(1000);
    await wait(PREVIEW_WAIT);
    expect(api.previewDpi).toHaveBeenCalledWith(1000);

    await fireEvent.click(screen.getByRole("button", { name: "Discard" }));
    await wait(0);

    expect(api.revertPreview).toHaveBeenCalledOnce();
  });

  it("goes back at once even when the mouse does not answer", async () => {
    await open();
    await changeFirstLevel(1000);
    await wait(PREVIEW_WAIT);
    api.revertPreview.mockReturnValue(new Promise(() => {}));

    await fireEvent.click(screen.getByRole("button", { name: "Discard" }));

    expect(firstLevel().value).toBe("800");
  });
});

describe("pending previews", () => {
  it("are dropped by Make active", async () => {
    await open();
    await changeFirstLevel(1000);

    await fireEvent.click(screen.getByRole("button", { name: "Make active" }));
    await wait(PREVIEW_WAIT);

    expect(api.setActiveProfile).toHaveBeenCalledWith(2);
    expect(api.previewDpi).not.toHaveBeenCalled();
  });

  it("are dropped by choosing another profile", async () => {
    await open();
    await changeFirstLevel(1000);

    await fireEvent.click(screen.getByRole("button", { name: /Profile 2/ }));
    await wait(PREVIEW_WAIT);

    expect(api.previewDpi).not.toHaveBeenCalled();
    expect(firstLevel().value).toBe("800");
  });

  it("are dropped by saving", async () => {
    await open();
    await changeFirstLevel(1000);

    await fireEvent.click(screen.getByRole("button", { name: "Save to mouse" }));
    await wait(PREVIEW_WAIT);

    expect(api.saveProfile).toHaveBeenCalledOnce();
    expect(api.saveProfile.mock.calls[0][1].dpi_levels[0]).toBe(1000);
    expect(api.previewDpi).not.toHaveBeenCalled();
  });
});

describe("the saved message", () => {
  async function save() {
    await open();
    await changeFirstLevel(1000);
    await fireEvent.click(screen.getByRole("button", { name: "Save to mouse" }));
    await wait(0);
  }

  it("appears next to the Save button, so the page does not move", async () => {
    await save();
    const message = screen.getByText("Profile 1 saved to the mouse.");
    expect(message.closest("footer")).toBe(screen.getByRole("button", { name: "Save to mouse" }).closest("footer"));
  });

  it("goes away after a few seconds", async () => {
    await save();
    await wait(4000);
    expect(screen.queryByText("Profile 1 saved to the mouse.")).toBeNull();
  });
});

describe("the experimental notice", () => {
  it("offers the test report for a mouse not tested yet, and not for a tested one", async () => {
    const page = await open({ ...info, verified: false, writable: false });
    await wait(0);
    expect(screen.getByRole("button", { name: "Copy test report" })).toBeTruthy();
    page.unmount();

    await open();
    expect(screen.queryByRole("button", { name: "Copy test report" })).toBeNull();
  });
});

describe("Backups", () => {
  const backups = [
    { id: "1794000200-profile1", created: 1794000200, label: "profile1" },
    { id: "1794000100-factory2", created: 1794000100, label: "factory2" },
  ];

  async function openBackups() {
    api.mouseBackups.mockResolvedValue(backups);
    await open();
    await fireEvent.click(screen.getByRole("button", { name: "Backups…" }));
    await wait(0);
  }

  it("lists the backups, newest first, with what they were taken before", async () => {
    await openBackups();
    const dialog = screen.getByRole("dialog");
    expect(dialog.textContent).toContain("Backups of the G502 HERO");
    const items = screen.getAllByRole("listitem").map((li) => li.textContent);
    expect(items[0]).toContain("Before saving profile 1");
    expect(items[1]).toContain("Before resetting profile 2 to factory settings");
  });

  it("restores only after a second click, then shows the saved settings and what changed", async () => {
    await openBackups();
    await changeFirstLevel(1000);
    const restored = state();
    restored.profiles[0].settings.dpi_levels = [400, 1600];
    api.restoreMouseBackup.mockResolvedValue({ state: restored, profiles: [1], macros: false, directory: false });

    const [newest] = screen.getAllByRole("button", { name: "Restore" });
    await fireEvent.click(newest);
    expect(api.restoreMouseBackup).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByRole("button", { name: "Restore this backup?" }));
    await wait(0);

    expect(api.restoreMouseBackup).toHaveBeenCalledWith("1794000200-profile1");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(firstLevel().value).toBe("400");
    expect(screen.getByText("Backup restored: profile 1 changed.")).toBeTruthy();
  });

  it("keeps the dialog open with the error when the restore fails", async () => {
    await openBackups();
    api.restoreMouseBackup.mockRejectedValue("this backup does not match the memory of this mouse");

    await fireEvent.click(screen.getAllByRole("button", { name: "Restore" })[0]);
    await fireEvent.click(screen.getByRole("button", { name: "Restore this backup?" }));
    await wait(0);

    expect(screen.getByRole("dialog").textContent).toContain("this backup does not match the memory of this mouse");
    expect(screen.getByRole("button", { name: "Backups…" })).toBeTruthy();
  });
});
