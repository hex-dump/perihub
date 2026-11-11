<script lang="ts">
  // A Corsair mouse or keyboard: what PeriHub reads from it, and experimental lighting for this session.
  import { onDestroy } from "svelte";
  import { api, debounce, errorText, type CorsairInfo } from "../lib/api";
  import KeepChangesDialog from "../components/KeepChangesDialog.svelte";
  import NoWarranty from "../components/NoWarranty.svelte";
  import DeviceTestReport from "../components/DeviceTestReport.svelte";
  import { corsairTest } from "../lib/testReport";

  interface Props {
    info: CorsairInfo | null;
    error: string | null;
    /** PeriHub runs as a Flatpak and cannot see whether another Corsair program runs. */
    sandboxed?: boolean;
  }

  let { info, error: connectError, sandboxed = false }: Props = $props();

  let busy = $state<string | null>(null);
  let error = $state<string | null>(null);
  let asking = $state(false);
  let understood = $state(false);
  let confirmWorks = $state(false);
  let color = $state("#ff6a00");

  const on = $derived(!!info?.lighting);
  const noun = $derived(info?.kind === "keyboard" ? "keyboard" : "mouse");


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
    if (await run("Taking control of the lighting…", () => api.corsairSetLighting(info!.product_id, color))) confirmWorks = true;
  }

  async function keep() {
    confirmWorks = false;
    await api.corsairKeepLighting(info!.product_id).catch(() => {});
  }

  async function turnOff() {
    confirmWorks = false;
    await run(`Giving the ${noun} back to its own settings…`, () => api.corsairLightingOff(info!.product_id));
  }

  const sendColor = debounce((value: string) => {
    api.corsairSetLighting(info!.product_id, value).catch((e) => (error = errorText(e)));
  }, 150);
  onDestroy(() => sendColor.cancel());

  function pick(value: string) {
    color = value;
    if (on) sendColor(value);
  }

</script>

<header>
  <h1>{info ? `Corsair ${info.model}` : "Corsair device"}</h1>
  <span class="chip test">Experimental</span>
</header>

{#if connectError && !info}<div class="notice error">{connectError}</div>{/if}

{#if info}
  <div class="notice info">
    <b>Experimental support.</b> Nobody has tested PeriHub on a Corsair {noun} yet. PeriHub reads the {info.model} and
    changes nothing, except the lighting if you turn it on below. A test report on PeriHub's GitHub page with what you see
    here helps make it fully supported.
    <div class="row report">
      <DeviceTestReport fields={(version, system) => corsairTest(info!, version, system)} />
    </div>
  </div>

  {#if info.mode === 2}
    <div class="notice info">
      The {noun} was under the control of another program (such as iCUE) when PeriHub found it: what it shows may not be
      its own settings.
    </div>
  {/if}

  <section class="card">
    <h2>What PeriHub read</h2>
    <dl>
      <dt>Connection</dt><dd>{info.receiver ? "Wireless receiver" : "USB cable"}</dd>
      <dt>Firmware</dt><dd>{info.firmware ?? "not read"}</dd>
      <dt>Polling rate</dt><dd>{info.polling_hz ? `${info.polling_hz} Hz` : "not read"}</dd>
      {#if info.kind === "mouse"}<dt>DPI</dt><dd>{info.dpi ?? "not read"}</dd>{/if}
      <dt>LEDs</dt><dd>{info.leds}</dd>
    </dl>
    {#if info.read_errors.length}
      <p class="small muted">Could not read: {info.read_errors.join("; ")}</p>
    {/if}
  </section>

  <section class="card">
    <h2>Lighting (experimental)</h2>
    {#if info.leds === 0}
      <p class="muted">The {info.model} has no lighting.</p>
    {:else if on}
      <p>PeriHub drives the lighting until you turn it off or quit PeriHub. It is not saved in the {noun}.</p>
      <div class="row">
        <label class="row color">Colour <input type="color" value={color} oninput={(e) => pick((e.target as HTMLInputElement).value)} /></label>
        <div class="grow"></div>
        <button disabled={!!busy} onclick={turnOff}>Turn off</button>
      </div>
    {:else if asking}
      <div class="col ask">
        <p>
          PeriHub has to take the {noun} into software mode, as iCUE does. While it is on,
          {info.kind === "keyboard"
            ? "some keys (for example the volume dial or FN combinations) might not work, and it is not known whether every key still types"
            : "some buttons (for example the DPI button) might not work"}, and the lighting shows only what PeriHub sends.
          Nothing is written to the {noun}'s memory.
        </p>
        <p>
          It stays off while another program that drives Corsair mice runs (iCUE, OpenRGB, ckb-next, OpenLinkHub):
          two of them at once can leave a {noun} unable to {info.kind === "keyboard" ? "type" : "click"}.
        </p>
        {#if sandboxed}
          <p>
            <b>PeriHub runs as a Flatpak and cannot see the other programs running:</b> quit OpenRGB, ckb-next and
            OpenLinkHub yourself before you turn it on.
          </p>
        {/if}
        <p>
          <b>To get out:</b> <b>Turn off</b> here, <b>Turn off experimental lighting</b> in the tray menu, or quit
          PeriHub. If the {noun} still behaves oddly, unplug it and plug it back in. The confirmation that follows gives it
          back by itself after 15 seconds, without any key or click.
        </p>
        <NoWarranty device={`Corsair ${info.model}`} bind:accepted={understood} />
        <div class="row end">
          <button onclick={() => (asking = false)}>Cancel</button>
          <button class="primary" disabled={!understood || !!busy} onclick={turnOn}>Turn on for this session</button>
        </div>
      </div>
    {:else}
      <p class="muted">The {noun} uses its own lighting. You can try PeriHub's, for this session only.</p>
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
      undoText={`gives the ${noun} back to its own settings`}
      checkText={info.kind === "keyboard"
        ? "type a few keys, try the volume dial and FN combinations"
        : "move it, click with every button, use the scroll wheel"}
      onkeep={keep}
      onundo={turnOff}
    />
  {/if}
{:else if !connectError}
  <div class="notice info">No Corsair device connected.</div>
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


  .end {
    justify-content: flex-end;
    gap: 8px;
  }
</style>
