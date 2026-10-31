<script lang="ts">
  import { onMount } from "svelte";
  import { getVersion } from "@tauri-apps/api/app";
  import { api, errorText, type MouseInfo } from "../lib/api";
  import { testFormUrl, testReport } from "../lib/testReport";

  interface Props {
    mouse: MouseInfo;
  }

  const COPIED_MS = 3000;

  let { mouse }: Props = $props();
  let version = $state("");
  let system = $state("");
  let copied = $state(false);
  let error = $state<string | null>(null);

  onMount(async () => {
    version = await getVersion();
    system = (await api.appSettings()).system;
  });

  // Browsers allow writing to the clipboard only straight after the click: no waiting before it.
  async function copy() {
    try {
      await navigator.clipboard.writeText(testReport(mouse, version, system));
      error = null;
      copied = true;
      setTimeout(() => (copied = false), COPIED_MS);
    } catch (e) {
      error = errorText(e);
    }
  }

  function openForm() {
    api.openTestForm(testFormUrl(mouse, version, system)).catch((e) => (error = errorText(e)));
  }
</script>

<div class="row wrap">
  <button type="button" disabled={!version} onclick={copy}>{copied ? "Copied" : "Copy test report"}</button>
  <button type="button" disabled={!version} onclick={openForm}>Open test form</button>
</div>
{#if error}<div class="notice error">{error}</div>{/if}
