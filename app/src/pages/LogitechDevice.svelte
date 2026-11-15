<script lang="ts">
  // A Logitech device behind a receiver that PeriHub knows by the features it reports: what it is and
  // what it can do, its battery and DPI as they are now, and the DPI as an experimental change (in the
  // device's RAM, set again every time it connects). Everything else is only read.
  import { onDestroy } from "svelte";
  import { api, errorText, type LogitechLive, type ReceiverReport, type UnsupportedReceiverDevice } from "../lib/api";
  import { reportRateLabel } from "../lib/keys";
  import { capabilities, effectName, kindName, layoutName, zoneName } from "../lib/logitechFeatures";
  import { acceptNoWarranty, noWarrantyAccepted } from "../lib/noWarranty";
  import { receiverTest } from "../lib/testReport";
  import DeviceTestReport from "../components/DeviceTestReport.svelte";
  import NoWarranty from "../components/NoWarranty.svelte";
  import UnifyingPermission from "../components/UnifyingPermission.svelte";

  interface Props {
    receiver: ReceiverReport | null;
    device: UnsupportedReceiverDevice | null;
    /** Running programs that set Logitech devices' settings themselves, such as "Solaar". */
    programs?: string[];
    /** PeriHub runs as a Flatpak and cannot see whether such a program runs. */
    sandboxed?: boolean;
  }

  let { receiver, device, programs = [], sandboxed = false }: Props = $props();

  const LIVE_MS = 2000;

  const found = $derived(device ? capabilities(device) : null);
  const name = $derived(device?.name ?? "device");
  const kind = $derived(kindName(device?.kind ?? null).toLowerCase());
  /** PeriHub has the device open: it answers, and its DPI can be read and changed. */
  const open = $derived(!!device && !!receiver && receiver.open_slots.includes(device.slot));
  /** The DPI can be changed: adjustable DPI and no onboard profiles, whose active profile sets the DPI
   * (as in the core's `dpi_changeable`). */
  const changeable = $derived(!!device?.dpi && !device.onboard);
  /** The "No warranty" box is asked once per model. */
  const warrantyKey = $derived(`dpi:${name}`);

  let live = $state<LogitechLive | null>(null);
  let dpi = $state<number | null>(null);
  let busy = $state<string | null>(null);
  let error = $state<string | null>(null);
  let message = $state<string | null>(null);
  /** Waiting for the "No warranty" box before the first change. */
  let asking = $state(false);
  let understood = $state(false);

  async function refresh() {
    if (!device || !receiver || !open || busy || document.hidden) return;
    try {
      live = await api.logitechLive(receiver.product_id, device.slot);
      if (dpi === null && live.dpi !== null) dpi = live.dpi;
    } catch {
      /* asleep or busy: tried again later */
    }
  }

  $effect(() => {
    if (open) refresh();
    else live = null;
  });
  const poll = setInterval(refresh, LIVE_MS);
  onDestroy(() => clearInterval(poll));

  async function run(label: string, task: () => Promise<void>, done: string) {
    busy = label;
    error = message = null;
    try {
      await task();
      message = done;
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = null;
    }
    await refresh();
  }

  function apply() {
    if (dpi === null) return;
    if (!noWarrantyAccepted(warrantyKey)) {
      asking = true;
      return;
    }
    const value = dpi;
    run("Setting the DPI…", () => api.logitechSetDpi(receiver!.product_id, device!.slot, value), `DPI set to ${value}.`);
  }

  function confirm() {
    asking = false;
    understood = false;
    acceptNoWarranty(warrantyKey);
    apply();
  }

  function reset() {
    dpi = null;
    run(
      "Putting the device's own DPI back…",
      () => api.logitechResetDpi(receiver!.product_id, device!.slot),
      "The device's own DPI is back.",
    );
  }

  function dpiText(caps: NonNullable<UnsupportedReceiverDevice["dpi"]>): string {
    if (caps.list.length) return caps.list.join(", ");
    return `${caps.min} to ${caps.max}${caps.step ? `, in steps of ${caps.step}` : ""}`;
  }

  /** Who drives the lights, from 0x8071 function 5. */
  function controlText(control: number): string {
    if (control === 0) return `The ${kind} itself, with its own effects`;
    if (control === 3) return "A program, such as G HUB";
    return `Mode ${control}`;
  }

  function seconds(s: number): string {
    return s >= 60 && s % 60 === 0 ? `${s / 60} min` : `${s} s`;
  }

  function batteryText(b: NonNullable<LogitechLive["battery"]>): string {
    if (b.state === "full") return "Charged";
    return `${b.estimated ? "About " : ""}${b.percent}%${b.state === "charging" ? ", charging" : ""}`;
  }
</script>

<header>
  <div class="col tight grow">
    <h1>Logitech {name}</h1>
    <span class="muted small">
      {#if device && receiver}
        {kindName(device.kind)} · through its {receiver.name}, slot {device.slot}{open ? "" : " · asleep"}
      {:else}
        Not connected
      {/if}
    </span>
  </div>
  {#if live?.battery}
    <div class="now" title="The device's battery">
      <strong>{batteryText(live.battery)}</strong>
      <span class="small muted">Battery</span>
    </div>
  {/if}
  {#if live?.dpi}
    <div class="now" title="The DPI the device is using right now">
      <strong>{live.dpi}</strong>
      <span class="small muted">DPI now</span>
    </div>
  {/if}
</header>

{#if device && receiver && found}
  <div class="notice info">
    {#if changeable}
      <b>Experimental.</b> PeriHub knows the {name} by the features it reports. It can change its DPI, which nobody has
      tried on such a {kind} yet; everything else is only read. Whether it works or not, a test report on PeriHub's
      GitHub page helps.
    {:else}
      <b>Not supported yet.</b> PeriHub knows the {name} by the features it reports, and only reads it: nothing on it
      is changed. A test report on PeriHub's GitHub page helps: it tells what this {kind} has.
    {/if}
    <div class="row report">
      <DeviceTestReport fields={(version, system) => receiverTest(receiver!, device!, version, system, live)} />
    </div>
  </div>

  {#each programs as program (program)}
    <div class="notice error">
      <b>{program} is running.</b> It sets its own DPI and other settings on Logitech devices, and may undo what PeriHub
      sets on the {name}. Use one of the two for this device.
    </div>
  {/each}

  {#if changeable && device.dpi}
    <section class="card">
      <h2>DPI <span class="chip test">Experimental</span></h2>
      {#if !open}
        <p class="muted">The {name} is asleep: move it or turn it on to change its DPI.</p>
      {:else}
        <p class="small muted">
          PeriHub sets the DPI in the {kind}'s RAM: nothing is written to its memory, and the {kind} goes back to its own
          setting when it is turned off or falls asleep. That is why PeriHub sets your DPI again every time it connects,
          while PeriHub is running.
        </p>
        <div class="row dpi">
          {#if device.dpi.list.length}
            <select bind:value={dpi} disabled={!!busy} aria-label="DPI">
              {#each device.dpi.list as value (value)}<option {value}>{value}</option>{/each}
            </select>
          {:else}
            <input
              type="number"
              aria-label="DPI"
              min={device.dpi.min}
              max={device.dpi.max}
              step={device.dpi.step || 1}
              bind:value={dpi}
              disabled={!!busy}
            />
          {/if}
          <button class="primary" disabled={!!busy || dpi === null || dpi === live?.dpi} onclick={apply}>Set DPI</button>
          {#if live?.saved_dpi}
            <button disabled={!!busy} onclick={reset}>Back to the device's own DPI</button>
          {/if}
          <span class="grow"></span>
          {#if busy}
            <span class="muted small" role="status">{busy}</span>
          {:else if message}
            <span class="saved small" role="status">{message}</span>
          {/if}
        </div>
        {#if live?.saved_dpi}
          <p class="small muted">PeriHub sets {live.saved_dpi} DPI every time the {name} connects.</p>
        {/if}
        {#if sandboxed}
          <p class="small muted">
            PeriHub runs as a Flatpak and cannot see whether Solaar or logiops runs: they set their own DPI and may undo
            PeriHub's, so quit them for this device.
          </p>
        {/if}
        {#if asking}
          <div class="ask col">
            <p>
              <b>Change the DPI of the {name}?</b> Nobody has tried this on such a {kind} yet. PeriHub only sends DPI
              values the {kind} itself says it accepts, and nothing is written to its memory: turning it off and on
              again brings its own DPI back.
            </p>
            <NoWarranty device={`Logitech ${name}`} bind:accepted={understood} />
            <div class="row end">
              <button onclick={() => ((asking = false), (understood = false))}>Cancel</button>
              <button class="primary" disabled={!understood} onclick={confirm}>Set DPI</button>
            </div>
          </div>
        {/if}
        {#if error}<div class="notice error">{error}</div>{/if}
      {/if}
    </section>
  {/if}

  {#if device.lighting}
    {@const lighting = device.lighting}
    {@const now = live?.lighting}
    <section class="card">
      <h2>Lighting <span class="chip">Read only</span></h2>
      <p class="small muted">
        PeriHub reads the {kind}'s lighting and does not change it yet. Your test report tells how to change it safely.
      </p>
      <dl>
        {#if lighting.brightness}
          <dt>Brightness</dt>
          <dd>
            {#if now?.brightness != null}{now.brightness} of {lighting.brightness.max}{now.on === false ? ", lights off" : ""}
            {:else}{lighting.brightness.min} to {lighting.brightness.max}{/if}
            {#if lighting.brightness.steps}<span class="muted">· {lighting.brightness.steps} levels on the key</span>{/if}
          </dd>
        {/if}
        {#if now?.control != null}<dt>Lights driven by</dt><dd>{controlText(now.control)}</dd>{/if}
        {#each lighting.effects?.zones ?? [] as zone (zone.index)}
          <dt>{zoneName(zone.location, device.kind)}</dt>
          <dd>{[...new Set(zone.effects.map((e) => effectName(e.id)))].join(", ") || "no effects listed"}</dd>
        {/each}
        {#if lighting.keys.length}<dt>Per-key colours</dt><dd>{lighting.keys.length} keys</dd>{/if}
        {#each [["Startup effect", lighting.effects?.startup], ["Shutdown effect", lighting.effects?.shutdown]] as const as [label, effect] (label)}
          {#if effect}
            <dt>{label}</dt>
            <dd class="row colors">
              {effect.enabled ? "On" : "Off"}
              {#each effect.colors as color, i (i)}<span class="swatch" style:background={color} title={color}></span>{/each}
            </dd>
          {/if}
        {/each}
        {#if lighting.effects?.idle_s != null}
          <dt>When not in use</dt>
          <dd>
            Dims after {seconds(lighting.effects.idle_s)}{lighting.effects.sleep_s != null
              ? `, off after ${seconds(lighting.effects.sleep_s)}`
              : ""}
          </dd>
        {/if}
        {#if lighting.layout != null}<dt>Layout</dt><dd>{layoutName(lighting.layout)}</dd>{/if}
      </dl>
      {#if lighting.errors.length}
        <p class="small muted">Could not read: {lighting.errors.join("; ")}</p>
      {/if}
    </section>
  {/if}

  <section class="card">
    <h2>What it can do</h2>
    {#if found.named.length}
      <ul>
        {#each found.named as capability (capability)}<li>{capability}</li>{/each}
      </ul>
    {:else}
      <p class="muted">None of the features PeriHub knows by name.</p>
    {/if}
    {#if found.others}
      <p class="small muted">
        And {found.others} more {found.others === 1 ? "feature" : "features"} PeriHub has no name for yet.
      </p>
    {/if}
  </section>

  <section class="card">
    <h2>What PeriHub read</h2>
    <dl>
      <dt>Firmware</dt><dd>{device.firmware.join(", ") || "not read"}</dd>
      <dt>HID++</dt><dd>{device.protocol}</dd>
      {#if device.dpi}<dt>DPI it accepts</dt><dd>{dpiText(device.dpi)}</dd>{/if}
      {#if device.report_rates_ms.length}
        <dt>Polling rates</dt><dd>{device.report_rates_ms.map(reportRateLabel).join(", ")}</dd>
      {/if}
      {#if device.onboard}
        <dt>Onboard memory</dt>
        <dd>
          {device.onboard.profile_count} profiles, profile format {device.onboard.profile_format},
          {device.onboard.sector_size}-byte sectors
        </dd>
      {/if}
    </dl>
    {#if device.read_errors.length}
      <p class="small muted">Could not read: {device.read_errors.join("; ")}</p>
    {/if}
  </section>

  <!-- Linux only: the way to remove the optional rule that opened the receiver. -->
  {#if receiver.unifying && !navigator.userAgent.includes("Windows")}
    <section class="card"><UnifyingPermission {receiver} /></section>
  {/if}
{:else}
  <div class="notice info">
    This device is not connected, or it is asleep. Turn it on or move it: PeriHub finds it again within a few seconds.
  </div>
{/if}

<style>
  header {
    display: flex;
    align-items: flex-start;
    gap: 12px;
    margin-bottom: 16px;
  }

  h1 {
    margin: 0;
  }

  .tight {
    gap: 4px;
  }

  .now {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    padding: 8px 14px;
    border-radius: var(--radius);
    background: var(--panel);
    border: 1px solid var(--line);
    line-height: 1.15;
  }

  .now strong {
    font-size: 24px;
    font-variant-numeric: tabular-nums;
    color: var(--accent);
  }

  h2 {
    margin: 0 0 12px;
    font-size: 1rem;
  }

  .card {
    margin-top: 16px;
  }

  .dpi {
    gap: 8px;
    align-items: center;
    margin-bottom: 10px;
  }

  .dpi input {
    width: 110px;
  }

  .ask {
    gap: 10px;
    margin-top: 12px;
  }

  .end {
    justify-content: flex-end;
    gap: 8px;
  }

  ul {
    margin: 0 0 10px;
    padding-left: 20px;
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

  .colors {
    gap: 6px;
    align-items: center;
  }

  .swatch {
    width: 14px;
    height: 14px;
    border-radius: 4px;
    border: 1px solid var(--line);
  }
</style>
