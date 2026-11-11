<script lang="ts">
  // A Logitech device behind a receiver that PeriHub does not support yet: what it is and what it
  // can do, from the features it reports. PeriHub only reads it.
  import type { ReceiverReport, UnsupportedReceiverDevice } from "../lib/api";
  import { reportRateLabel } from "../lib/keys";
  import { capabilities, kindName } from "../lib/logitechFeatures";
  import { receiverTest } from "../lib/testReport";
  import DeviceTestReport from "../components/DeviceTestReport.svelte";
  import UnifyingPermission from "../components/UnifyingPermission.svelte";

  interface Props {
    receiver: ReceiverReport | null;
    device: UnsupportedReceiverDevice | null;
  }

  let { receiver, device }: Props = $props();

  const found = $derived(device ? capabilities(device) : null);
  const name = $derived(device?.name ?? "device");

  function dpiText(dpi: NonNullable<UnsupportedReceiverDevice["dpi"]>): string {
    if (dpi.list.length) return dpi.list.join(", ");
    return `${dpi.min} to ${dpi.max}${dpi.step ? `, in steps of ${dpi.step}` : ""}`;
  }
</script>

<header>
  <div class="col tight grow">
    <h1>Logitech {name}</h1>
    <span class="muted small">
      {#if device && receiver}
        {kindName(device.kind)} · through its {receiver.name}, slot {device.slot}
      {:else}
        Not connected
      {/if}
    </span>
  </div>
  {#if device}<span class="chip test">Read-only</span>{/if}
</header>

{#if device && receiver && found}
  <div class="notice info">
    <b>Not supported yet.</b> PeriHub recognizes the {name} by the features it reports, and only reads it: nothing on
    it is changed. Changing its settings comes next, starting with the DPI. A test report on PeriHub's GitHub page helps:
    it tells what this {kindName(device.kind).toLowerCase()} has.
    <div class="row report">
      <DeviceTestReport fields={(version, system) => receiverTest(receiver!, device!, version, system)} />
    </div>
  </div>

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
      {#if device.dpi}<dt>DPI</dt><dd>{dpiText(device.dpi)}</dd>{/if}
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
    align-items: center;
    gap: 12px;
    margin-bottom: 16px;
  }

  h1 {
    margin: 0;
  }

  .tight {
    gap: 4px;
  }

  h2 {
    margin: 0 0 12px;
    font-size: 1rem;
  }

  .card {
    margin-top: 16px;
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
</style>
