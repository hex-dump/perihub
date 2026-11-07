import type { MouseRestore } from "./api";

/** What a backup was taken before, from its label. */
export function describeBackup(label: string): string {
  if (label === "restore") return "Before restoring a backup";
  if (label === "directory") return "Before enabling or disabling a profile";
  if (label === "keymap") return "Before changing the key mapping";
  const profile = /^profile(\d+)$/.exec(label);
  if (profile) return `Before saving profile ${profile[1]}`;
  const factory = /^factory(\d+)$/.exec(label);
  if (factory) return `Before resetting profile ${factory[1]} to factory settings`;
  return label;
}

function list(items: string[]): string {
  return items.length <= 1 ? items.join("") : `${items.slice(0, -1).join(", ")} and ${items[items.length - 1]}`;
}

/** The message shown after restoring a mouse backup. */
export function mouseRestoreMessage(restore: MouseRestore): string {
  const parts: string[] = [];
  if (restore.profiles.length === 1) parts.push(`profile ${restore.profiles[0]}`);
  else if (restore.profiles.length > 1) parts.push(`profiles ${list(restore.profiles.map(String))}`);
  if (restore.macros) parts.push("the macros");
  if (restore.directory) parts.push("the enabled profiles");
  if (!parts.length) return "The mouse already matches this backup: nothing was written.";
  return `Backup restored: ${list(parts)} changed.`;
}

/** The message shown after restoring a key mapping backup. */
export function keymapRestoreMessage(keys: number): string {
  if (keys === 0) return "The keyboard already matches this backup: nothing was written.";
  return `Backup restored: ${keys} ${keys === 1 ? "key" : "keys"} changed.`;
}
