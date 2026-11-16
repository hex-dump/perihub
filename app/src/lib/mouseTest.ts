// The maths of the mouse tester page. Nothing here talks to the device: it only looks at the events a
// webview delivers to the page.

/** Where the movement events come from, which decides if a polling rate can be trusted. */
export type PollingSource =
  /** pointerrawupdate: one event for each report, not tied to the screen refresh. */
  | "raw"
  /** pointermove with getCoalescedEvents(): the reports between two frames, each with its own time. */
  | "coalesced"
  /** pointermove only: at most one event for each frame of the screen. */
  | "frames";

/** What the window offers, to pick where the movement is read from. */
export function pollingSource(has: { raw: boolean; coalesced: boolean }): PollingSource {
  return has.raw ? "raw" : has.coalesced ? "coalesced" : "frames";
}

/** A pause in the movement ends a run: the gap is not part of the rate. */
export const MAX_GAP_MS = 50;
/** Samples are grouped in windows of this length to get min and max. */
export const WINDOW_MS = 250;
/** Fewest intervals in a window for its rate to count. */
const MIN_WINDOW_INTERVALS = 5;
/** Fewest intervals before any rate is shown. */
export const MIN_INTERVALS = 30;
/** More than this share of zero-length gaps means the times are batched, so they say nothing. */
const MAX_ZERO_SHARE = 0.2;
/** Without coalesced events, a rate this close to the screen refresh is the screen, not the mouse. */
const REFRESH_MARGIN = 1.25;

export type Verdict =
  /** The number is the mouse's polling rate. */
  | "ok"
  /** The webview limits what the page sees: no number is shown as the polling rate. */
  | "capped"
  /** Not enough movement yet. */
  | "waiting";

export interface PollingStats {
  verdict: Verdict;
  /** Why the number cannot be trusted, when the verdict is "capped". */
  reason: string | null;
  /** Rate of the last moments, in Hz. */
  live: number;
  average: number;
  min: number;
  max: number;
  /** Intervals measured. */
  samples: number;
}

export interface PollingOptions {
  source: PollingSource;
  /** Screen refresh in Hz, if known. */
  refreshHz?: number | null;
}

/** Collects the times of movement events (milliseconds) and works out their rate. */
export class PollingMeter {
  private last: number | null = null;
  private recent: number[] = [];
  private total = 0;
  private count = 0;
  private zero = 0;
  private windowStart: number | null = null;
  private windowSum = 0;
  private windowCount = 0;
  private min = Infinity;
  private max = 0;
  /** Most events seen in one dispatch (coalesced mode). */
  private burst = 1;

  constructor(private options: PollingOptions) {}

  setRefresh(hz: number | null) {
    this.options.refreshHz = hz;
  }

  /** Events handed to the page together, such as the coalesced events of one pointermove. */
  noteBurst(size: number) {
    this.burst = Math.max(this.burst, size);
  }

  /** One event, at `time` milliseconds. Times must not go back. */
  add(time: number) {
    const previous = this.last;
    this.last = time;
    if (previous === null) return;
    const gap = time - previous;
    if (gap < 0 || gap > MAX_GAP_MS) {
      this.windowStart = null;
      this.windowSum = 0;
      this.windowCount = 0;
      return;
    }
    this.count++;
    this.total += gap;
    if (gap === 0) this.zero++;
    this.recent.push(gap);
    if (this.recent.length > 100) this.recent.shift();

    if (this.windowStart === null) this.windowStart = previous;
    this.windowSum += gap;
    this.windowCount++;
    if (time - this.windowStart >= WINDOW_MS) {
      if (this.windowCount >= MIN_WINDOW_INTERVALS && this.windowSum > 0) {
        const rate = (this.windowCount * 1000) / this.windowSum;
        this.min = Math.min(this.min, rate);
        this.max = Math.max(this.max, rate);
      }
      this.windowStart = null;
      this.windowSum = 0;
      this.windowCount = 0;
    }
  }

  reset() {
    this.last = null;
    this.recent = [];
    this.total = 0;
    this.count = 0;
    this.zero = 0;
    this.windowStart = null;
    this.windowSum = 0;
    this.windowCount = 0;
    this.min = Infinity;
    this.max = 0;
    this.burst = 1;
  }

  stats(): PollingStats {
    const average = this.total > 0 ? (this.count * 1000) / this.total : 0;
    const recentSum = this.recent.reduce((a, b) => a + b, 0);
    const live = recentSum > 0 ? (this.recent.length * 1000) / recentSum : 0;
    const base = {
      live,
      average,
      min: Number.isFinite(this.min) ? this.min : 0,
      max: this.max,
      samples: this.count,
    };
    if (this.count < MIN_INTERVALS) return { ...base, verdict: "waiting", reason: null };
    const reason = this.cap(average);
    return { ...base, verdict: reason ? "capped" : "ok", reason };
  }

  /** Why the measurement cannot be trusted, or null. */
  private cap(average: number): string | null {
    const { source, refreshHz } = this.options;
    if (source === "frames") {
      return "This window reports the mouse once for each frame of the screen, not once for each report of the mouse.";
    }
    if (this.zero / this.count > MAX_ZERO_SHARE) {
      return "This window delivers the movement in batches, so the times of the single reports are lost.";
    }
    if (source === "coalesced" && this.burst <= 1 && refreshHz && average <= refreshHz * REFRESH_MARGIN) {
      return "This window does not give the single reports between two frames, so the number only shows the refresh rate of the screen.";
    }
    return null;
  }
}

/** Screen refresh rate from the gaps between animation frames (milliseconds), as the median. */
export function refreshRate(frameGaps: number[]): number | null {
  const gaps = frameGaps.filter((g) => g > 1 && g < 100).sort((a, b) => a - b);
  if (gaps.length < 10) return null;
  return 1000 / gaps[Math.floor(gaps.length / 2)];
}

/** Counts per inch from the counts reported over a known distance. */
export function countsPerInch(counts: number, distanceCm: number): number | null {
  if (!(counts > 0) || !(distanceCm > 0)) return null;
  return counts / (distanceCm / 2.54);
}

/** Pointer lock gives raw counts only when asked without the system's adjustments. */
export type DpiSource = "raw" | "pointer";

// --- Clicks ---

/** A press this soon after the one before is not a person: the switch bounced. */
export const CHATTER_MS = 50;
/** A press this soon after the one before is a double click, wanted or not. */
export const DOUBLE_MS = 300;

export type ClickKind = "single" | "double" | "chatter";

export interface ClickStats {
  presses: number;
  doubles: number;
  chatter: number;
  /** Shortest time between two presses, ms. */
  shortest: number | null;
  /** The last gap, ms. */
  last: number | null;
  lastKind: ClickKind;
}

export const BUTTON_NAMES = ["Left", "Middle", "Right", "Back", "Forward"] as const;

export function clickKind(gap: number | null): ClickKind {
  if (gap === null || gap > DOUBLE_MS) return "single";
  return gap < CHATTER_MS ? "chatter" : "double";
}

/** Presses of each mouse button and the gaps between them. */
export class ClickTracker {
  private lastTime = new Map<number, number>();
  private stats = new Map<number, ClickStats>();

  press(button: number, time: number): ClickKind {
    const before = this.lastTime.get(button);
    this.lastTime.set(button, time);
    const gap = before === undefined ? null : time - before;
    const kind = clickKind(gap);
    const s = this.stats.get(button) ?? {
      presses: 0,
      doubles: 0,
      chatter: 0,
      shortest: null,
      last: null,
      lastKind: "single" as ClickKind,
    };
    s.presses++;
    if (kind === "double") s.doubles++;
    if (kind === "chatter") s.chatter++;
    if (gap !== null && gap <= DOUBLE_MS) s.shortest = s.shortest === null ? gap : Math.min(s.shortest, gap);
    s.last = gap;
    s.lastKind = kind;
    this.stats.set(button, s);
    return kind;
  }

  get(button: number): ClickStats | null {
    const s = this.stats.get(button);
    return s ? { ...s } : null;
  }

  /** Buttons pressed so far, in order of number. */
  buttons(): number[] {
    return [...this.stats.keys()].sort((a, b) => a - b);
  }

  reset() {
    this.lastTime.clear();
    this.stats.clear();
  }
}
