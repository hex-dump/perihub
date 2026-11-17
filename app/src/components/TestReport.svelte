<script lang="ts">
  import { onMount } from "svelte";
  import { getVersion } from "@tauri-apps/api/app";
  import { api, errorText, type MouseInfo, type WriteEntry } from "../lib/api";
  import { testFormUrl, testReport } from "../lib/testReport";
  import { readOnly, withWriteLog } from "../lib/readOnly.svelte";

  interface Props {
    mouse: MouseInfo;
  }

  const COPIED_MS = 3000;

  let { mouse }: Props = $props();
  let version = $state("");
  let system = $state("");
  let copied = $state(false);
  let error = $state<string | null>(null);
  /** The writes of this session, for the end of the report: read again before a click. */
  let writes = $state<WriteEntry[]>([]);

  async function loadWrites() {
    try {
      writes = await api.writeLog();
    } catch {
      // The report goes without them.
    }
  }

  const report = () => withWriteLog(testReport(mouse, version, system), writes, readOnly.on);

  onMount(async () => {
    loadWrites();
    version = await getVersion();
    system = (await api.appSettings()).system;
  });

  // Browsers allow writing to the clipboard only straight after the click: no waiting before it.
  async function copy() {
    try {
      await navigator.clipboard.writeText(report());
      error = null;
      copied = true;
      setTimeout(() => (copied = false), COPIED_MS);
    } catch (e) {
      error = errorText(e);
    }
  }

  async function openForm() {
    await loadWrites();
    api.openTestForm(testFormUrl(mouse, version, system, report())).catch((e) => (error = errorText(e)));
  }
</script>

<div class="row wrap">
  <button type="button" disabled={!version} onpointerenter={loadWrites} onfocus={loadWrites} onclick={copy}>
    {copied ? "Copied" : "Copy test report"}
  </button>
  <button type="button" disabled={!version} onclick={openForm}>Open test form</button>
</div>
{#if error}<div class="notice error" role="alert">{error}</div>{/if}
