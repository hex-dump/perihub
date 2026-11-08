import { cleanup, render, screen, waitFor } from "@testing-library/svelte";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const api = vi.hoisted(() => ({
  appSettings: vi.fn(),
  setAppSettings: vi.fn(),
  openLogFolder: vi.fn(),
  openTestForm: vi.fn(),
  quit: vi.fn(),
  checkUpdate: vi.fn(),
  updateAvailable: vi.fn(),
  onUpdate: vi.fn(),
  installUpdate: vi.fn(),
}));

vi.mock("../lib/api", async (original) => ({ ...(await original<typeof import("../lib/api")>()), api }));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: async () => "0.5.2" }));

import type { AppSettings, DeviceStatus } from "../lib/api";
import Settings from "./Settings.svelte";

const settings = (autostart: boolean): AppSettings => ({
  start_hidden: false,
  autostart,
  check_updates: false,
  updates_supported: false,
  package_manager: null,
  log_dir: "/logs",
  system: "windows x86_64",
});

const status: DeviceStatus = {
  keyboard: null,
  mouse: null,
  keyboard_error: null,
  mouse_error: null,
  mouse_asleep: null,
  unsupported_keyboard: null,
  conflicts: [],
  corsair_mouse: null,
  corsair_error: null,
};

const autostartBox = () => screen.getByRole("checkbox", { name: /Start PeriHub when I log in/ }) as HTMLInputElement;

beforeEach(() => {
  vi.clearAllMocks();
  api.appSettings.mockResolvedValue(settings(true));
});

afterEach(cleanup);

describe("Settings", () => {
  it("reads autostart again when the window comes back, as Task Manager can change it", async () => {
    render(Settings, { props: { status } });
    await waitFor(() => expect(autostartBox().checked).toBe(true));

    api.appSettings.mockResolvedValue(settings(false));
    window.dispatchEvent(new Event("focus"));

    await waitFor(() => expect(autostartBox().checked).toBe(false));
  });

  it("stops reading it once the page is closed", async () => {
    const page = render(Settings, { props: { status } });
    await waitFor(() => expect(autostartBox().checked).toBe(true));
    page.unmount();
    api.appSettings.mockClear();

    window.dispatchEvent(new Event("focus"));

    expect(api.appSettings).not.toHaveBeenCalled();
  });

  it("offers the test report only with a mouse connected", async () => {
    render(Settings, { props: { status } });
    await waitFor(() => expect(autostartBox().checked).toBe(true));
    expect(screen.queryByRole("button", { name: "Copy test report" })).toBeNull();
  });
});
