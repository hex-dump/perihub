<script lang="ts">
  // The settings of a device that cannot report them (most SteelSeries mice, the lighting of the Logitech
  // G213), or that a readable device cannot report (the colour of Razer mice): PeriHub sends one at a time and
  // never saves it to the device's memory. It cannot show, check or undo them, so the warning is always on screen.
  //
  // With `stored`, the settings the device keeps in its memory and cannot report either (the lighting effects of a
  // Razer keyboard): PeriHub remembers what it sent itself and can send the one before the last again, behind the No
  // warranty box. What the device held before PeriHub's first change was set by other software and cannot be put back.
  import { api, errorText, type BlindSetting, type BlindValue, type EffectValue, type PeripheralInfo } from "../lib/api";
  import {
    blindText,
    effectDraft,
    effectValue,
    makerSoftware,
    pickEffect,
    putBack,
    unknownBefore,
  } from "../lib/blind";
  import { acceptNoWarranty, noWarrantyAccepted } from "../lib/noWarranty";
  import { readOnly } from "../lib/readOnly.svelte";
  import PeripheralSaveDialog from "./PeripheralSaveDialog.svelte";

  interface Props {
    info: PeripheralInfo;
    /** Called with what the device shows after a setting was sent. */
    onchange: (info: PeripheralInfo) => void;
    /** Only the settings the device keeps in its memory and PeriHub remembers (otherwise only the others). */
    stored?: boolean;
  }

  let { info, onchange, stored = false }: Props = $props();

  const name = $derived(`${info.brand} ${info.model}`);
  /** What the device is, in the warning below. */
  const thing = $derived(info.kind === "keyboard" ? "keyboard" : "mouse");
  const settings = $derived(info.blind.filter((s) => !!s.remembered === stored));
  const software = $derived(makerSoftware(info.brand));
  /** What is typed or picked for each setting, before Send. */
  let drafts = $state<Record<string, string>>({});
  /** The same for an effect: its name, colours and speed. */
  let effectDrafts = $state<Record<string, EffectValue>>({});
  let busy = $state<string | null>(null);
  let error = $state<string | null>(null);
  /** The send that waits for the No warranty box. */
  let asking = $state<(() => Promise<void>) | null>(null);

  function initial(s: BlindSetting): string {
    if (s.last && s.last.type !== "effect") return s.last.type === "dpi" ? s.last.value.join(", ") : `${s.last.value}`;
    switch (s.kind.type) {
      case "choice":
        return s.kind.values[0] ?? "";
      case "color":
        return "#ffffff";
      case "range":
        return `${s.kind.min}`;
      case "dpi":
        return "800";
      case "effect":
        return "";
    }
  }

  /** What the effect form shows for a setting: what was picked, else what PeriHub last sent. */
  function effectOf(s: BlindSetting): EffectValue {
    return effectDrafts[s.id] ?? effectDraft(s.kind.type === "effect" ? s.kind.effects : [], s.last);
  }

  /** The value to send, or an explanation of what is wrong with what was typed. */
  function value(s: BlindSetting, text: string): BlindValue | string {
    switch (s.kind.type) {
      case "effect": {
        const effect = effectValue(s.kind.effects, effectOf(s));
        return typeof effect === "string" ? effect : { type: "effect", value: effect };
      }
      case "choice":
        return { type: "choice", value: text };
      case "color":
        return { type: "color", value: text };
      case "range": {
        const n = Number(text);
        const { min, max, step } = s.kind;
        if (!Number.isInteger(n) || n < min || n > max || (n - min) % step !== 0) {
          return `${s.label}: from ${min} to ${max} in steps of ${step}`;
        }
        return { type: "number", value: n };
      }
      case "dpi": {
        const { min, max, step, max_presets } = s.kind;
        const dpis = text.split(",").map((d) => Number(d.trim()));
        if (dpis.length > max_presets) return `${s.label}: at most ${max_presets} presets`;
        if (dpis.some((d) => !Number.isInteger(d) || d < min || d > max || (d - min) % step !== 0)) {
          return `${s.label}: each from ${min} to ${max} DPI in steps of ${step}`;
        }
        return { type: "dpi", value: dpis };
      }
    }
  }

  /** Runs a send, once the No warranty box was ticked where the device keeps what is sent. */
  function gated(s: BlindSetting, go: () => Promise<void>) {
    if (!s.remembered || noWarrantyAccepted(`save:${info.key}`)) return go();
    asking = go;
  }

  async function run(id: string, send: () => Promise<PeripheralInfo>) {
    busy = id;
    error = null;
    try {
      onchange(await send());
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = null;
    }
  }

  function send(s: BlindSetting) {
    const v = value(s, drafts[s.id] ?? initial(s));
    if (typeof v === "string") {
      error = v;
      return;
    }
    return gated(s, () => run(s.id, () => api.peripheralSetBlind(info.key, s.id, v)));
  }

  function putBackSetting(s: BlindSetting) {
    return gated(s, () => run(s.id, () => api.peripheralPutBackBlind(info.key, s.id)));
  }

  function confirmed() {
    const go = asking;
    asking = null;
    acceptNoWarranty(`save:${info.key}`);
    return go?.();
  }
</script>

{#if asking}
  <PeripheralSaveDialog device={name} source={info.source} unreadable onclose={() => (asking = null)}
    onconfirm={confirmed} />
{/if}

{#if stored}
  {#if info.editable}
    <div class="notice warn">
      <b>Experimental. PeriHub cannot read which lighting effect the {name} shows,</b> so it cannot show it or check it.
      <b>Send</b> puts an effect into the keyboard's memory the way {info.source.replace(" (facts only)", "")} does, so it
      should stay after the keyboard is turned off or unplugged: nobody has checked that yet. PeriHub remembers what it
      sent itself, and <b>Put back</b> sends the effect from before again. It cannot put back the effect the keyboard had
      before its first change: that was set by {software} (or by the keyboard itself) and cannot be read. If you want to
      get it back, note it in {software} first.
    </div>
  {/if}
{:else if settings.length > 0}
  {#if info.editable && info.readable && info.blind.some((b) => b.remembered)}
    <!-- Next to the lighting effects the keyboard keeps: what is sent here is the part that is not saved. -->
    <div class="notice warn">
      <b>Experimental. PeriHub cannot read what the {name} shows after these,</b> so it cannot show it, check it or put
      back what was there. <b>Send</b> shows or sets one of them the way {info.source.replace(" (facts only)", "")} does,
      without saving it to the {thing}'s memory. Nobody has checked yet how long it lasts, or what comes back after the
      {thing} is unplugged.{#if info.settings.leds.length > 0}{" "}The brightness above is a separate setting, saved with an
        undo.{/if}
    </div>
  {:else if info.editable && info.readable}
    <div class="notice warn">
      <b>Experimental. PeriHub cannot read the lighting effect of the {name},</b> so it cannot show it, check a colour or
      put the effect from before back. {#if thing === "keyboard"}<b>Send</b> sends what you picked, without saving it to the keyboard's
      memory.{:else}<b>Send</b> shows one colour the way {info.source.replace(" (facts only)", "")} does, without saving it to
      the {thing}'s memory.{/if} Nobody has checked yet how long it lasts, or which lighting comes back
      after the {thing} is unplugged.{#if info.settings.leds.length > 0}{" "}The brightness above is a separate setting, saved
        with an undo.{/if}
    </div>
  {:else if info.editable}
    <div class="notice warn">
      <b>Experimental. The {name} cannot report its settings,</b> so PeriHub cannot show them, check a change or undo it.
      <b>Send</b> changes one setting until the {thing} is unplugged or turned off: PeriHub never saves to its memory before
      someone has tested it ({#if thing === "keyboard"}its sources say the keyboard forgets what was sent, which nobody has checked
        with PeriHub yet{:else}rivalcfg documents that settings not saved do not last{/if}). <b>To go back</b> to the settings
      saved in the {thing}, unplug it and plug it back in.
    </div>
  {/if}
{/if}
{#if error}<div class="notice error" role="alert">{error}</div>{/if}

<table>
  <tbody>
    {#each settings as s (s.id)}
      {@const text = drafts[s.id] ?? initial(s)}
      {@const effects = s.kind.type === "effect" ? s.kind.effects : []}
      {@const draft = effectOf(s)}
      {@const spec = effects.find((e) => e.name === draft.name)}
      {@const back = putBack(s)}
      <tr>
        <th>{s.label}</th>
        <td>
          {#if info.editable}
            <div class="row">
              {#if s.kind.type === "effect"}
                <select aria-label={s.label} value={draft.name} disabled={!!busy}
                  onchange={(e) => (effectDrafts[s.id] = pickEffect(effects, effectOf(s), e.currentTarget.value))}>
                  {#each effects as e (e.name)}<option value={e.name}>{e.name}</option>{/each}
                </select>
                {#each draft.colors as color, i (i)}
                  <input type="color" aria-label={`${s.label}: colour ${i + 1}`} value={color} disabled={!!busy}
                    onchange={(e) => {
                      const next = effectOf(s);
                      effectDrafts[s.id] = { ...next, colors: next.colors.map((c, j) => (j === i ? e.currentTarget.value : c)) };
                    }} />
                {/each}
                {#if spec && spec.speeds.length > 0}
                  <select aria-label={`${s.label}: speed`} value={draft.speed} disabled={!!busy}
                    onchange={(e) => (effectDrafts[s.id] = { ...effectOf(s), speed: Number(e.currentTarget.value) })}>
                    {#each spec.speeds as [n, label] (n)}<option value={n}>{label}</option>{/each}
                  </select>
                {/if}
              {:else if s.kind.type === "choice"}
                <select aria-label={s.label} value={text} disabled={!!busy}
                  onchange={(e) => (drafts[s.id] = e.currentTarget.value)}>
                  {#each s.kind.values as v (v)}<option value={v}>{v}</option>{/each}
                </select>
              {:else if s.kind.type === "color"}
                <input type="color" aria-label={s.label} value={text} disabled={!!busy}
                  onchange={(e) => (drafts[s.id] = e.currentTarget.value)} />
              {:else}
                <input type="text" aria-label={s.label} value={text} disabled={!!busy}
                  placeholder={s.kind.type === "dpi" ? "e.g. 800, 1600" : ""}
                  onchange={(e) => (drafts[s.id] = e.currentTarget.value)} />
              {/if}
              <button class="tiny" disabled={!!busy || readOnly.on} onclick={() => send(s)}>{busy === s.id ? "Sending…" : "Send"}</button>
            </div>
          {/if}
          <span class="muted small">{s.remembered ? "Last sent by PeriHub" : "Last sent"}: {blindText(s.last, s)}</span>
          {#if s.remembered}
            {#if back}
              <div class="row put-back">
                {#if info.editable}
                  <button class="tiny" disabled={!!busy || readOnly.on} onclick={() => putBackSetting(s)}>Put back</button>
                {/if}
                <span class="muted small">before it, PeriHub sent: {blindText(back, s)}</span>
              </div>
            {/if}
            {#if unknownBefore(s)}
              <div class="muted small unknown">
                {#if s.last}
                  PeriHub can only put back what it sent itself. The effect the keyboard had before PeriHub's first
                  change was set by {software} (or by the keyboard itself) and is not known, so it cannot be put back.
                {:else}
                  PeriHub has not sent an effect yet. The effect the keyboard shows now was set by {software} (or by the
                  keyboard itself): PeriHub cannot read it, so it cannot put it back after a change. Note it in {software}
                  first if you want to restore it.
                {/if}
                If you changed the effect somewhere else since, what PeriHub last sent is out of date.
              </div>
            {:else}
              <div class="muted small unknown">If you changed the effect somewhere else since, what PeriHub last sent is out of date.</div>
            {/if}
          {/if}
        </td>
      </tr>
    {/each}
  </tbody>
</table>

<style>
  table {
    width: 100%;
    border-collapse: collapse;
    margin-top: 8px;
  }

  th,
  td {
    padding: 6px 0;
    vertical-align: top;
  }

  th {
    width: 35%;
    color: var(--muted);
    font-weight: normal;
    text-align: left;
  }

  .row {
    gap: 8px;
    align-items: center;
    margin-bottom: 4px;
    flex-wrap: wrap;
  }

  .put-back {
    margin-top: 4px;
  }

  .unknown {
    margin-top: 4px;
  }

  input[type="text"] {
    width: 160px;
  }
</style>
