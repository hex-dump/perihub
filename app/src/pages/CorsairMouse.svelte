<script lang="ts">
  // A Corsair mouse: what PeriHub reads from it, and experimental lighting for this session.
  import { onDestroy } from "svelte";
  import { getVersion } from "@tauri-apps/api/app";
  import { api, debounce, errorText, type CorsairMouseInfo } from "../lib/api";
  import KeepChangesDialog from "../components/KeepChangesDialog.svelte";

  interface Props {
    info: CorsairMouseInfo | null;
    error: string | null;
  }

  let { info, error: connectError }: Props = $props();

  const COPIED_MS = 3000;

  let busy = $state<string | null>(null);
  let error = $state<string | null>(null);
  let asking = $state(false);
  let understood = $state(false);
  let confirmWorks = $state(false);
  let color = $state("#ff6a00");
  let copied = $state(false);

  const on = $derived(!!info?.lighting);

  function hex(n: number) {
    return n.toString(16).padStart(4, "0");
  }

  async function run(label: string, task: () => Promise<unknown>) {
    busy = label;
    error = null;
    try {
      await task();
      return true;
    } catch (e) {
      error = errorText(e);
      return false;
    } finally {
      busy = null;
    }
  }

  async function turnOn() {
    asking = false;
    understood = false;
    if (await run("Taking control of the lighting…", () => api.corsairSetLighting(color))) confirmWorks = true;
  }

  async function keep() {
    confirmWorks = false;
    await api.corsairKeepLighting().catch(() => {});
  }

  async function turnOff() {
    confirmWorks = false;
    await run("Giving the mouse back to its own settings…", () => api.corsairLightingOff());
  }

  const sendColor = debounce((value: string) => {
    api.corsairSetLighting(value).catch((e) => (error = errorText(e)));
  }, 150);
  onDestroy(() => sendColor.cancel());

  function pick(value: string) {
    color = value;
    if (on) sendColor(value);
  }

  async function copyReport() {
    if (!info) return;
    const system = (await api.appSettings().catch(() => null))?.system ?? "?";
    const lines = [
      `PeriHub ${await getVersion()} on ${system}`,
      `Mouse: Corsair ${info.model}, USB ID 1b1c:${hex(info.product_id)}`,
      `Firmware: ${info.firmware ?? "not read"}`,
      `Mode at connection: ${info.mode ?? "not read"}`,
      `Polling: ${info.polling_hz ?? "not read"} Hz, DPI: ${info.dpi ?? "not read"}`,
      `Could not read: ${info.read_errors.join("; ") || "nothing"}`,
      `Experimental lighting: ${on ? "on" : "off"}`,
    ];
    try {
      await navigator.clipboard.writeText(lines.join("\n"));
      copied = true;
      setTimeout(() => (copied = false), COPIED_MS);
    } catch (e) {
      error = errorText(e);
    }
  }
</script>

<header>
  <h1>{info ? `Corsair ${info.model}` : "Corsair mouse"}</h1>
  <span class="chip test">Experimental</span>
</header>

{#if connectError && !info}<div class="notice error">{connectError}</div>{/if}

{#if info}
  <div class="notice info">
    <b>Experimental support.</b> Nobody has tested PeriHub on a Corsair mouse yet. PeriHub reads the {info.model} and
    changes nothing, except the lighting if you turn it on below. A report on PeriHub's GitHub page (device request)
    with what you see here helps a lot.
    <div class="row report">
      <button class="tiny" onclick={copyReport}>{copied ? "Copied" : "Copy report"}</button>
    </div>
  </div>

  {#if info.mode === 2}
    <div class="notice info">
      The mouse was under the control of another program (such as iCUE) when PeriHub found it: what it shows may not be
      its own settings.
    </div>
  {/if}

  <section class="card">
    <h2>What PeriHub read</h2>
    <dl>
      <dt>Firmware</dt><dd>{info.firmware ?? "not read"}</dd>
      <dt>Polling rate</dt><dd>{info.polling_hz ? `${info.polling_hz} Hz` : "not read"}</dd>
      <dt>DPI</dt><dd>{info.dpi ?? "not read"}</dd>
      <dt>LEDs</dt><dd>{info.leds}</dd>
    </dl>
    {#if info.read_errors.length}
      <p class="small muted">Could not read: {info.read_errors.join("; ")}</p>
    {/if}
  </section>

  <section class="card">
    <h2>Lighting (experimental)</h2>
    {#if on}
      <p>PeriHub drives the lighting until you turn it off or quit PeriHub. It is not saved in the mouse.</p>
      <div class="row">
        <label class="row color">Colour <input type="color" value={color} oninput={(e) => pick((e.target as HTMLInputElement).value)} /></label>
        <div class="grow"></div>
        <button disabled={!!busy} onclick={turnOff}>Turn off</button>
      </div>
    {:else if asking}
      <div class="col ask">
        <p>
          PeriHub has to take the mouse into software mode, as iCUE does. While it is on, some buttons (for example the
          DPI button) might not work, and the lighting shows only what PeriHub sends. Nothing is written to the mouse's
          memory.
        </p>
        <p>
          It stays off while another program that drives Corsair mice runs (iCUE, OpenRGB, ckb-next, OpenLinkHub):
          two of them at once can leave a mouse unable to click.
        </p>
        <p>
          <b>To get out:</b> <b>Turn off</b> here, <b>Turn off experimental editing</b> in the tray menu, or quit
          PeriHub. If the mouse still behaves oddly, unplug it and plug it back in.
        </p>
        <label class="row check">
          <input type="checkbox" bind:checked={understood} />
          I understand that this has not been tested on any Corsair mouse.
        </label>
        <div class="row end">
          <button onclick={() => (asking = false)}>Cancel</button>
          <button class="primary" disabled={!understood || !!busy} onclick={turnOn}>Turn on for this session</button>
        </div>
      </div>
    {:else}
      <p class="muted">The mouse uses its own lighting. You can try PeriHub's, for this session only.</p>
      <div class="row">
        <label class="row color">Colour <input type="color" value={color} oninput={(e) => pick((e.target as HTMLInputElement).value)} /></label>
        <div class="grow"></div>
        <button disabled={!!busy} onclick={() => (asking = true)}>Try experimental lighting…</button>
      </div>
    {/if}
    {#if busy}<p class="small muted" role="status">{busy}</p>{/if}
    {#if error}<div class="notice error">{error}</div>{/if}
  </section>

  {#if confirmWorks && on}
    <KeepChangesDialog
      model={info.model}
      undoText="gives the mouse back to its own settings"
      checkText="move it, click with every button, use the scroll wheel"
      onkeep={keep}
      onundo={turnOff}
    />
  {/if}
{:else if !connectError}
  <div class="notice info">No Corsair mouse connected.</div>
{/if}

<style>
  header {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-bottom: 16px;
  }

  h1 {
    margin: 0;
  }

  h2 {
    margin: 0 0 12px;
    font-size: 1rem;
  }

  .card {
    margin-top: 16px;
  }

  dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 6px 24px;
    margin: 0;
  }

  dt {
    color: var(--muted);
  }

  dd {
    margin: 0;
  }

  p {
    margin: 0 0 10px;
  }

  .report {
    margin-top: 10px;
  }

  .color {
    gap: 10px;
    align-items: center;
  }

  .ask {
    gap: 4px;
  }

  .check {
    gap: 10px;
    align-items: flex-start;
    margin: 6px 0 10px;
  }

  .end {
    justify-content: flex-end;
    gap: 8px;
  }
</style>
