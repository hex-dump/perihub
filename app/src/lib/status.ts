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
  // A beaker: changes at your own risk.
  experimental: ["M3.5 2.5h9", "M4.6 2.5v9.6a1.7 1.7 0 0 0 1.7 1.7h3.4a1.7 1.7 0 0 0 1.7-1.7V2.5", "M4.6 8.4h6.8"],
  // A moon: there, but asleep.
  asleep: ["M13.2 9.6A5.6 5.6 0 1 1 6.4 2.8a4.4 4.4 0 0 0 6.8 6.8z"],
  // An empty circle: not there, or not in use.
  idle: [circle(4.75)],
};

/** How a support level of the device list is shown. */
export function levelStatus(level: Level): StatusKind {
  return level === "supported" ? "ok" : level === "experimental" ? "experimental" : level === "read_only" ? "readonly" : "idle";
}

/** How a support state said in words ("Supported", "Experimental", "Read-only", …) is shown. */
export function stateStatus(state: string): StatusKind {
  return /^supported/i.test(state) ? "ok" : /^experimental/i.test(state) ? "experimental" : /^read-only/i.test(state) ? "readonly" : "idle";
}
