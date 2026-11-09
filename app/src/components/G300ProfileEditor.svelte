<script lang="ts">
  // Edits one G300 profile (experimental editing). Only what the G300 can store is offered: 8 light
  // colours, 4 report rates, DPI from 250 to 2500 in steps of 250, and the functions of its buttons.
  // The left and right buttons keep what they have; a button set to something PeriHub cannot write
  // (a mouse button above 5, an unknown code) can be kept as it is.
  import { untrack } from "svelte";
  import type { G300Button, G300Profile, G300Settings, KeyInfo } from "../lib/api";
  import { keyLabel, MODIFIERS } from "../lib/keys";

  interface Props {
    profile: G300Profile;
    /** Keyboard keys to offer: only USB usages 0x04–0x65 are kept. */
    keys: KeyInfo[];
    names: string[];
    describe: (button: G300Button) => string;
    busy: boolean;
    onsave: (settings: G300Settings) => void;
    oncancel: () => void;
  }

  let { profile, keys, names, describe, busy, onsave, oncancel }: Props = $props();

  const COLORS: { value: string; label: string }[] = [
    { value: "#ffffff", label: "White" },
    { value: "#ff0000", label: "Red" },
    { value: "#00ff00", label: "Green" },
    { value: "#0000ff", label: "Blue" },
    { value: "#ffff00", label: "Yellow" },
    { value: "#00ffff", label: "Cyan" },
    { value: "#ff00ff", label: "Magenta" },
    { value: "#000000", label: "Off" },
  ];
  const RATES = [125, 250, 500, 1000];
  const DPIS = Array.from({ length: 10 }, (_, i) => (i + 1) * 250);

  type Kind =
    | "keep"
    | "none"
    | "mouse"
    | "key"
    | "dpi_up"
    | "dpi_down"
    | "dpi_cycle"
    | "profile_cycle"
    | "dpi_shift"
    | "dpi_default";
  const FUNCTIONS: { kind: Kind; label: string }[] = [
    { kind: "mouse", label: "Mouse click" },
    { kind: "key", label: "Keyboard key" },
    { kind: "dpi_up", label: "DPI +" },
    { kind: "dpi_down", label: "DPI –" },
    { kind: "dpi_cycle", label: "Cycle DPI" },
    { kind: "dpi_default", label: "Default DPI" },
    { kind: "dpi_shift", label: "DPI shift (hold)" },
    { kind: "profile_cycle", label: "Next profile" },
    { kind: "none", label: "Nothing" },
  ];
  const CLICKS = [
    { button: 1, label: "Left click" },
    { button: 2, label: "Right click" },
    { button: 3, label: "Middle click" },
    { button: 4, label: "Back" },
    { button: 5, label: "Forward" },
  ];
  /** The G300 keeps the left and right modifiers apart: the four on the left are offered. */
  const LEFT_MODIFIERS = MODIFIERS.filter((m) => m.bit <= 0x08);

  const clone = <T,>(value: T): T => JSON.parse(JSON.stringify(value));
  const original = untrack(() => clone(profile));

  let draft = $state<G300Settings>({
    color: original.color,
    report_rate_hz: original.report_rate_hz ?? 1000,
    dpi_levels: [...original.dpi_levels],
    default_level: original.default_level ?? 0,
    shift_dpi: original.shift_dpi,
    buttons: clone(original.buttons),
  });

  const keyChoices = $derived(keys.filter((k) => k.usage >= 0x04 && k.usage <= 0x65));
  const unchanged = $derived(
    JSON.stringify(draft) ===
      JSON.stringify({
        color: original.color,
        report_rate_hz: original.report_rate_hz ?? 1000,
        dpi_levels: original.dpi_levels,
        default_level: original.default_level ?? 0,
        shift_dpi: original.shift_dpi,
        buttons: original.buttons,
      }),
  );

  /** A button whose current function cannot be picked again: it can only be kept. */
  function onlyKept(b: G300Button): boolean {
    return b.type === "unknown" || (b.type === "mouse" && b.button > 5);
  }

  function kindOf(i: number): Kind {
    const b = draft.buttons[i];
    if (JSON.stringify(b) === JSON.stringify(original.buttons[i]) && onlyKept(b)) return "keep";
    return b.type as Kind;
  }

  function setKind(i: number, kind: Kind) {
    const keep = original.buttons[i];
    const next: G300Button =
      kind === "keep"
        ? clone(keep)
        : kind === "mouse"
          ? { type: "mouse", button: 3 }
          : kind === "key"
            ? keep.type === "key"
              ? clone(keep)
              : { type: "key", modifiers: 0, key: 0x04 }
            : ({ type: kind } as G300Button);
    draft.buttons[i] = next;
  }

  function toggleModifier(i: number, bit: number, on: boolean) {
    const b = draft.buttons[i];
    if (b.type !== "key") return;
    draft.buttons[i] = { ...b, modifiers: on ? b.modifiers | bit : b.modifiers & ~bit };
  }

  const value = (e: Event) => (e.target as HTMLSelectElement).value;
</script>

<div class="editor col">
  <label class="field">
    <span>Light</span>
    <select value={draft.color} onchange={(e) => (draft.color = value(e))}>
      {#each COLORS as c (c.value)}<option value={c.value}>{c.label}</option>{/each}
    </select>
  </label>
  <label class="field">
    <span>Report rate</span>
    <select value={String(draft.report_rate_hz)} onchange={(e) => (draft.report_rate_hz = Number(value(e)))}>
      {#each RATES as hz (hz)}<option value={String(hz)}>{hz} Hz</option>{/each}
    </select>
  </label>

  <fieldset>
    <legend>DPI levels</legend>
    {#each draft.dpi_levels as dpi, i (i)}
      <div class="row level">
        <span class="muted small">{i + 1}</span>
        <select aria-label={`DPI level ${i + 1}`} value={String(dpi)} onchange={(e) => (draft.dpi_levels[i] = Number(value(e)))}>
          {#each DPIS as d (d)}<option value={String(d)}>{d}</option>{/each}
        </select>
        <label class="row small">
          <input type="radio" name={`default-${original.number}`} checked={draft.default_level === i}
            onchange={() => (draft.default_level = i)} />
          Default
        </label>
      </div>
    {/each}
  </fieldset>

  <label class="field">
    <span>DPI shift</span>
    <select
      value={draft.shift_dpi === null ? "off" : String(draft.shift_dpi)}
      onchange={(e) => (draft.shift_dpi = value(e) === "off" ? null : Number(value(e)))}
    >
      <option value="off">Off</option>
      {#each DPIS as d (d)}<option value={String(d)}>{d} DPI while held</option>{/each}
    </select>
  </label>

  <table>
    <tbody>
      {#each draft.buttons as button, i (i)}
        <tr>
          <th>{names[i]}</th>
          <td>
            {#if i < 2}
              <span>{describe(button)}</span> <span class="muted small">(cannot be changed)</span>
            {:else}
              <div class="row wrap">
                <select aria-label={`${names[i]} function`} value={kindOf(i)} onchange={(e) => setKind(i, value(e) as Kind)}>
                  {#if onlyKept(original.buttons[i])}
                    <option value="keep">Keep: {describe(original.buttons[i])}</option>
                  {/if}
                  {#each FUNCTIONS as f (f.kind)}<option value={f.kind}>{f.label}</option>{/each}
                </select>
                {#if button.type === "mouse" && button.button <= 5}
                  <select aria-label={`${names[i]} click`} value={String(button.button)}
                    onchange={(e) => (draft.buttons[i] = { type: "mouse", button: Number(value(e)) })}>
                    {#each CLICKS as c (c.button)}<option value={String(c.button)}>{c.label}</option>{/each}
                  </select>
                {:else if button.type === "key"}
                  {#each LEFT_MODIFIERS as mod (mod.bit)}
                    <label class="row small">
                      <input type="checkbox" checked={(button.modifiers & mod.bit) !== 0}
                        onchange={(e) => toggleModifier(i, mod.bit, (e.target as HTMLInputElement).checked)} />
                      {mod.label}
                    </label>
                  {/each}
                  <span class="muted">+</span>
                  <select aria-label={`${names[i]} key`} value={String(button.key)}
                    onchange={(e) => button.type === "key" && (draft.buttons[i] = { ...button, key: Number(value(e)) })}>
                    {#if button.modifiers !== 0}<option value="0">(no key)</option>{/if}
                    {#if button.key !== 0 && !keyChoices.some((k) => k.usage === button.key)}
                      <option value={String(button.key)}>{keyLabel(button.key)}</option>
                    {/if}
                    {#each keyChoices as k (k.usage)}<option value={String(k.usage)}>{keyLabel(k.usage)}</option>{/each}
                  </select>
                {/if}
              </div>
            {/if}
          </td>
        </tr>
      {/each}
    </tbody>
  </table>

  <div class="row end">
    <button disabled={busy} onclick={oncancel}>Cancel</button>
    <button class="primary" disabled={busy || unchanged} onclick={() => onsave($state.snapshot(draft))}>Save to the mouse</button>
  </div>
</div>

<style>
  .editor {
    gap: 10px;
  }

  .field {
    display: grid;
    grid-template-columns: 7em 1fr;
    align-items: center;
    gap: 8px;
  }

  .field span {
    color: var(--muted);
  }

  fieldset {
    margin: 0;
    padding: 6px 10px 8px;
    border: 1px solid var(--line);
    border-radius: 8px;
  }

  legend {
    color: var(--muted);
    padding: 0 4px;
  }

  .level {
    gap: 8px;
    margin-top: 4px;
  }

  .wrap {
    flex-wrap: wrap;
    gap: 6px;
  }

  table {
    width: 100%;
    border-collapse: collapse;
  }

  th {
    width: 30%;
    color: var(--muted);
    font-weight: normal;
    text-align: left;
    vertical-align: top;
    padding: 6px 0;
  }

  td {
    padding: 3px 0;
  }

  .end {
    justify-content: flex-end;
    gap: 8px;
  }
</style>
