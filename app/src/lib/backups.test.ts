import { describe, expect, it } from "vitest";
import { describeBackup, keymapRestoreMessage, mouseRestoreMessage } from "./backups";
import type { MouseState } from "./api";

const state = {} as MouseState;

describe("describeBackup", () => {
  it("names what each backup was taken before", () => {
    expect(describeBackup("profile3")).toBe("Before saving profile 3");
    expect(describeBackup("factory2")).toBe("Before resetting profile 2 to factory settings");
    expect(describeBackup("directory")).toBe("Before enabling or disabling a profile");
    expect(describeBackup("keymap")).toBe("Before changing the key mapping");
    expect(describeBackup("restore")).toBe("Before restoring a backup");
    expect(describeBackup("reload-probe")).toBe("reload-probe");
  });
});

describe("restore messages", () => {
  it("say what changed in the mouse", () => {
    expect(mouseRestoreMessage({ state, profiles: [4], macros: false, directory: false })).toBe(
      "Backup restored: profile 4 changed.",
    );
    expect(mouseRestoreMessage({ state, profiles: [2, 4, 5], macros: true, directory: true })).toBe(
      "Backup restored: profiles 2, 4 and 5, the macros and the enabled profiles changed.",
    );
    expect(mouseRestoreMessage({ state, profiles: [], macros: false, directory: false })).toBe(
      "The mouse already matches this backup: nothing was written.",
    );
  });

  it("count the keys that changed", () => {
    expect(keymapRestoreMessage(1)).toBe("Backup restored: 1 key changed.");
    expect(keymapRestoreMessage(3)).toBe("Backup restored: 3 keys changed.");
    expect(keymapRestoreMessage(0)).toBe("The keyboard already matches this backup: nothing was written.");
  });
});
