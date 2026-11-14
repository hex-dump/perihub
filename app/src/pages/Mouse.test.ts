import { cleanup, fireEvent, render, screen, within } from "@testing-library/svelte";
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
  keepChanges: vi.fn(),
  undoLastWrite: vi.fn(),
  unifyingAccess: vi.fn(),
  grantUnifyingAccess: vi.fn(),
  revokeUnifyingAccess: vi.fn(),
}));

vi.mock("../lib/api", async (original) => ({ ...(await original<typeof import("../lib/api")>()), api }));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: async () => "0.5.2" }));

import type { MouseInfo, MouseState, ProfileSettings } from "../lib/api";
import { acceptNoWarranty } from "../lib/noWarranty";
import Mouse from "./Mouse.svelte";

const info: MouseInfo = {
  product_id: 0xc08b,
  model: "G502 HERO",
  name: "G502 HERO Gaming Mouse",
  verified: true,
  experimental: false,
  pending_undo: false,
  receiver: false,
  link: null,
  host_mode: false,
  firmware: [{ kind: 0, name: "U1", version: "27.2.1" }],
  dpi: { min: 100, max: 25600, step: 50, list: [] },
  report_rates_ms: [1, 2, 4, 8],
  led_zones: [],
  features: [],
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
  writable_format: true,
  switches_back: true,
  profile_list: "own",
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
  profile_list: "own",
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
  api.keepChanges.mockResolvedValue(undefined);
  api.undoLastWrite.mockImplementation(async () => ({ state: state(), profiles: [1], macros: false, directory: false }));
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
    const page = await open({ ...info, verified: false, writable: false, writable_format: false });
    await wait(0);
    expect(screen.getByRole("button", { name: "Copy test report" })).toBeTruthy();
    page.unmount();

    await open();
    expect(screen.queryByRole("button", { name: "Copy test report" })).toBeNull();
  });
});

describe("a device on a receiver that PeriHub does not support", () => {
  const bolt = {
    product_id: 0xc548,
    name: "Logi Bolt receiver",
    collections: ["interface 2, usage page ff00, usage 0002"],
    bolt: true,
    unifying: false,
    reachable: true,
    denied: false,
    open_slots: [1],
    slots: ["slot 1: HID++ 4.5"],
    unsupported: [
      {
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
      },
    ],
  };
  const note = "Logi Bolt receiver c548, slot 1: MX Master 3S is not supported yet";
  const access = { rule_installed: false, supported: true, manual_install: "install", manual_remove: "remove" };

  beforeEach(() => {
    api.unifyingAccess.mockResolvedValue(access);
    api.grantUnifyingAccess.mockResolvedValue(undefined);
    api.revokeUnifyingAccess.mockResolvedValue(undefined);
  });

  it("is pointed to its own page instead of an error", async () => {
    render(Mouse, { props: { info: null, error: note, asleep: null, receivers: [bolt] } });
    await wait(0);
    expect(screen.getByText("MX Master 3S is paired with your Logi Bolt receiver.")).toBeTruthy();
    expect(screen.getByText(/Its page in the sidebar shows what it can do/)).toBeTruthy();
    expect(screen.queryByText(note)).toBeNull();
  });

  it("explains why a Unifying receiver stays closed on Linux", async () => {
    const unifying = {
      ...bolt,
      product_id: 0xc52b,
      name: "Unifying receiver",
      bolt: false,
      unifying: true,
      denied: true,
      slots: ["could not ask: Permission denied"],
      unsupported: [],
    };
    // The core no longer reports the refusal as a connection error: only another mouse's error can be there.
    render(Mouse, { props: { info: null, error: null, asleep: null, receivers: [unifying] } });
    await wait(0);
    expect(screen.queryByText(/install the PeriHub udev rule/)).toBeNull();
    expect(screen.getByText("PeriHub cannot open your Unifying receiver.")).toBeTruthy();
    expect(screen.getByText(/every key you press on the keyboards paired with this receiver/)).toBeTruthy();

    // The rule is installed only after the box is ticked.
    const allow = screen.getByRole("button", { name: "Allow PeriHub to open the receiver" }) as HTMLButtonElement;
    expect(allow.disabled).toBe(true);
    await fireEvent.click(screen.getByRole("checkbox", { name: /I understand that every program/ }));
    await fireEvent.click(allow);
    await wait(0);
    expect(api.grantUnifyingAccess).toHaveBeenCalledOnce();
    expect(screen.getByText(/Permission given/)).toBeTruthy();
  });

  it("offers to remove the rule once PeriHub installed it", async () => {
    api.unifyingAccess.mockResolvedValue({ ...access, rule_installed: true });
    const open = {
      ...bolt,
      product_id: 0xc52b,
      name: "Unifying receiver",
      bolt: false,
      unifying: true,
      unsupported: [],
    };
    render(Mouse, { props: { info: null, error: null, asleep: null, receivers: [open] } });
    await wait(0);
    await fireEvent.click(screen.getByRole("button", { name: "Remove permission" }));
    await wait(0);
    expect(api.revokeUnifyingAccess).toHaveBeenCalledOnce();
    expect(screen.getByText(/Permission removed/)).toBeTruthy();
  });

  it("still offers a report when the Bolt receiver is reached but nothing answered", async () => {
    render(Mouse, {
      props: { info: null, error: null, asleep: null, receivers: [{ ...bolt, slots: [], unsupported: [] }] },
    });
    await wait(0);
    expect(screen.getByText(/nothing paired with it has answered/)).toBeTruthy();
    expect(screen.getByRole("button", { name: "Open test form" })).toBeTruthy();
  });

  // Issue #4: a G502 on a POWERPLAY receiver, and PeriHub showed nothing at all.
  it("offers a report when a LIGHTSPEED receiver is reached but nothing answered", async () => {
    const lightspeed = {
      ...bolt,
      product_id: 0xc53a,
      name: "LIGHTSPEED receiver",
      bolt: false,
      slots: ["slot 1: no answer", "slot 2: no answer"],
      unsupported: [],
    };
    render(Mouse, { props: { info: null, error: null, asleep: null, receivers: [lightspeed] } });
    await wait(0);
    expect(screen.getByText(/PeriHub sees your LIGHTSPEED receiver, but nothing paired with it has answered/)).toBeTruthy();
    expect(screen.getByRole("button", { name: "Open test form" })).toBeTruthy();
  });

  it("does not say nothing answered when a mouse answered but could not be opened", async () => {
    const lightspeed = { ...bolt, product_id: 0xc539, name: "LIGHTSPEED receiver", bolt: false, unsupported: [] };
    const error = "LIGHTSPEED receiver c539: onboard profile memory uses an unknown format";
    render(Mouse, { props: { info: null, error, asleep: null, receivers: [lightspeed] } });
    await wait(0);
    expect(screen.getByText(error)).toBeTruthy();
    expect(screen.queryByText(/nothing paired with it has answered/)).toBeNull();
    expect(screen.getByRole("button", { name: "Open test form" })).toBeTruthy();
  });

  it("only says the mouse is asleep when it is", async () => {
    const lightspeed = { ...bolt, product_id: 0xc539, name: "LIGHTSPEED receiver", bolt: false, unsupported: [] };
    render(Mouse, { props: { info: null, error: null, asleep: "G903 HERO", receivers: [lightspeed] } });
    await wait(0);
    expect(screen.getByText(/The G903 HERO is turned off or asleep/)).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Open test form" })).toBeNull();
  });

  it("is not shown while a supported mouse is open", async () => {
    render(Mouse, { props: { info, error: null, asleep: null, receivers: [bolt] } });
    await wait(0);
    expect(screen.queryByText(/is paired with your/)).toBeNull();
  });
});

describe("a new device status", () => {
  it("keeps unsaved changes when it is about another device", async () => {
    const page = await open();
    await changeFirstLevel(1000);
    expect(api.mouseState).toHaveBeenCalledOnce();

    // The keyboard was plugged in, or another program started: the same mouse, in a new status.
    await page.rerender({ info: structuredClone(info), error: null, asleep: null });
    await wait(0);
    expect(api.mouseState).toHaveBeenCalledOnce();
    expect(firstLevel().value).toBe("1000");
  });

  it("reads the mouse again when the mouse itself was opened again", async () => {
    const page = await open({ ...info, verified: false, experimental: true });
    await page.rerender({ info: { ...info, verified: false, experimental: false, writable: false }, error: null, asleep: null });
    await wait(0);
    expect(api.mouseState).toHaveBeenCalledTimes(2);

    // Unplugged and plugged back in.
    await page.rerender({ info: null, error: null, asleep: null });
    await page.rerender({ info: structuredClone(info), error: null, asleep: null });
    await wait(0);
    expect(api.mouseState).toHaveBeenCalledTimes(3);
  });
});

describe("enabling and disabling profiles", () => {
  it("puts the box back when the mouse refuses the change", async () => {
    api.setProfileEnabled.mockRejectedValue("at least one profile must stay enabled");
    await open();
    const box = (screen.getAllByRole("checkbox", { name: "Enabled" }) as HTMLInputElement[])[1];
    expect(box.checked).toBe(true);
    await fireEvent.click(box);
    await wait(0);
    expect(api.setProfileEnabled).toHaveBeenCalledWith(2, false);
    expect(screen.getByText(/at least one profile must stay enabled/)).toBeTruthy();
    expect(box.checked).toBe(true);
  });

  it("shows the new state when the mouse takes it", async () => {
    api.setProfileEnabled.mockImplementation(async () => {
      const next = state();
      next.profiles[1].enabled = false;
      return next;
    });
    await open();
    const box = (screen.getAllByRole("checkbox", { name: "Enabled" }) as HTMLInputElement[])[1];
    await fireEvent.click(box);
    await wait(0);
    expect((screen.getAllByRole("checkbox", { name: "Enabled" }) as HTMLInputElement[])[1].checked).toBe(false);
  });
});

describe("a mouse nothing was saved to", () => {
  const blank = async (): Promise<MouseState> => ({ ...state(), profile_list: "factory" });

  it("says that the factory profiles are shown and that the first save creates the list", async () => {
    api.mouseState.mockImplementation(blank);
    await open();
    expect(screen.getByText(/Nothing has been saved to this mouse yet/)).toBeTruthy();
    expect(screen.getByText(/Your first save also creates the mouse's own profile list/)).toBeTruthy();
  });

  it("tells the owner of an untested mouse how to get a profile list first", async () => {
    api.mouseState.mockImplementation(blank);
    await open({ ...info, verified: false, experimental: true });
    expect(screen.getByText(/save a profile once with Logitech G HUB/)).toBeTruthy();
  });

  it("says that a damaged profile list is not saved over", async () => {
    api.mouseState.mockImplementation(async (): Promise<MouseState> => ({ ...state(), profile_list: "damaged" }));
    await open();
    expect(screen.getByText(/The profile list in this mouse's memory is damaged/)).toBeTruthy();
    expect(screen.queryByText(/Nothing has been saved to this mouse yet/)).toBeNull();
  });

  it("says nothing for a mouse with its own profile list", async () => {
    await open();
    expect(screen.queryByText(/Nothing has been saved to this mouse yet/)).toBeNull();
  });
});

describe("a memory format nobody has tested", () => {
  const onboard = { ...info.onboard, sector_size: 255 };
  const experimental = {
    ...info,
    model: "G403 HERO",
    product_id: 0xc08f,
    onboard,
    verified: false,
    experimental: true,
    writable: true,
  };

  /** A mouse nobody has saved to with PeriHub yet, so that the "No warranty" box was never ticked. */
  const firstTime = (productId: number) => ({ ...experimental, model: "G402", product_id: productId });

  async function clickSave() {
    await fireEvent.click(screen.getByRole("button", { name: "Save to mouse" }));
    await wait(0);
  }

  it("can be edited straight away, with a notice that asks for feedback", async () => {
    await open(experimental);
    expect(firstLevel().matches(":disabled")).toBe(false);
    expect(screen.queryByRole("button", { name: "Edit anyway…" })).toBeNull();
    expect(screen.getByText(/Experimental: not tested yet/)).toBeTruthy();
    expect(screen.getByText(/profile\s+format 2, 255-byte sectors/)).toBeTruthy();
    expect(screen.getByRole("button", { name: "Copy test report" })).toBeTruthy();
  });

  it("asks for the No warranty box before the first save only", async () => {
    await open(firstTime(0xc07e));
    await changeFirstLevel(1000);
    await clickSave();
    const dialog = screen.getByRole("dialog");
    expect(dialog.textContent).toContain("Save to the G402?");
    expect(screen.getByRole("note").textContent).toContain("accepts no responsibility for any damage");
    expect(api.saveProfile).not.toHaveBeenCalled();
    const confirm = within(dialog).getByRole("button", { name: "Save to mouse" }) as HTMLButtonElement;
    expect(confirm.disabled).toBe(true);
    await fireEvent.click(screen.getByRole("checkbox", { name: /I have read this/ }));
    expect(confirm.disabled).toBe(false);
    await fireEvent.click(confirm);
    await wait(0);
    expect(api.saveProfile).toHaveBeenCalledOnce();
    expect(screen.queryByRole("dialog")).toBeNull();
    await fireEvent.click(screen.getByRole("button", { name: "Keep changes" }));

    // The second save goes straight to the mouse.
    await changeFirstLevel(1200);
    await clickSave();
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(api.saveProfile).toHaveBeenCalledTimes(2);
  });

  it("saves nothing when the box is cancelled", async () => {
    await open(firstTime(0xc07f));
    await changeFirstLevel(1000);
    await clickSave();
    await fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await wait(0);
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(api.saveProfile).not.toHaveBeenCalled();
  });

  it("asks before a factory reset too", async () => {
    await open(firstTime(0xc080));
    await fireEvent.click(screen.getByRole("button", { name: "Factory settings" }));
    await fireEvent.click(screen.getByRole("button", { name: "Reset this profile?" }));
    await wait(0);
    const dialog = screen.getByRole("dialog");
    expect(within(dialog).getByRole("button", { name: "Reset this profile" })).toBeTruthy();
    expect(api.restoreFactory).not.toHaveBeenCalled();
  });

  it("does not enable or disable profiles", async () => {
    await open(experimental);
    const enabled = screen.getAllByRole("checkbox", { name: "Enabled" }) as HTMLInputElement[];
    expect(enabled.every((c) => c.disabled)).toBe(true);
  });

  describe("after the No warranty box", () => {
    beforeEach(() => acceptNoWarranty(experimental.product_id));

    it("asks to keep a save, and undoes it after 15 seconds without an answer", async () => {
      await open(experimental);
      await changeFirstLevel(1000);
      await clickSave();
      expect(screen.getByRole("alertdialog").textContent).toContain("Keep these changes?");
      await wait(15_000);
      expect(api.undoLastWrite).toHaveBeenCalledOnce();
      expect(screen.queryByRole("alertdialog")).toBeNull();
      expect(screen.getByText("Changes undone: the backup is back on the mouse.")).toBeTruthy();
    });

    it("keeps a save when asked", async () => {
      await open(experimental);
      await changeFirstLevel(1000);
      await clickSave();
      await fireEvent.click(screen.getByRole("button", { name: "Keep changes" }));
      await wait(20_000);
      expect(api.keepChanges).toHaveBeenCalledOnce();
      expect(api.undoLastWrite).not.toHaveBeenCalled();
    });
  });

  it("is read-only through a receiver, and says to use the cable", async () => {
    await open({ ...experimental, experimental: false, writable: false, receiver: true });
    expect(screen.getByText(/Read-only through the receiver/)).toBeTruthy();
    expect(firstLevel().matches(":disabled")).toBe(true);
  });

  it("stays read-only when PeriHub cannot write the format", async () => {
    await open({
      ...experimental,
      experimental: false,
      writable: false,
      writable_format: false,
      onboard: { ...onboard, profile_format: 4 },
    });
    expect(screen.getByText(/memory format PeriHub cannot write yet/)).toBeTruthy();
    expect(firstLevel().matches(":disabled")).toBe(true);
    expect((screen.getByRole("button", { name: "Save to mouse" }) as HTMLButtonElement).disabled).toBe(true);
  });
});

describe("another program in control of the mouse", () => {
  it("is said to give the mouse back only on a model tested as a whole", async () => {
    const page = await open({ ...info, host_mode: true });
    expect(screen.getByText(/PeriHub gives the mouse back its profiles within a few\s+seconds/)).toBeTruthy();
    page.unmount();

    // Another model with the G502 HERO's memory format: tested, but PeriHub leaves its mode alone.
    await open({ ...info, model: "G403", product_id: 0xc083, host_mode: true, switches_back: false });
    expect(screen.queryByText(/gives the mouse back/)).toBeNull();
    expect(screen.getByText(/PeriHub does not take the mouse back by itself/)).toBeTruthy();
  });

  it("is also said on a mouse with experimental saves", async () => {
    await open({ ...info, verified: false, experimental: true, switches_back: false, host_mode: true });
    expect(screen.getByText(/what you save may seem to do nothing/)).toBeTruthy();
  });
});

describe("a tested mouse", () => {
  it("saves without asking anything", async () => {
    await open();
    await changeFirstLevel(1000);
    await fireEvent.click(screen.getByRole("button", { name: "Save to mouse" }));
    await wait(0);
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(api.saveProfile).toHaveBeenCalledOnce();
  });

  it("switches profiles through its receiver but does not save there", async () => {
    await open({ ...info, receiver: true, writable: false });
    expect(screen.getByText(/it does not save: a write over the\s+radio has never been tried/)).toBeTruthy();
    expect(firstLevel().matches(":disabled")).toBe(false);
    expect((screen.getByRole("button", { name: "Save to mouse" }) as HTMLButtonElement).disabled).toBe(true);
    // Issue #4: the tester was asked for a report from this page.
    expect(screen.getByRole("button", { name: "Open test form" })).toBeTruthy();
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

describe("Lighting", () => {
  const RED = { mode: "fixed", color: "#ff0000" } as const;
  const BLUE = { mode: "fixed", color: "#0000ff" } as const;
  const ALL_EFFECTS = [0, 1, 10, 3];
  // The G502 HERO: zone 0 is the DPI indicator, zone 1 the G logo.
  const zones = [
    { index: 0, location: 1, effects: ALL_EFFECTS },
    { index: 1, location: 2, effects: ALL_EFFECTS },
  ];

  async function openLighting(leds: ProfileSettings["leds"], ledZones = zones) {
    api.mouseState.mockImplementation(async () => {
      const s = state();
      for (const p of s.profiles) p.settings.leds = structuredClone(leds);
      return s;
    });
    render(Mouse, { props: { info: { ...info, led_zones: ledZones }, error: null, asleep: null, initialTab: "lighting" } });
    await wait(0);
  }

  const syncBox = () => screen.getByRole("checkbox", { name: /Same effect on all zones/ }) as HTMLInputElement;
  const cardOf = (title: string) => screen.getByRole("heading", { name: title }).closest(".card") as HTMLElement;
  const previews = () => api.previewLed.mock.calls.map(([zone, effect]) => [zone, effect.mode]);

  beforeEach(() => api.previewLed.mockResolvedValue(undefined));

  it("opens in sync when every zone shows the same effect, and out of sync otherwise", async () => {
    await openLighting([RED, RED]);
    expect(syncBox().checked).toBe(true);
    expect(screen.getByRole("heading", { name: "All zones" })).toBeTruthy();
    cleanup();

    await openLighting([RED, BLUE]);
    expect(syncBox().checked).toBe(false);
    expect(screen.getByRole("heading", { name: "DPI indicator" })).toBeTruthy();
    expect(screen.getByRole("heading", { name: "G logo" })).toBeTruthy();
  });

  it("out of sync, breathing on one zone leaves the other zone's color alone", async () => {
    await openLighting([RED, BLUE]);
    await fireEvent.click(within(cardOf("G logo")).getByRole("button", { name: "Breathing" }));
    await wait(PREVIEW_WAIT);
    expect(previews()).toEqual([[1, "breathing"]]);
    expect(within(cardOf("DPI indicator")).getByRole("button", { name: "Fixed" }).classList).toContain("active");
  });

  it("in sync, one change goes to every zone", async () => {
    await openLighting([RED, RED]);
    await fireEvent.click(within(cardOf("All zones")).getByRole("button", { name: "Color cycle" }));
    await wait(PREVIEW_WAIT);
    expect(previews()).toEqual([[0, "cycle"], [1, "cycle"]]);
  });

  it("turning sync on gives every zone the first zone's effect, restarted together", async () => {
    const green = { mode: "breathing", color: "#00ff00", period_ms: 3000, brightness: 80 } as const;
    await openLighting([green, BLUE]);
    await fireEvent.click(syncBox());
    await wait(PREVIEW_WAIT);
    expect(api.previewLed.mock.calls).toEqual([[0, green], [1, green]]);

    api.previewLed.mockClear();
    await fireEvent.click(syncBox());
    await wait(PREVIEW_WAIT);
    expect(api.previewLed).not.toHaveBeenCalled();
  });

  it("in sync, offers only the effects every zone can show", async () => {
    await openLighting([RED, RED], [zones[0], { index: 1, location: 2, effects: [0, 1, 3] }]);
    const card = cardOf("All zones");
    expect(within(card).queryByRole("button", { name: "Breathing" })).toBeNull();
    expect(within(card).getByRole("button", { name: "Color cycle" })).toBeTruthy();
  });

  it("turning sync on skips an effect another program set", async () => {
    await openLighting([{ mode: "raw", bytes: [0x0b, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0] }, BLUE]);
    await fireEvent.click(syncBox());
    await wait(PREVIEW_WAIT);
    expect(api.previewLed.mock.calls).toEqual([[0, BLUE], [1, BLUE]]);
  });

  it("sends two quick changes on different zones, not only the last one", async () => {
    await openLighting([RED, BLUE]);
    await fireEvent.click(within(cardOf("DPI indicator")).getByRole("button", { name: "Off" }));
    await fireEvent.click(within(cardOf("G logo")).getByRole("button", { name: "Color cycle" }));
    await wait(PREVIEW_WAIT);
    expect(previews()).toEqual([[0, "off"], [1, "cycle"]]);
  });
});
