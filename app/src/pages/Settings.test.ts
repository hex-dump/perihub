import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/svelte";
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
import { updates } from "../lib/updates.svelte";

const settings = (autostart: boolean): AppSettings => ({
  start_hidden: false,
  autostart,
  check_updates: false,
  updates_supported: false,
  package_manager: null,
  log_dir: "/logs",
  config_dir: "/config",
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
};

const autostartBox = () => screen.getByRole("checkbox", { name: /Start PeriHub when I log in/ }) as HTMLInputElement;

beforeEach(() => {
  vi.clearAllMocks();
  api.appSettings.mockResolvedValue(settings(true));
});

afterEach(cleanup);

describe("Settings", () => {
  it("says where the settings are stored for this system", async () => {
    const page = render(Settings, { props: { status } });
    await waitFor(() => expect(screen.getByText("/config")).toBeTruthy());
    expect(page.container.textContent).toContain("HKCU");
    cleanup();
    api.appSettings.mockResolvedValue({ ...settings(true), system: "linux x86_64" });
    const linux = render(Settings, { props: { status: { ...status, sandboxed: true } } });
    await waitFor(() => expect(linux.container.textContent).toContain("Flatpak portal"));
    expect(linux.container.textContent).toContain("71");
  });

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

  it("says before installing that there is no installer window and PeriHub opens again by itself", async () => {
    api.appSettings.mockResolvedValue({ ...settings(true), updates_supported: true, check_updates: true });
    api.installUpdate.mockReturnValue(new Promise(() => {}));
    updates.available = { version: "0.8.2" };
    render(Settings, { props: { status } });
    const button = await screen.findByRole("button", { name: "Update and restart" });
    expect(document.body.textContent).not.toContain("installs the update without any window");
    await fireEvent.click(button);
    expect(api.installUpdate).toHaveBeenCalled();
    expect(document.body.textContent).toContain("installs the update without any window and opens again by itself");
    updates.available = null;
    updates.installing = false;
  });

  it("opens the credits in a dialog that Esc closes, and focus goes back to the button", async () => {
    render(Settings, { props: { status } });
    const button = screen.getByRole("button", { name: "Credits" });
    expect(screen.queryByRole("dialog")).toBeNull();
    button.focus();
    await fireEvent.click(button);
    // The dialog is loaded when first opened.
    const dialog = await screen.findByRole("dialog", { name: "Credits" });
    expect(dialog.getAttribute("aria-modal")).toBe("true");
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Close" }));
    for (const text of ["libratbag", "rivalcfg", "WTFPL", "GPL-2.0-or-later", "OpenRGB", "Solaar", "No code was copied."]) {
      expect(dialog.textContent).toContain(text);
    }
    expect(screen.getByRole("region", { name: "List of credits" }).getAttribute("tabindex")).toBe("0");

    await fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(document.activeElement).toBe(button);
  });

  it("closes the credits with the Close button, and keeps Tab inside", async () => {
    render(Settings, { props: { status } });
    const button = screen.getByRole("button", { name: "Credits" });
    button.focus();
    await fireEvent.click(button);
    const close = await screen.findByRole("button", { name: "Close" });
    const list = screen.getByRole("region", { name: "List of credits" });
    close.focus();
    await fireEvent.keyDown(close, { key: "Tab", shiftKey: true });
    expect(document.activeElement).toBe(list);
    await fireEvent.keyDown(list, { key: "Tab" });
    expect(document.activeElement).toBe(close);

    await fireEvent.click(close);
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(document.activeElement).toBe(button);
  });

  it("offers the test report only with a mouse connected", async () => {
    render(Settings, { props: { status } });
    await waitFor(() => expect(autostartBox().checked).toBe(true));
    expect(screen.queryByRole("button", { name: "Copy test report" })).toBeNull();
  });
});
