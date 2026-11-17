<script lang="ts">
  import { onMount } from "svelte";
  import { getVersion } from "@tauri-apps/api/app";
  import { api, errorText, type MouseInfo, type WriteEntry } from "../lib/api";
  import { testFormUrl, testReport } from "../lib/testReport";
  import { MAX_URL_LENGTH, PASTE_NOTE } from "../lib/reportWizard";
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
  /** The report was too long for the form's address: it went to the clipboard instead. */
  let pasted = $state(false);
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

  // The writes were read when the pointer or the focus came to the button: no waiting before the
  // clipboard. A report too long for the address (a long transcript) is pasted into the form.
  async function openForm() {
    const text = report();
    let url = testFormUrl(mouse, version, system, text);
    pasted = false;
    if (url.length > MAX_URL_LENGTH) {
      try {
        await navigator.clipboard.writeText(text);
      } catch (e) {
        error = errorText(e);
        return;
      }
      url = testFormUrl(mouse, version, system, PASTE_NOTE);
      pasted = true;
    }
    api.openTestForm(url).catch((e) => (error = errorText(e)));
  }
</script>

<div class="row wrap">
  <button type="button" disabled={!version} onpointerenter={loadWrites} onfocus={loadWrites} onclick={copy}>
    {copied ? "Copied" : "Copy test report"}
  </button>
  <button type="button" disabled={!version} onpointerenter={loadWrites} onfocus={loadWrites} onclick={openForm}>
    Open test form
  </button>
</div>
{#if pasted}
  <div class="notice info">The report is too long for a link: PeriHub copied it. Paste it into the Test report box of the form.</div>
{/if}
{#if error}<div class="notice error">{error}</div>{/if}
