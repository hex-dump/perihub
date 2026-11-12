<script lang="ts">
  // The connected devices PeriHub does not know, each with a device request filled in for it.
  import { onMount } from "svelte";
  import { getVersion } from "@tauri-apps/api/app";
  import { api, errorText, type ReportDevice } from "../lib/api";
  import { deviceRequestUrl, reportDeviceName, reportUsbId as usbId } from "../lib/testReport";

  interface Props {
    /** No supported device is connected: this page is all the sidebar shows. */
    alone: boolean;
    /** "vvvv:pppp" of the devices PeriHub lists by other means (QMK keyboards). */
    shown: string[];
  }

  const BLANK_FORM = "https://github.com/hex-dump/perihub/issues/new?template=device_request.yml&";
  /** The list is taken again while the page is open, as devices come and go. */
  const REFRESH_MS = 2000;
  const COPIED_MS = 3000;

  let { alone, shown }: Props = $props();
  let devices = $state<ReportDevice[] | null>(null);
  let version = $state("");
  let system = $state("");
  let copied = $state<string | null>(null);
  let error = $state<string | null>(null);

  const listed = $derived((devices ?? []).filter((d) => !shown.includes(usbId(d))));

  async function load() {
    try {
      devices = await api.devicesForReport();
    } catch (e) {
      error = errorText(e);
    }
  }

  onMount(() => {
    load();
    getVersion().then((v) => (version = v));
    api
      .appSettings()
      .then((s) => (system = s.system))
      .catch(() => (system = "unknown system"));
    const timer = setInterval(load, REFRESH_MS);
    return () => clearInterval(timer);
  });

  function request(device: ReportDevice | null) {
    const url = device ? deviceRequestUrl(device, version, system) : BLANK_FORM;
    api.openTestForm(url).catch((e) => (error = errorText(e)));
  }

  // Browsers allow writing to the clipboard only straight after the click: no waiting before it.
  async function copy(device: ReportDevice) {
    const url = new URL(deviceRequestUrl(device, version, system));
    try {
      await navigator.clipboard.writeText(url.searchParams.get("details") ?? "");
      error = null;
      copied = usbId(device);
      setTimeout(() => (copied = null), COPIED_MS);
    } catch (e) {
      error = errorText(e);
    }
  }
</script>

<header>
  <h1>Send a report for your device</h1>
</header>

<div class="notice info">
  {#if alone}<b>PeriHub found no mouse or keyboard it supports.</b>{/if}
  Pick yours below: PeriHub opens a device request on GitHub with its name, its USB id and the HID collections it
  shows, so its support can be looked into. No serial number is included, and nothing is sent until you submit the
  form, which needs a GitHub account.
</div>
{#if error}<div class="notice error">{error}</div>{/if}

{#if devices === null}
  <p class="muted">Looking for devices…</p>
{:else if listed.length === 0}
  <div class="card col">
    <p>No other USB or Bluetooth device found. If yours is connected, plug it straight into the computer (not through a
      hub or a dock) and wait a few seconds.</p>
    <button type="button" class="self-start" disabled={!version} onclick={() => request(null)}>Open an empty request</button>
  </div>
{:else}
  <p class="muted small hint">
    A device may show its chip maker's name (SONiX, Holtek, Telink…) instead of its brand: the USB id tells it apart.
  </p>
  <div class="col list">
    {#each listed as device (usbId(device))}
      <div class="card row device">
        <span class="col tight grow">
          <b>{reportDeviceName(device)}</b>
          <span class="muted small">
            USB ID {usbId(device)} · {device.bus} · {device.collections.length}
            {device.collections.length === 1 ? "collection" : "collections"}
          </span>
        </span>
        <button type="button" class="tiny" disabled={!version} onclick={() => copy(device)}>
          {copied === usbId(device) ? "Copied" : "Copy report"}
        </button>
        <button type="button" disabled={!version} onclick={() => request(device)}>Request support</button>
      </div>
    {/each}
  </div>
{/if}

<style>
  .hint {
    margin: 12px 0 10px;
  }

  .list {
    gap: 10px;
  }

  .device {
    gap: 12px;
  }

  .self-start {
    align-self: flex-start;
  }
</style>
