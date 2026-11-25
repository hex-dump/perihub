import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const api = vi.hoisted(() => ({
  profiles: vi.fn(),
  setProfiles: vi.fn(),
  setupFocus: vi.fn(),
  installedGames: vi.fn(),
  onFocus: vi.fn(),
}));

vi.mock("../lib/api", async (original) => ({
  ...(await original<typeof import("../lib/api")>()),
  api,
}));

import type { DeviceStatus, FocusSupport, ProfilesInfo } from "../lib/api";
import AppProfiles from "./AppProfiles.svelte";

const status = { keyboard: null, mouse: null } as unknown as DeviceStatus;

function info(support: FocusSupport): ProfilesInfo {
  return { support, focused: null, recent: [], profiles: { enabled: false, rules: [] }, presets: [] };
}

const setup: FocusSupport = {
  kind: "setup",
  desktop: "GNOME",
  reason: "On GNOME, PeriHub needs its small GNOME Shell extension.",
  action: "Enable GNOME integration",
};

const flush = () => new Promise((r) => setTimeout(r, 0));

beforeEach(() => {
  vi.clearAllMocks();
  api.onFocus.mockResolvedValue(() => {});
  api.setProfiles.mockResolvedValue(undefined);
});

afterEach(cleanup);

describe("App profiles", () => {
  it("offers one button when a setup step is needed, then shows the rules", async () => {
    api.profiles.mockResolvedValueOnce(info(setup)).mockResolvedValueOnce(info({ kind: "supported", desktop: "GNOME" }));
    api.setupFocus.mockResolvedValue("GNOME integration is on.");
    render(AppProfiles, { status });
    await flush();
    expect(screen.getByText(setup.reason)).toBeTruthy();
    expect(screen.queryByText("Rules")).toBeNull();
    await fireEvent.click(screen.getByRole("button", { name: "Enable GNOME integration" }));
    await flush();
    expect(api.setupFocus).toHaveBeenCalledTimes(1);
    expect(screen.getByText("GNOME integration is on.")).toBeTruthy();
    expect(screen.getByText("Rules")).toBeTruthy();
  });

  it("says plainly when a sign-in is still needed", async () => {
    const signIn = "Almost done: log out and back in.";
    api.profiles
      .mockResolvedValueOnce(info(setup))
      .mockResolvedValueOnce(info({ kind: "unsupported", reason: "Log out and back in to finish." }));
    api.setupFocus.mockResolvedValue(signIn);
    render(AppProfiles, { status });
    await flush();
    await fireEvent.click(screen.getByRole("button", { name: "Enable GNOME integration" }));
    await flush();
    expect(screen.getByText(signIn)).toBeTruthy();
    expect(screen.queryByText("Log out and back in to finish.")).toBeNull();
    expect(screen.queryByRole("button", { name: "Enable GNOME integration" })).toBeNull();
  });

  it("shows the error when the setup fails", async () => {
    api.profiles.mockResolvedValue(info(setup));
    api.setupFocus.mockRejectedValue("GNOME Shell could not be reached.");
    render(AppProfiles, { status });
    await flush();
    await fireEvent.click(screen.getByRole("button", { name: "Enable GNOME integration" }));
    await flush();
    expect(screen.getByText("GNOME Shell could not be reached.")).toBeTruthy();
  });

  it("in the Flatpak, shows the command to run on the host instead of the button", async () => {
    const command =
      "gnome-extensions install --force ~/.var/app/io.github.hex_dump.perihub/perihub-focus@hex-dump.github.io.shell-extension.zip && gnome-extensions enable perihub-focus@hex-dump.github.io";
    const reason = "The Flatpak cannot install it by itself: run this command in a terminal, then log out and back in.";
    api.profiles.mockResolvedValue(info({ kind: "host_command", desktop: "GNOME", reason, command }));
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    render(AppProfiles, { status });
    await flush();
    expect(screen.getByText(reason)).toBeTruthy();
    expect(screen.getByText(command)).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Enable GNOME integration" })).toBeNull();
    expect(screen.queryByText("Rules")).toBeNull();
    await fireEvent.click(screen.getByRole("button", { name: "Copy command" }));
    await flush();
    expect(writeText).toHaveBeenCalledWith(command);
    expect(screen.getByRole("button", { name: "Copied" })).toBeTruthy();
    expect(api.setupFocus).not.toHaveBeenCalled();
  });

  it("says how to copy by hand when the clipboard refuses", async () => {
    api.profiles.mockResolvedValue(info({ kind: "host_command", desktop: "GNOME", reason: "Run this.", command: "gnome-extensions install x" }));
    Object.defineProperty(navigator, "clipboard", { value: { writeText: vi.fn().mockRejectedValue(new Error("denied")) }, configurable: true });
    render(AppProfiles, { status });
    await flush();
    await fireEvent.click(screen.getByRole("button", { name: "Copy command" }));
    await flush();
    expect(screen.getByText("The command could not be copied: select it and copy it by hand.")).toBeTruthy();
  });

  it("adds a rule from an installed game and warns about a guessed app name", async () => {
    api.profiles.mockResolvedValue(info({ kind: "supported", desktop: "KDE" }));
    api.installedGames.mockResolvedValue([
      { name: "Elden Ring", launcher: "Steam", app: "steam_app_1245620", certain: true, note: null },
      { name: "Super Game", launcher: "Lutris", app: "start.sh", certain: false, note: "Native games choose their own window class." },
    ]);
    render(AppProfiles, { status });
    await flush();
    await fireEvent.click(screen.getByRole("button", { name: "Add from installed games" }));
    await flush();
    expect(screen.getByText("Native games choose their own window class.")).toBeTruthy();
    await fireEvent.input(screen.getByLabelText("Search games"), { target: { value: "elden" } });
    expect(screen.queryByText("Super Game")).toBeNull();
    await fireEvent.click(screen.getByText("Elden Ring"));
    expect((screen.getByPlaceholderText("window class") as HTMLInputElement).value).toBe("steam_app_1245620");
    expect(screen.getByText("Rule added for Elden Ring.")).toBeTruthy();
    await fireEvent.click(screen.getByText("Elden Ring"));
    expect(screen.getByText("There is already a rule for steam_app_1245620.")).toBeTruthy();
    expect(screen.getAllByPlaceholderText("window class")).toHaveLength(1);
  });

  it("marks disabled mouse profiles unavailable and warns if a rule points to one", async () => {
    const g502 = {
      model: "G502 HERO Wireless",
      verified: true,
      bluetooth: null,
      onboard: { profile_count: 5 },
      enabled_profiles: [true, true, true, false, false],
    } as unknown as DeviceStatus["mouse"];
    api.profiles.mockResolvedValue({
      ...info({ kind: "supported", desktop: "Windows" }),
      profiles: { enabled: true, rules: [{ app: "game.exe", lighting: null, mouse_profile: 4 }] },
    });
    render(AppProfiles, { status: { ...status, mouse: g502 } });
    await flush();

    const options = screen.getAllByRole("option") as HTMLOptionElement[];
    expect(options.find((o) => o.textContent?.includes("Profile 3"))?.disabled).toBe(false);
    const p4 = options.find((o) => o.textContent?.includes("Profile 4 (disabled on mouse)"));
    expect(p4?.disabled).toBe(true);
    expect(screen.getByText(/Rules set to a disabled profile are skipped/)).toBeTruthy();
  });
});
