import { describe, expect, it } from "vitest";
import type { BlindSetting, BlindValue, EffectSpec, EffectValue } from "./api";
import {
  blindText,
  effectDraft,
  effectText,
  effectValue,
  makerSoftware,
  pickEffect,
  putBack,
  unknownBefore,
} from "./blind";

const reactiveSpeeds: [number, string][] = [
  [1, "500 ms"],
  [2, "1000 ms"],
  [4, "2000 ms"],
];
const effects: EffectSpec[] = [
  { name: "Off (no effect)", colors: 0, speeds: [] },
  { name: "Static, one colour", colors: 1, speeds: [] },
  { name: "Breathing, two colours", colors: 2, speeds: [] },
  { name: "Reactive, one colour", colors: 1, speeds: reactiveSpeeds },
];
const effect = (name: string, colors: string[], speed: number | null = null): BlindValue => ({
  type: "effect",
  value: { name, colors, speed },
});
const setting = (extra: Partial<BlindSetting>): BlindSetting => ({
  id: "effect",
  label: "Lighting effect",
  kind: { type: "effect", effects },
  last: null,
  ...extra,
});

describe("what was sent, in words", () => {
  it("names an effect with its colours and its speed", () => {
    expect(effectText({ name: "Off (no effect)", colors: [], speed: null })).toBe("Off (no effect)");
    expect(effectText({ name: "Breathing, two colours", colors: ["#ff0000", "#00ff00"], speed: null })).toBe(
      "Breathing, two colours (#ff0000, #00ff00)",
    );
    // The speed by the name the effect gives it, else by its number.
    expect(effectText({ name: "Reactive, one colour", colors: ["#ff0000"], speed: 2 }, effects)).toBe(
      "Reactive, one colour (#ff0000, 1000 ms)",
    );
    expect(effectText({ name: "Reactive, one colour", colors: ["#ff0000"], speed: 3 }, effects)).toBe(
      "Reactive, one colour (#ff0000, speed 3)",
    );
    expect(effectText({ name: "Reactive, one colour", colors: [], speed: 1 })).toBe("Reactive, one colour (speed 1)");
  });

  it("says 'not sent yet' for nothing and writes the other values plainly", () => {
    expect(blindText(null)).toBe("not sent yet");
    expect(blindText(undefined)).toBe("not sent yet");
    expect(blindText({ type: "color", value: "#123456" })).toBe("#123456");
    expect(blindText({ type: "dpi", value: [800, 1600] })).toBe("800, 1600 DPI");
    expect(blindText({ type: "choice", value: "Colour cycle" })).toBe("Colour cycle");
    expect(blindText({ type: "number", value: 500 })).toBe("500");
    const s = setting({});
    expect(blindText(effect("Reactive, one colour", ["#00ff00"], 4), s)).toBe("Reactive, one colour (#00ff00, 2000 ms)");
  });
});

describe("what can be put back", () => {
  it("is what was sent before the last change, only for a remembered setting", () => {
    const before = effect("Static, one colour", ["#00ff00"]);
    expect(putBack(setting({ remembered: true, last: effect("Off (no effect)", []), previous: before }))).toEqual(before);
    expect(putBack(setting({ remembered: true, last: before, previous: null }))).toBeNull();
    expect(putBack(setting({ remembered: true, last: before }))).toBeNull();
    expect(putBack(setting({ remembered: false, last: before, previous: before }))).toBeNull();
  });

  it("is unknown before the second change: the effect from before the first one was set by other software", () => {
    const sent = effect("Static, one colour", ["#00ff00"]);
    // Nothing sent yet, and after the first change: nothing to put back.
    expect(unknownBefore(setting({ remembered: true }))).toBe(true);
    expect(unknownBefore(setting({ remembered: true, last: sent, previous: null }))).toBe(true);
    // From the second change on there is something.
    expect(unknownBefore(setting({ remembered: true, last: sent, previous: sent }))).toBe(false);
    // A setting that is not remembered has nothing to say about it.
    expect(unknownBefore(setting({ remembered: false }))).toBe(false);
    expect(unknownBefore(setting({}))).toBe(false);
  });

  it("names the software that set what the device had", () => {
    expect(makerSoftware("Razer")).toBe("Razer Synapse");
    expect(makerSoftware("Logitech")).toBe("the maker's own software");
  });
});

describe("the effect form", () => {
  it("starts with the first effect, or with what PeriHub last sent", () => {
    expect(effectDraft(effects, null)).toEqual({ name: "Off (no effect)", colors: [], speed: null });
    expect(effectDraft(effects, effect("Reactive, one colour", ["#123456"], 4))).toEqual({
      name: "Reactive, one colour",
      colors: ["#123456"],
      speed: 4,
    });
    // What was last sent is no longer offered: the first effect.
    expect(effectDraft(effects, effect("Ripple", ["#123456"])).name).toBe("Off (no effect)");
    expect(effectDraft([], null)).toEqual({ name: "", colors: [], speed: null });
  });

  it("keeps the colours already chosen when another effect is picked, and the speed where it still fits", () => {
    const draft: EffectValue = { name: "Reactive, one colour", colors: ["#123456"], speed: 4 };
    // Two colours: the first stays, the second starts green.
    expect(pickEffect(effects, draft, "Breathing, two colours")).toEqual({
      name: "Breathing, two colours",
      colors: ["#123456", "#00ff00"],
      speed: null,
    });
    // No colour and no speed for Off.
    expect(pickEffect(effects, draft, "Off (no effect)")).toEqual({ name: "Off (no effect)", colors: [], speed: null });
    // A speed the next effect does not have becomes its first.
    const slow = pickEffect(effects, { ...draft, speed: 3 }, "Reactive, one colour");
    expect(slow.speed).toBe(1);
    expect(pickEffect(effects, draft, "Ripple")).toBe(draft);
  });

  it("sends only what the effect takes, and says what is missing", () => {
    expect(effectValue(effects, { name: "Static, one colour", colors: ["#00FF00"], speed: null })).toEqual({
      name: "Static, one colour",
      colors: ["#00FF00"],
      speed: null,
    });
    // A speed left over from another effect is dropped.
    expect(effectValue(effects, { name: "Static, one colour", colors: ["#00ff00"], speed: 2 })).toEqual({
      name: "Static, one colour",
      colors: ["#00ff00"],
      speed: null,
    });
    expect(effectValue(effects, { name: "Reactive, one colour", colors: ["#00ff00"], speed: 4 })).toEqual({
      name: "Reactive, one colour",
      colors: ["#00ff00"],
      speed: 4,
    });
    expect(effectValue(effects, { name: "Reactive, one colour", colors: ["#00ff00"], speed: 3 })).toBe(
      "Reactive, one colour: choose a speed",
    );
    expect(effectValue(effects, { name: "Reactive, one colour", colors: ["#00ff00"], speed: null })).toBe(
      "Reactive, one colour: choose a speed",
    );
    expect(effectValue(effects, { name: "Breathing, two colours", colors: ["#00ff00"], speed: null })).toBe(
      "Breathing, two colours: 2 colours needed",
    );
    expect(effectValue(effects, { name: "Static, one colour", colors: ["green"], speed: null })).toBe(
      "Static, one colour: one colour needed",
    );
    expect(effectValue(effects, { name: "Ripple", colors: [], speed: null })).toBe("Choose an effect");
  });
});
