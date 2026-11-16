import { describe, expect, it } from "vitest";
import { ClickTracker, PollingMeter, clickKind, countsPerInch, pollingSource, refreshRate } from "./mouseTest";

function feed(m: PollingMeter, hz: number, seconds: number, burstEvery = 1) {
  const step = 1000 / hz;
  for (let i = 0; i < hz * seconds; i++) m.add(1000 + i * step);
  void burstEvery;
}

describe("PollingMeter", () => {
  it("waits for enough movement", () => {
    const m = new PollingMeter({ source: "raw" });
    m.add(0);
    m.add(1);
    expect(m.stats().verdict).toBe("waiting");
  });

  it("measures 1000 Hz from raw events", () => {
    const m = new PollingMeter({ source: "raw" });
    feed(m, 1000, 1);
    const s = m.stats();
    expect(s.verdict).toBe("ok");
    expect(s.average).toBeCloseTo(1000, 0);
    expect(s.live).toBeCloseTo(1000, 0);
    expect(s.min).toBeCloseTo(1000, 0);
    expect(s.max).toBeCloseTo(1000, 0);
  });

  it("ignores pauses", () => {
    const m = new PollingMeter({ source: "raw" });
    feed(m, 125, 1);
    m.add(5000);
    m.add(5008);
    expect(m.stats().average).toBeCloseTo(125, 0);
  });

  it("refuses a number when only frames are reported", () => {
    const m = new PollingMeter({ source: "frames" });
    feed(m, 60, 2);
    const s = m.stats();
    expect(s.verdict).toBe("capped");
    expect(s.reason).toMatch(/frame/);
  });

  it("refuses batched times", () => {
    const m = new PollingMeter({ source: "raw" });
    for (let i = 0; i < 40; i++) {
      m.add(i * 16);
      m.add(i * 16);
    }
    expect(m.stats().verdict).toBe("capped");
  });

  it("refuses a coalesced rate that equals the screen refresh", () => {
    const m = new PollingMeter({ source: "coalesced", refreshHz: 60 });
    feed(m, 60, 2);
    expect(m.stats().verdict).toBe("capped");
  });

  it("trusts coalesced events that came in bursts", () => {
    const m = new PollingMeter({ source: "coalesced", refreshHz: 60 });
    m.noteBurst(16);
    feed(m, 1000, 1);
    expect(m.stats().verdict).toBe("ok");
  });

  it("starts again after reset", () => {
    const m = new PollingMeter({ source: "raw" });
    feed(m, 500, 1);
    m.reset();
    expect(m.stats().samples).toBe(0);
  });
});

describe("pollingSource", () => {
  it("prefers raw events, then coalesced ones", () => {
    expect(pollingSource({ raw: true, coalesced: true })).toBe("raw");
    expect(pollingSource({ raw: false, coalesced: true })).toBe("coalesced");
    expect(pollingSource({ raw: false, coalesced: false })).toBe("frames");
  });
});

describe("refreshRate", () => {
  it("takes the median gap", () => {
    expect(refreshRate(Array(20).fill(1000 / 144))).toBeCloseTo(144, 0);
    expect(refreshRate([16])).toBeNull();
  });
});

describe("countsPerInch", () => {
  it("scales the distance to an inch", () => {
    expect(countsPerInch(31500, 10)).toBeCloseTo(8001, 0);
    expect(countsPerInch(800, 2.54)).toBeCloseTo(800, 5);
  });
  it("needs real numbers", () => {
    expect(countsPerInch(0, 10)).toBeNull();
    expect(countsPerInch(100, 0)).toBeNull();
    expect(countsPerInch(100, NaN)).toBeNull();
  });
});

describe("clicks", () => {
  it("sorts the gaps", () => {
    expect(clickKind(null)).toBe("single");
    expect(clickKind(20)).toBe("chatter");
    expect(clickKind(120)).toBe("double");
    expect(clickKind(900)).toBe("single");
  });

  it("counts for each button", () => {
    const t = new ClickTracker();
    t.press(0, 0);
    t.press(0, 25);
    t.press(0, 200);
    t.press(0, 2000);
    t.press(2, 10);
    expect(t.get(0)).toMatchObject({ presses: 4, chatter: 1, doubles: 1, shortest: 25, last: 1800, lastKind: "single" });
    expect(t.get(2)).toMatchObject({ presses: 1, chatter: 0, shortest: null });
    expect(t.buttons()).toEqual([0, 2]);
    expect(t.get(1)).toBeNull();
    t.reset();
    expect(t.buttons()).toEqual([]);
  });
});
