<script lang="ts">
  // The connected devices PeriHub does not know, each with a device request filled in for it.
  import { onMount } from "svelte";
  import { getVersion } from "@tauri-apps/api/app";
  import { api, errorText, type ReportDevice, type UsbRecorderInfo } from "../lib/api";
  import { deviceDetails, deviceRequestLink, reportDeviceName, reportVersion, reportUsbId as usbId } from "../lib/testReport";
  import UsbRecorder from "../components/UsbRecorder.svelte";

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
  /** What "Record the official software" can do here; null until asked. */
  let recorder = $state<UsbRecorderInfo | null>(null);
  /** The device whose recorder is open. A copy, kept apart from the list: its USB devices disconnect for a
   * moment when a recording starts on Windows, and the recording must not end with its row. */
  let recording = $state<ReportDevice | null>(null);
  const recordingId = $derived(recording ? usbId(recording) : null);
  /** Finished recordings, by USB id: they go into that device's request. */
  let recordings = $state<Record<string, string>>({});
  /** Said after a request whose recording did not fit in the link. */
  let pasteHint = $state<string | null>(null);

  const listed = $derived((devices ?? []).filter((d) => !shown.includes(usbId(d))));

  /** The list as last shown: the same list again changes nothing on screen, so it is not set again. */
  let shownJson = "";

  async function load() {
    try {
      const next = await api.devicesForReport();
      const json = JSON.stringify(next);
      if (json !== shownJson) {
        shownJson = json;
        devices = next;
      }
    } catch (e) {
      error = errorText(e);
    }
  }

  onMount(() => {
    load();
    api.usbRecordInfo().then((i) => (recorder = i), () => {});
    getVersion().then((v) => (version = version || v));
    api
      .appSettings()
      .then(async (s) => {
        system = s.system;
        version = reportVersion(await getVersion(), s.build);
      })
      .catch(() => (system = "unknown system"));
    // Not while the window is minimized: nobody sees the list.
    const timer = setInterval(() => document.hidden || load(), REFRESH_MS);
    return () => clearInterval(timer);
  });

  async function request(device: ReportDevice | null) {
    pasteHint = null;
    const recorded = device ? (recordings[usbId(device)] ?? null) : null;
    const link = device ? deviceRequestLink(device, version, system, recorded) : { url: BLANK_FORM, shortened: false };
    // A recording that does not fit in the link is copied whole, for the user to paste (before
    // anything is awaited: browsers allow writing to the clipboard only straight after the click).
    if (device && recorded && link.shortened) {
      try {
        await navigator.clipboard.writeText(deviceDetails(device, version, system, true, recorded));
        pasteHint = usbId(device);
      } catch (e) {
        error = errorText(e);
      }
    }
    api.openTestForm(link.url).catch((e) => (error = errorText(e)));
  }

  // Browsers allow writing to the clipboard only straight after the click: no waiting before it.
  async function copy(device: ReportDevice) {
    try {
      // The whole text, with the raw descriptors, which the link may have had to leave out.
      await navigator.clipboard.writeText(deviceDetails(device, version, system, true, recordings[usbId(device)] ?? null));
      error = null;
      copied = usbId(device);
      setTimeout(() => (copied = null), COPIED_MS);
    } catch (e) {
      error = errorText(e);
    }
  }
</script>

<header class="page-head">
  <h1>Send a report for your device</h1>
</header>

<p class="lead">
  {#if alone}<b>PeriHub found no mouse or keyboard it supports.</b>{/if}
  Pick yours below: PeriHub opens a device request on GitHub with its name, its USB id, the HID collections it
  shows and the layout of its report descriptors (read without opening anything that needs administrator rights, and
  without sending anything to the device), so its support can be looked into. Tick what it can do in its official
  software on the form. No serial number is included, and nothing is sent until you submit the
  form, which needs a GitHub account. <b>Record the official software</b> adds what that software sends the device
  while you change its settings, which is what a maintainer needs to support them.
</p>
{#if error}<div class="notice error" role="alert">{error}</div>{/if}

{#if devices === null}
  <p class="loading">Looking for devices…</p>
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
        {#if recorder && device.bus === "USB" && recordingId !== usbId(device)}
          <button type="button" class="tiny" disabled={recording !== null} onclick={() => (recording = $state.snapshot(device))}>
            {recordings[usbId(device)] ? "Record again" : "Record the official software"}
          </button>
        {/if}
        <button type="button" class="tiny" disabled={!version} onclick={() => copy(device)}>
          {copied === usbId(device) ? "Copied" : "Copy report"}
        </button>
        <button type="button" disabled={!version} onclick={() => request(device)}>Request support</button>
      </div>
      {#if recordings[usbId(device)] && recordingId !== usbId(device)}
        <span class="muted small recorded">The USB recording goes into this device's request.</span>
      {/if}
      {#if pasteHint === usbId(device)}
        <div class="notice warn" role="status">
          The recording did not fit in the link, so the whole report is copied: in the form, select everything in
          <b>What PeriHub saw</b> and paste in its place.
        </div>
      {/if}
    {/each}
  </div>
{/if}

{#if recorder && recording}
  {@const device = recording}
  <div class="recorder-slot">
    <UsbRecorder
      {device}
      info={recorder}
      onrecorded={(text) => (recordings = { ...recordings, [usbId(device)]: text })}
      onclose={() => (recording = null)}
    />
  </div>
{/if}

<style>
  .recorder-slot {
    margin-top: 10px;
  }

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

  .recorded {
    margin: -4px 0 0 4px;
  }
</style>
