<script lang="ts">
  // The settings of a device that cannot report them (most SteelSeries mice), or that a readable
  // device cannot report (the colour of Razer mice): PeriHub sends one at a time and never saves it
  // to the device's memory. It cannot show, check or undo them, so the warning is always on screen.
  import { api, errorText, type BlindSetting, type BlindValue, type PeripheralInfo } from "../lib/api";

  interface Props {
    info: PeripheralInfo;
    /** Called with what the device shows after a setting was sent. */
    onchange: (info: PeripheralInfo) => void;
  }

  let { info, onchange }: Props = $props();

  const name = $derived(`${info.brand} ${info.model}`);
  /** What is typed or picked for each setting, before Send. */
  let drafts = $state<Record<string, string>>({});
  let busy = $state<string | null>(null);
  let error = $state<string | null>(null);

  function shown(value: BlindValue | null): string {
    if (!value) return "not sent yet";
    switch (value.type) {
      case "dpi":
        return `${value.value.join(", ")} DPI`;
      case "color":
        return value.value;
      default:
        return `${value.value}`;
    }
  }

  function initial(s: BlindSetting): string {
    if (s.last) return s.last.type === "dpi" ? s.last.value.join(", ") : `${s.last.value}`;
    switch (s.kind.type) {
      case "choice":
        return s.kind.values[0] ?? "";
      case "color":
        return "#ffffff";
      case "range":
        return `${s.kind.min}`;
      case "dpi":
        return "800";
    }
  }

  /** The value to send, or an explanation of what is wrong with what was typed. */
  function value(s: BlindSetting, text: string): BlindValue | string {
    switch (s.kind.type) {
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

  async function send(s: BlindSetting) {
    const v = value(s, drafts[s.id] ?? initial(s));
    if (typeof v === "string") {
      error = v;
      return;
    }
    busy = s.id;
    error = null;
    try {
      onchange(await api.peripheralSetBlind(info.key, s.id, v));
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = null;
    }
  }
</script>

{#if info.editable && info.readable}
  <div class="notice warn">
    <b>Experimental. PeriHub cannot read the lighting effect of the {name},</b> so it cannot show it, check a colour or
    put the effect from before back. <b>Send</b> shows one colour the way {info.source.replace(" (facts only)", "")} does,
    without saving it to the mouse's memory. Nobody has checked yet how long it lasts, or which lighting comes back
    after the mouse is unplugged.{#if info.settings.leds.length > 0} The brightness above is a separate setting, saved
      with an undo.{/if}
  </div>
{:else if info.editable}
  <div class="notice warn">
    <b>Experimental. The {name} cannot report its settings,</b> so PeriHub cannot show them, check a change or undo it.
    <b>Send</b> changes one setting until the mouse is unplugged or turned off: PeriHub never saves to its memory before
    someone has tested it (rivalcfg documents that settings not saved do not last). <b>To go back</b> to the settings
    saved in the mouse, unplug it and plug it back in.
  </div>
{/if}
{#if error}<div class="notice error">{error}</div>{/if}

<table>
  <tbody>
    {#each info.blind as s (s.id)}
      {@const text = drafts[s.id] ?? initial(s)}
      <tr>
        <th>{s.label}</th>
        <td>
          {#if info.editable}
            <div class="row">
              {#if s.kind.type === "choice"}
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
              <button class="tiny" disabled={!!busy} onclick={() => send(s)}>{busy === s.id ? "Sending…" : "Send"}</button>
            </div>
          {/if}
          <span class="muted small">Last sent: {shown(s.last)}</span>
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
  }

  input[type="text"] {
    width: 160px;
  }
</style>
