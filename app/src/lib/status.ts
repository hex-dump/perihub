// The state of a device or of its support, shown as an icon and a word by Status.svelte. The colors are the
// status colors of styles.css (--ok, --warn, --bad, --readonly, --experimental and --faint for idle), which
// are the same whatever the accent. Some accents are close to one of them (green and teal to "connected",
// orange and pink to errors), so the icon and the word say what the state is, and the color only helps.
import type { Level } from "./devices";

export type StatusKind = "ok" | "warn" | "bad" | "readonly" | "experimental" | "asleep" | "idle";

const circle = (r: number) => `M8 ${8 - r}a${r} ${r} 0 1 0 0 ${2 * r}a${r} ${r} 0 1 0 0-${2 * r}z`;

/** Each kind's icon: path data on a 16×16 grid, drawn with a stroke. */
export const STATUS_ICONS: Record<StatusKind, string[]> = {
  // A check in a circle: connected, working, supported.
  ok: [circle(6.25), "M5.4 8.2l1.8 1.8 3.4-3.7"],
  // A triangle with an exclamation mark: needs attention.
  warn: ["M8 2.3 14.2 13.2H1.8z", "M8 6.6v2.9", "M8 11.3h.01"],
  // A cross in a circle: failed.
  bad: [circle(6.25), "M5.9 5.9l4.2 4.2", "M10.1 5.9l-4.2 4.2"],
  // An eye: looked at, never changed.
  readonly: ["M1.6 8S4 3.6 8 3.6 14.4 8 14.4 8 12 12.4 8 12.4 1.6 8 1.6 8z", circle(1.9)],
  // A lab flask: changes at your own risk. Phosphor Icons "Flask", regular weight, unaltered (MIT license,
  // Copyright (c) 2023 Phosphor Icons, see THIRD-PARTY-NOTICES.md): drawn filled on its 256 grid (FILLED).
  experimental: [
    "M221.69,199.77,160,96.92V40h8a8,8,0,0,0,0-16H88a8,8,0,0,0,0,16h8V96.92L34.31,199.77A16,16,0,0,0,48,224H208a16,16,0,0,0,13.72-24.23ZM110.86,103.25A7.93,7.93,0,0,0,112,99.14V40h32V99.14a7.93,7.93,0,0,0,1.14,4.11L183.36,167c-12,2.37-29.07,1.37-51.75-10.11-15.91-8.05-31.05-12.32-45.22-12.81ZM48,208l28.54-47.58c14.25-1.74,30.31,1.85,47.82,10.72,19,9.61,35,12.88,48,12.88a69.89,69.89,0,0,0,19.55-2.7L208,208Z",
  ],
  // A moon: there, but asleep.
  asleep: ["M13.2 9.6A5.6 5.6 0 1 1 6.4 2.8a4.4 4.4 0 0 0 6.8 6.8z"],
  // An empty circle: not there, or not in use.
  idle: [circle(4.75)],
};

/** Icons copied from an icon set as they are: filled, on their own grid, instead of drawn with a stroke on 16×16. */
export const FILLED: Partial<Record<StatusKind, { viewBox: string }>> = {
  experimental: { viewBox: "0 0 256 256" },
};

/** How a support level of the device list is shown. */
export function levelStatus(level: Level): StatusKind {
  return level === "supported" ? "ok" : level === "experimental" ? "experimental" : level === "read_only" ? "readonly" : "idle";
}

/** How a support state said in words ("Supported", "Experimental", "Read-only", …) is shown. */
export function stateStatus(state: string): StatusKind {
  return /^supported/i.test(state) ? "ok" : /^experimental/i.test(state) ? "experimental" : /^read-only/i.test(state) ? "readonly" : "idle";
}
