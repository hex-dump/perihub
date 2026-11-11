<script lang="ts">
  // A mouse or keyboard of another brand, supported from published documentation of its protocol and
  // never tried with PeriHub: what was read from it, and experimental changes. Apply sends a change
  // without saving it to the device's memory, always under a warning; Save to memory asks for the
  // No warranty box the first time. Every change is undone unless kept.
  import { onDestroy } from "svelte";
  import { api, errorText, type PeripheralInfo, type PeripheralSettings } from "../lib/api";
  import { acceptNoWarranty, noWarrantyAccepted } from "../lib/noWarranty";
  import { peripheralTest } from "../lib/testReport";
  import DeviceTestReport from "../components/DeviceTestReport.svelte";
  import KeepChangesDialog from "../components/KeepChangesDialog.svelte";
  import PeripheralSaveDialog from "../components/PeripheralSaveDialog.svelte";
  import BlindSettings from "../components/BlindSettings.svelte";

  interface Props {
    info: PeripheralInfo | null;
    error: string | null;
  }

  let { info: initial, error: connectError }: Props = $props();

  /** The device's buttons change its DPI level and profile. */
  const LIVE_MS = 5000;

  let live = $state<PeripheralInfo | null>(null);
  const info = $derived(live?.key === initial?.key ? live : initial);
  const name = $derived(info ? `${info.brand} ${info.model}` : "");

  /** The settings being edited; null until something is changed. */
  let draft = $state<PeripheralSettings | null>(null);
  /** The profile the draft was made on: the core refuses the change if the device switched since. */
  let draftProfile: number | null = null;
  const shown = $derived(draft ?? info?.settings ?? null);
  const changed = $derived(!!draft && !!info && JSON.stringify(draft) !== JSON.stringify(info.settings));

  // Another device, or the same one opened again: what was read and edited before is old.
  const opened = $derived(initial ? `${initial.key}-${initial.editable}` : "gone");
  $effect(() => {
    void opened;
    live = null;
    draft = null;
    confirmKeep = false;
  });

  const timer = setInterval(() => {
    if (!initial || draft || busy || confirmKeep) return;
    api.peripheralInfo(initial.key).then((i) => (live = i)).catch(() => {});
  }, LIVE_MS);
  onDestroy(() => clearInterval(timer));

  let confirmKeep = $state(false);
  let asking = $state(false);
  let busy = $state<string | null>(null);
  let error = $state<string | null>(null);
  let message = $state<string | null>(null);

  function edit(change: (s: PeripheralSettings) => void) {
    if (!info) return;
    if (!draft) draftProfile = info.active_profile;
    const next: PeripheralSettings = structuredClone($state.snapshot(draft ?? info.settings));
    change(next);
    draft = next;
    message = null;
  }

  async function run(label: string, task: () => Promise<void>) {
    busy = label;
    error = message = null;
    try {
      await task();
    } catch (e) {
      error = errorText(e);
      draft = null;
    } finally {
      busy = null;
    }
  }

  function send(save: boolean) {
    if (!info || !shown) return;
    const key = info.key;
    const settings = $state.snapshot(shown) as PeripheralSettings;
    return run(save ? "Saving to the device's memory…" : "Sending the change…", async () => {
      live = await api.peripheralChange(key, settings, save, draft ? draftProfile : (info?.active_profile ?? null));
      draft = null;
      confirmKeep = true;
    });
  }

  /** On devices where every change stays, the change itself is the save. */
  const saveFlag = () => !info?.changes_stay;

  function saveToMemory() {
    if (!info) return;
    if (noWarrantyAccepted(`save:${info.key}`)) send(saveFlag());
    else asking = true;
  }

  async function keep() {
    confirmKeep = false;
    if (info) await api.peripheralKeepChanges(info.key).catch(() => {});
    message = "Changes kept.";
  }

  function undo() {
    confirmKeep = false;
    if (!info) return;
    const key = info.key;
    return run("Putting the settings from before back…", async () => {
      live = await api.peripheralUndo(key);
      message = "Changes undone: the settings from before are back.";
    });
  }

  function optionLabel(id: string, value: number | undefined): string {
    const choice = info?.limits.options.find((c) => c.id === id);
    return choice?.values.find(([v]) => v === value)?.[1] ?? (value === undefined ? "not read" : `${value}`);
  }

  function modeName(led: number, mode: number): string {
    return info?.limits.leds[led]?.modes.find(([m]) => m === mode)?.[1] ?? `Other (mode ${mode})`;
  }
</script>

<header>
  <h1>{info ? name : "Device"}</h1>
  {#if info}<span class="chip test">{info.editable ? "Experimental" : "Read-only"}</span>{/if}
</header>

{#if connectError && !info}<div class="notice error">{connectError}</div>{/if}

{#if info}
  {#if info.problem}
    <div class="notice error">
      <b>PeriHub read something it did not expect from the {name}:</b> {info.problem}. It changes nothing on it. A test
      report would help support it.
    </div>
  {/if}
  <div class="notice info">
    <b>Nobody has tried PeriHub on the {name} yet.</b> PeriHub knows its protocol from {info.source}: what is below was
    read from the device{#if info.editable}, and changing it is experimental{/if}. Whether it matches your device or
    not, please send a test report.
    {#if info.pending_undo}
      <p class="small"><b>A change to this device was not kept</b> before PeriHub closed or it was unplugged.</p>
    {/if}
    <div class="row report">
      <DeviceTestReport fields={(version, system) => peripheralTest(info!, version, system)} />
    </div>
  </div>

  {#if confirmKeep}
    <KeepChangesDialog model={info.model} undoText="puts the settings from before back"
      checkText={info.kind === "keyboard" ? "look at its lighting" : "move it, click, check what you changed"}
      onkeep={keep} onundo={undo} />
  {/if}
  {#if asking}
    <PeripheralSaveDialog device={name} source={info.source} onclose={() => (asking = false)}
      onconfirm={() => {
        asking = false;
        acceptNoWarranty(`save:${info!.key}`);
        send(saveFlag());
      }} />
  {/if}
  {#if busy}<div class="notice info">{busy}</div>{/if}
  {#if error}<div class="notice error">{error}</div>{/if}
  {#if message}<div class="notice ok">{message}</div>{/if}

  {#if shown}
    <section class="card">
      <dl>
        {#if info.firmware}<dt>Firmware</dt><dd>{info.firmware}</dd>{/if}
        {#if info.battery}
          <dt>Battery</dt><dd>{info.battery.level} %{info.battery.charging ? ", charging" : ""}</dd>
        {/if}
        {#if info.profiles > 1}
          <dt>Profile</dt>
          <dd>{info.active_profile ?? "?"} of {info.profiles} <span class="muted small">(the one in use; PeriHub reads and changes only this one)</span></dd>
        {/if}
      </dl>

      {#if !info.readable}
        <BlindSettings {info} onchange={(i) => (live = i)} />
      {:else}
      <dl class="settings">
        {#if shown.dpi_levels.length > 0 || info.limits.dpi}
        <dt>{shown.dpi_levels.length === 1 ? "DPI" : "DPI levels"}</dt>
        <dd class="levels">
          {#each shown.dpi_levels as dpi, i (i)}
            {#if info.editable && info.limits.dpi}
              <input type="number" aria-label={`DPI level ${i + 1}`} min={info.limits.dpi.min} max={info.limits.dpi.max}
                step={info.limits.dpi.step} value={dpi} disabled={!!busy}
                onchange={(e) => edit((s) => (s.dpi_levels[i] = Number(e.currentTarget.value)))} />
            {:else}
              <span class="level" class:current={i + 1 === info.active_dpi}>{dpi}</span>
            {/if}
          {/each}
          {#if info.active_dpi}<span class="muted small">level {info.active_dpi} in use</span>{/if}
          {#if info.editable && !info.limits.dpi}<span class="muted small">(not changed on this device)</span>{/if}
        </dd>
        {/if}

        {#if shown.polling_hz !== null || info.limits.polling_hz.length > 0}
        <dt>Polling rate</dt>
        <dd>
          {#if info.editable && info.limits.polling_hz.length > 0}
            <select aria-label="Polling rate" value={shown.polling_hz} disabled={!!busy}
              onchange={(e) => edit((s) => (s.polling_hz = Number(e.currentTarget.value)))}>
              {#each info.limits.polling_hz as hz (hz)}<option value={hz}>{hz} Hz</option>{/each}
            </select>
          {:else}
            {shown.polling_hz ? `${shown.polling_hz} Hz` : "not read"}
          {/if}
        </dd>
        {/if}

        {#each info.limits.options as choice (choice.id)}
          <dt>{choice.label}</dt>
          <dd>
            {#if info.editable}
              <select aria-label={choice.label} value={shown.options[choice.id]} disabled={!!busy}
                onchange={(e) => edit((s) => (s.options[choice.id] = Number(e.currentTarget.value)))}>
                {#each choice.values as [value, label] (value)}<option value={value}>{label}</option>{/each}
              </select>
            {:else}
              {optionLabel(choice.id, shown.options[choice.id])}
            {/if}
          </dd>
        {/each}
      </dl>

      {#if shown.leds.length > 0}
        <h2>Lighting</h2>
        <table>
          <tbody>
            {#each shown.leds as led, i (i)}
              {@const limits = info.limits.leds[i]}
              <tr>
                <th>{limits?.name ?? `LED ${i + 1}`}</th>
                <td class="led">
                  {#if info.editable && limits}
                    <select aria-label={`${limits.name} mode`} value={led.mode} disabled={!!busy}
                      onchange={(e) => edit((s) => (s.leds[i].mode = Number(e.currentTarget.value)))}>
                      {#if !limits.modes.some(([m]) => m === led.mode)}
                        <option value={led.mode} disabled>{modeName(i, led.mode)}</option>
                      {/if}
                      {#each limits.modes as [mode, label] (mode)}<option value={mode}>{label}</option>{/each}
                    </select>
                    {#if limits.color}
                      <input type="color" aria-label={`${limits.name} colour`} value={led.color} disabled={!!busy}
                        onchange={(e) => edit((s) => (s.leds[i].color = e.currentTarget.value))} />
                    {/if}
                    {#if limits.brightness_step && led.brightness !== null}
                      <input type="range" aria-label={`${limits.name} brightness`} min="0" max="100"
                        step={limits.brightness_step} value={led.brightness} disabled={!!busy}
                        onchange={(e) => edit((s) => (s.leds[i].brightness = Number(e.currentTarget.value)))} />
                      <span class="small">{led.brightness} %</span>
                    {/if}
                  {:else}
                    <span class="swatch" style:background={led.color}></span>{modeName(i, led.mode)}{#if led.brightness !== null}, {led.brightness} %{/if}
                  {/if}
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}

      {#if info.buttons.length > 0}
        <h2>Buttons <span class="muted small">(as read; not changed yet)</span></h2>
        <table>
          <tbody>
            {#each info.buttons as button (button.name)}
              <tr><th>{button.name}</th><td>{button.action}</td></tr>
            {/each}
          </tbody>
        </table>
      {/if}

      {#if info.editable && info.changes_stay}
        <div class="notice warn change">
          <b>Experimental.</b> This device stores every change in its memory by itself: there is no change that lasts
          only until it is turned off. PeriHub asks for the No warranty box the first
          time, saves the settings first, reads them back after the change, and puts them back after 15 seconds unless
          you keep the change.
        </div>
        <div class="row actions">
          <button disabled={!changed || !!busy || confirmKeep} onclick={() => (draft = null)}>Discard</button>
          <button class="primary" disabled={!changed || !!busy || confirmKeep} onclick={saveToMemory}>Save to memory…</button>
        </div>
      {:else if info.editable}
        <div class="notice warn change">
          <b>Experimental.</b> <b>Apply</b> sends the change without saving it to the device's memory: it should last
          until the device is turned off, but nobody has confirmed that on this device. PeriHub saves the settings first,
          reads them back after the change, and puts them back after 15 seconds unless you keep the change.
          {#if info.can_save}<b>Save to memory</b> also stores it in the device.{/if}
        </div>
        <div class="row actions">
          <button disabled={!changed || !!busy || confirmKeep} onclick={() => (draft = null)}>Discard</button>
          <button class="primary" disabled={!changed || !!busy || confirmKeep} onclick={() => send(false)}>Apply</button>
          {#if info.can_save}
            <button disabled={!!busy || confirmKeep} onclick={saveToMemory}>Save to memory…</button>
          {/if}
        </div>
      {/if}
      {/if}
    </section>
  {/if}
{:else if !connectError}
  <div class="notice info">The device is not connected.</div>
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
    margin: 16px 0 8px;
    font-size: 1rem;
  }

  .report {
    margin-top: 10px;
  }

  dl.settings {
    margin-top: 8px;
  }

  dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 8px 16px;
    margin: 0;
    align-items: center;
  }

  dt,
  th {
    color: var(--muted);
    font-weight: normal;
    text-align: left;
  }

  dd {
    margin: 0;
  }

  .levels {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }

  .levels input {
    width: 90px;
  }

  .level.current {
    color: var(--ok);
    font-weight: 700;
  }

  table {
    width: 100%;
    border-collapse: collapse;
  }

  th,
  td {
    padding: 4px 0;
  }

  th {
    width: 35%;
  }

  .led {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .swatch {
    display: inline-block;
    width: 12px;
    height: 12px;
    margin-right: 6px;
    border-radius: 3px;
    border: 1px solid var(--line-strong);
    vertical-align: -1px;
  }

  .change {
    margin-top: 16px;
  }

  .actions {
    justify-content: flex-end;
    gap: 8px;
    margin-top: 8px;
  }
</style>
