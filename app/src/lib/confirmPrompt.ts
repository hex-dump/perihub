// A one-time prompt to send a test report for a device that is not confirmed on real hardware yet.
// Which devices count is decided here only, over the devices the "Test my device" wizard offers
// (reportWizard.ts): a device the core marks `tested` (confirmed.rs) is left out, and so is one that is
// supported already. The prompt's button starts the wizard for the device, the guided way to send it.
import type { DeviceStatus } from "./api";
import { type Candidate, candidates } from "./reportWizard";

export const PROMPTED_KEY = "perihub.confirmPrompted";

export interface Unconfirmed {
  /** Stored once the prompt was shown, so it never comes back. */
  key: string;
  /** The name the sidebar shows. */
  name: string;
  /** The device in the wizard's list: the wizard opens with it picked. */
  candidate: Candidate;
}

const hex = (id: number) => id.toString(16).padStart(4, "0");

/** The key a prompt is remembered by, or null for a device that is not asked about (confirmed, or not
 * known well enough: a receiver with nothing paired, a device PeriHub does not know yet). */
function promptKey(c: Candidate): string | null {
  const t = c.target;
  switch (t.type) {
    case "mouse":
      return t.mouse.verified ? null : `046d:${hex(t.mouse.product_id)}`;
    case "logitech":
      // The device's own id when it reports one, so it is asked about once however it is paired; else receiver and slot.
      return t.device.wireless_id == null
        ? `046d:${hex(t.receiver.product_id)}-${t.device.slot}`
        : `046d:w${hex(t.device.wireless_id)}`;
    case "corsair":
      return t.info.tested ? null : `1b1c:${hex(t.info.product_id)}`;
    case "peripheral":
      return t.info.tested ? null : t.info.key;
    default:
      return null;
  }
}

/** The connected devices nobody has confirmed on real hardware yet, in the wizard's order. Nothing
 * before the first scan is over: the lists are empty then, and would be a guess. */
export function unconfirmedDevices(status: DeviceStatus): Unconfirmed[] {
  if (!status.scanned) return [];
  return candidates(status, []).flatMap((candidate) => {
    const key = promptKey(candidate);
    return key === null ? [] : [{ key, name: candidate.name, candidate }];
  });
}

/** The keys already prompted, ever (empty when the storage is unavailable or holds something else). */
export function promptedKeys(): Set<string> {
  try {
    const saved: unknown = JSON.parse(localStorage.getItem(PROMPTED_KEY) ?? "[]");
    if (Array.isArray(saved)) return new Set(saved.filter((k): k is string => typeof k === "string"));
  } catch {
    /* storage unavailable or not JSON */
  }
  return new Set();
}

export function markPrompted(key: string) {
  try {
    localStorage.setItem(PROMPTED_KEY, JSON.stringify([...new Set([...promptedKeys(), key])]));
  } catch {
    /* storage unavailable */
  }
}

/** The first unconfirmed device not prompted yet. `seen` holds this session's keys, for when the storage cannot keep them. */
export function nextToPrompt(status: DeviceStatus, seen: ReadonlySet<string> = new Set()): Unconfirmed | null {
  const done = promptedKeys();
  return unconfirmedDevices(status).find((d) => !done.has(d.key) && !seen.has(d.key)) ?? null;
}
