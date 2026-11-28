<script lang="ts">
  // Copy test report / Open test form for Corsair devices and the G300 (the device test form).
  import { onMount } from "svelte";
  import { getVersion } from "@tauri-apps/api/app";
  import { api, errorText, type WriteEntry } from "../lib/api";
  import { deviceTestFormUrl, reportVersion, type DeviceTestFields } from "../lib/testReport";
  import { fitsGithubLink } from "../lib/githubLink";
  import { readOnly, withWriteLog } from "../lib/readOnly.svelte";

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

  function report(): DeviceTestFields {
    const f = fields(version, system);
    return { ...f, report: withWriteLog(f.report, writes, readOnly.on) };
  }

  onMount(async () => {
    loadWrites();
    const v = await getVersion();
    version = v;
    try {
      const settings = await api.appSettings();
      system = settings.system;
      version = reportVersion(v, settings.build);
    } catch {
      system = "unknown system";
    }
  });

  // Browsers allow writing to the clipboard only straight after the click: no waiting before it.
  async function copy() {
    try {
      await navigator.clipboard.writeText(report().report);
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
    const fields = report();
    let url = deviceTestFormUrl(fields, version, system);
    pasted = false;
    if (!fitsGithubLink(url)) {
      try {
        await navigator.clipboard.writeText(fields.report);
      } catch (e) {
        error = errorText(e);
        return;
      }
      url = deviceTestFormUrl({ ...fields, report: "" }, version, system);
      pasted = true;
    }
    api.openTestForm(url).catch((e) => (error = errorText(e)));
  }
</script>

<div class="row wrap">
  <button type="button" class="tiny" disabled={!version} onpointerenter={loadWrites} onfocus={loadWrites} onclick={copy}>
    {copied ? "Copied" : "Copy test report"}
  </button>
  <button type="button" class="tiny" disabled={!version} onpointerenter={loadWrites} onfocus={loadWrites} onclick={openForm}>
    Open test form
  </button>
</div>
{#if pasted}
  <div class="notice info">The report is too long for a link: PeriHub copied it. Paste it into the empty Test report box of the form.</div>
{/if}
{#if error}<div class="notice error" role="alert">{error}</div>{/if}

<style>
  .wrap {
    gap: 8px;
    flex-wrap: wrap;
  }
</style>
