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
  setReadOnly: vi.fn(),
  writeLog: vi.fn(),
  clearWriteLog: vi.fn(),
}));

vi.mock("../lib/api", async (original) => ({ ...(await original<typeof import("../lib/api")>()), api }));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: async () => "0.5.2" }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({ setTheme: async () => {} }) }));

import type { AppSettings, DeviceStatus } from "../lib/api";
import Settings from "./Settings.svelte";
import { updates } from "../lib/updates.svelte";
import { prefs } from "../lib/prefs.svelte";
import { readOnly } from "../lib/readOnly.svelte";

const settings = (autostart: boolean): AppSettings => ({
  start_hidden: false,
  autostart,
  check_updates: false,
  battery: { enabled: true, low_percent: 15, full: false },
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
  it("turns read-only mode on and shows the writes of this session", async () => {
    readOnly.on = false;
    api.setReadOnly.mockResolvedValue(true);
    api.writeLog.mockResolvedValue([
      { time: 0, device: "G502 HERO", kind: "HID++ 0x8100 function 7", data: "01 02", bytes: 2, result: "ok" },
      { time: 0, device: "G502 HERO", kind: "HID++ 0x2201 function 3", data: "00 03 20", bytes: 3, result: "refused (read-only mode)" },
    ]);
    render(Settings, { props: { status } });
    const box = screen.getByRole("checkbox", { name: /Read-only mode/ }) as HTMLInputElement;
    expect(box.checked).toBe(false);
    box.checked = true;
    await fireEvent.change(box);
    expect(api.setReadOnly).toHaveBeenCalledWith(true);
    await waitFor(() => expect(readOnly.on).toBe(true));
    expect(box.checked).toBe(true);

    const details = screen.getByText("Writes sent this session").closest("details")!;
    details.open = true;
    await fireEvent(details, new Event("toggle"));
    // Newest first.
    await waitFor(() => expect(details.querySelectorAll("li")).toHaveLength(2));
    expect(details.querySelector("li")!.textContent).toContain("refused (read-only mode)");
    readOnly.on = false;
  });

  it("puts the box back when read-only mode cannot be changed", async () => {
    readOnly.on = false;
    api.setReadOnly.mockRejectedValue("the settings could not be saved");
    render(Settings, { props: { status } });
    const box = screen.getByRole("checkbox", { name: /Read-only mode/ }) as HTMLInputElement;
    box.checked = true;
    await fireEvent.change(box);
    await waitFor(() => expect(screen.getByText("the settings could not be saved")).toBeTruthy());
    expect(box.checked).toBe(false);
  });

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

  it("saves the battery limit and the switches, and keeps the rest of the settings", async () => {
    api.setAppSettings.mockImplementation(async (s: AppSettings) => s);
    render(Settings, { props: { status } });
    const limit = (await screen.findByRole("combobox", { name: "Low below" })) as HTMLSelectElement;
    expect(limit.value).toBe("15");
    await fireEvent.change(limit, { target: { value: "30" } });
    await waitFor(() => expect(api.setAppSettings).toHaveBeenCalled());
    expect(api.setAppSettings.mock.calls[0][0].battery).toEqual({ enabled: true, low_percent: 30, full: false });
    expect(api.setAppSettings.mock.calls[0][0].autostart).toBe(true);

    const full = screen.getByRole("checkbox", { name: /fully charged/ });
    await fireEvent.click(full);
    await waitFor(() => expect(api.setAppSettings).toHaveBeenCalledTimes(2));
    expect(api.setAppSettings.mock.calls[1][0].battery).toEqual({ enabled: true, low_percent: 30, full: true });
  });

  it("turns the limit off with the battery switch, and shows a limit set in the file", async () => {
    api.appSettings.mockResolvedValue({ ...settings(true), battery: { enabled: false, low_percent: 12, full: false } });
    render(Settings, { props: { status } });
    const limit = (await screen.findByRole("combobox", { name: "Low below" })) as HTMLSelectElement;
    expect(limit.disabled).toBe(true);
    expect(limit.value).toBe("12");
    expect((screen.getByRole("checkbox", { name: /fully charged/ }) as HTMLInputElement).disabled).toBe(true);
  });

  it("puts the limit back when it cannot be saved", async () => {
    api.setAppSettings.mockRejectedValue("cannot save");
    render(Settings, { props: { status } });
    const limit = (await screen.findByRole("combobox", { name: "Low below" })) as HTMLSelectElement;
    await fireEvent.change(limit, { target: { value: "30" } });
    await waitFor(() => expect(limit.value).toBe("15"));
    expect(document.body.textContent).toContain("cannot save");
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

describe("Settings → Appearance", () => {
  beforeEach(() => {
    localStorage.clear();
    prefs.theme = "system";
    prefs.accent = "orange";
  });

  it("switches the theme and keeps the choice", async () => {
    render(Settings, { props: { status } });
    const light = screen.getByRole("button", { name: "Light" });
    expect(screen.getByRole("button", { name: "System" }).getAttribute("aria-pressed")).toBe("true");
    await fireEvent.click(light);
    expect(document.documentElement.dataset.theme).toBe("light");
    expect(light.getAttribute("aria-pressed")).toBe("true");
    expect(localStorage.getItem("perihub.theme")).toBe("light");
    await fireEvent.click(screen.getByRole("button", { name: "Dark" }));
    expect(document.documentElement.dataset.theme).toBe("dark");
  });

  it("offers the accents as labeled round swatches, and keeps the one chosen", async () => {
    render(Settings, { props: { status } });
    const names = screen.getAllByRole("radio").map((r) => r.getAttribute("aria-label"));
    expect(names).toEqual(["Orange", "Blue", "Green", "Purple", "Pink", "Teal"]);
    expect((screen.getByRole("radio", { name: "Orange" }) as HTMLInputElement).checked).toBe(true);
    await fireEvent.click(screen.getByRole("radio", { name: "Blue" }));
    expect(document.documentElement.dataset.accent).toBe("blue");
    expect(localStorage.getItem("perihub.accent")).toBe("blue");
    expect((screen.getByRole("radio", { name: "Blue" }) as HTMLInputElement).checked).toBe(true);
  });
});
