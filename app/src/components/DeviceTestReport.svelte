<script lang="ts">
  // Copy test report / Open test form for Corsair devices and the G300 (the device test form).
  import { onMount } from "svelte";
  import { getVersion } from "@tauri-apps/api/app";
  import { api, errorText } from "../lib/api";
  import { deviceTestFormUrl, type DeviceTestFields } from "../lib/testReport";

  interface Props {
    /** What PeriHub knows about the device, for this version and system. */
    fields: (version: string, system: string) => DeviceTestFields;
  }

  const COPIED_MS = 3000;

  let { fields }: Props = $props();
  let version = $state("");
  let system = $state("");
  let copied = $state(false);
  let error = $state<string | null>(null);

  onMount(async () => {
    version = await getVersion();
    try {
      system = (await api.appSettings()).system;
    } catch {
      system = "unknown system";
    }
  });

  // Browsers allow writing to the clipboard only straight after the click: no waiting before it.
  async function copy() {
    try {
      await navigator.clipboard.writeText(fields(version, system).report);
      error = null;
      copied = true;
      setTimeout(() => (copied = false), COPIED_MS);
    } catch (e) {
      error = errorText(e);
    }
  }

  function openForm() {
    api.openTestForm(deviceTestFormUrl(fields(version, system), version, system)).catch((e) => (error = errorText(e)));
  }
</script>

<div class="row wrap">
  <button type="button" class="tiny" disabled={!version} onclick={copy}>{copied ? "Copied" : "Copy test report"}</button>
  <button type="button" class="tiny" disabled={!version} onclick={openForm}>Open test form</button>
</div>
{#if error}<div class="notice error">{error}</div>{/if}

<style>
  .wrap {
    gap: 8px;
    flex-wrap: wrap;
  }
</style>
