import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const api = vi.hoisted(() => ({
  profiles: vi.fn(),
  setProfiles: vi.fn(),
  setupFocus: vi.fn(),
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
});
