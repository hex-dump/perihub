// Settings of a device that cannot report them (BlindSetting): how they read, and the form of a lighting effect.
// A setting the device keeps in its memory is "remembered": PeriHub keeps what it sent itself, so that the one before
// the last can be sent again ("Put back"). What the device held before PeriHub's first change was set by other
// software and cannot be read, so it is never offered back.

import type { BlindSetting, BlindValue, EffectSpec, EffectValue } from "./api";

/** The colours a new effect form starts with: red, then green. */
export const START_COLORS = ["#ff0000", "#00ff00"];

/** The software that sets what a device of this brand holds before PeriHub's first change. */
export function makerSoftware(brand: string): string {
  return brand === "Razer" ? "Razer Synapse" : "the maker's own software";
}

/** An effect in words: its name, its colours and its speed, by the name the effect gives it. */
export function effectText(value: EffectValue, effects: EffectSpec[] = []): string {
  const speeds = effects.find((e) => e.name === value.name)?.speeds ?? [];
  const speed = value.speed === null ? null : (speeds.find(([n]) => n === value.speed)?.[1] ?? `speed ${value.speed}`);
  const detail = [...value.colors, ...(speed === null ? [] : [speed])].join(", ");
  return detail ? `${value.name} (${detail})` : value.name;
}

/** What was sent, in words; "not sent yet" for nothing. */
export function blindText(value: BlindValue | null | undefined, setting?: BlindSetting): string {
  if (!value) return "not sent yet";
  switch (value.type) {
    case "dpi":
      return `${value.value.join(", ")} DPI`;
    case "effect":
      return effectText(value.value, setting?.kind.type === "effect" ? setting.kind.effects : []);
    default:
      return `${value.value}`;
  }
}

/** What "Put back" would send: the setting as PeriHub sent it before its last change. Nothing for a setting that is not
 * remembered, or before PeriHub's second change. */
export function putBack(setting: BlindSetting): BlindValue | null {
  return setting.remembered ? (setting.previous ?? null) : null;
}

/** PeriHub knows nothing from before: the setting the device had before PeriHub's first change was set by other
 * software, which PeriHub cannot read, so it cannot put it back. True for a remembered setting until its second
 * change, and again once everything PeriHub sent has been put back. */
export function unknownBefore(setting: BlindSetting): boolean {
  return !!setting.remembered && putBack(setting) === null;
}

function specOf(effects: EffectSpec[], name: string): EffectSpec | undefined {
  return effects.find((e) => e.name === name);
}

/** The first colours of the form: the ones PeriHub last sent when they fit, else the starting ones. */
function colorsFor(spec: EffectSpec, from: string[]): string[] {
  return Array.from({ length: spec.colors }, (_, i) => from[i] ?? START_COLORS[i] ?? "#ffffff");
}

/** The speed the effect takes that is nearest to the one wanted: the same number when it takes it, else its first. */
function speedFor(spec: EffectSpec, wanted: number | null): number | null {
  if (spec.speeds.length === 0) return null;
  return spec.speeds.some(([n]) => n === wanted) ? wanted : spec.speeds[0][0];
}

/** The effect form as it first shows: what PeriHub last sent, else the first effect. */
export function effectDraft(effects: EffectSpec[], last: BlindValue | null | undefined): EffectValue {
  const sent = last?.type === "effect" ? last.value : null;
  const spec = (sent && specOf(effects, sent.name)) || effects[0];
  if (!spec) return { name: "", colors: [], speed: null };
  return { name: spec.name, colors: colorsFor(spec, sent?.colors ?? []), speed: speedFor(spec, sent?.speed ?? null) };
}

/** The form after another effect was picked: the colours already chosen stay, as many as the new effect takes, and so
 * does the speed when the new effect has it. */
export function pickEffect(effects: EffectSpec[], draft: EffectValue, name: string): EffectValue {
  const spec = specOf(effects, name);
  if (!spec) return draft;
  return { name, colors: colorsFor(spec, draft.colors), speed: speedFor(spec, draft.speed) };
}

/** The effect to send, or why the form is not right. */
export function effectValue(effects: EffectSpec[], draft: EffectValue): EffectValue | string {
  const spec = specOf(effects, draft.name);
  if (!spec) return "Choose an effect";
  if (draft.colors.length !== spec.colors || draft.colors.some((c) => !/^#[0-9a-f]{6}$/i.test(c))) {
    return `${spec.name}: ${spec.colors === 1 ? "one colour" : `${spec.colors} colours`} needed`;
  }
  if (spec.speeds.length === 0) return { name: spec.name, colors: draft.colors, speed: null };
  if (!spec.speeds.some(([n]) => n === draft.speed)) return `${spec.name}: choose a speed`;
  return { name: spec.name, colors: draft.colors, speed: draft.speed };
}
