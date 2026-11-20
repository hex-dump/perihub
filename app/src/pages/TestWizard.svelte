<script lang="ts">
  // A guided test report: pick the device, read it (only reads), see exactly what the report holds,
  // tick what was tried, and send it through PeriHub's form on GitHub.
  import { onMount } from "svelte";
  import { getVersion } from "@tauri-apps/api/app";
  import { api, errorText, type DeviceStatus, type ReportDevice, type UsbRecorderInfo, type WriteEntry } from "../lib/api";
  import UsbRecorder from "../components/UsbRecorder.svelte";
  import { userNames } from "../lib/redact";
  import { reportVersion } from "../lib/testReport";
  import { stateStatus } from "../lib/status";
  import Status from "../components/Status.svelte";
  import { readOnly, writeLogLines } from "../lib/readOnly.svelte";
  import {
    candidates, checklist, checklistText, formAddress, formUrl, needsPaste, prepareForm, readDevice, reportText,
    type Answer, type Probed, type Reading,
  } from "../lib/reportWizard";

  interface Props {
    status: DeviceStatus;
    /** A device of the list to start with, as the prompt for an unconfirmed device asks: it is read at once. */
    start?: string | null;
  }

  type Step = "pick" | "read" | "record" | "preview" | "tried" | "send";

  const TITLES: Record<Step, string> = {
    pick: "Pick your device",
    read: "Read the device",
    record: "Record its software",
    preview: "See what is shared",
    tried: "What did you try?",
    send: "Send the report",
  };
  /** Names of the fields of the forms, as the form shows them. */
  const FIELD_NAMES: Record<string, string> = {
    title: "Title",
    "device-type": "Device type",
    brand: "Brand",
    model: "Device",
    "usb-id": "USB ID",
    connection: "Connection",
    version: "PeriHub version",
    os: "System",
    report: "Test report",
    details: "Details",
  };
  /** The list of other devices is taken again while the first step is open, as devices come and go. */
  const REFRESH_MS = 2000;
  const COPIED_MS = 3000;

  let { status, start = null }: Props = $props();
  let step = $state<Step>("pick");
  let unknown = $state<ReportDevice[] | null>(null);
  let pickedId = $state<string | null>(null);
  let version = $state("");
  let system = $state("");
  let names = $state<string[]>([]);
  let probed = $state<Probed | null>(null);
  let readings = $state<Reading[]>([]);
  let reading = $state(false);
  let answers = $state<Record<string, Answer>>({});
  /** The writes of this session, for the end of the report like the other test reports. */
  let writes = $state<WriteEntry[] | null>(null);
  let copied = $state(false);
  let opened = $state(false);
  let error = $state<string | null>(null);
  /** What "Record the official software" can do here; null until asked. */
  let recorder = $state<UsbRecorderInfo | null>(null);
  /** The device being recorded, a copy: on Windows its USB devices disconnect for a moment when a recording starts. */
  let recordingDevice = $state<ReportDevice | null>(null);
  /** A recording is running: leaving the step would cancel it. */
  let recorderBusy = $state(false);
  /** The finished recording of the picked device, for its request. */
  let recorded = $state<string | null>(null);
  /** Only the latest reading counts: the user may go back and pick another device. */
  let readId = 0;

  const list = $derived(candidates(status, unknown ?? []));
  const picked = $derived(list.find((c) => c.id === pickedId) ?? null);
  const items = $derived(probed ? checklist(probed.candidate) : []);
  /** A USB device PeriHub does not know: its request can carry a recording of the official software. Here
   * and not only on the Send a report page, which the sidebar shows only when no known device is there. */
  const unknownDevice = $derived(picked?.target.type === "unknown" && picked.target.device.bus === "USB" ? picked.target.device : null);
  /** Nothing to tick for a device PeriHub does not know: that step is left out, and the recording comes instead. */
  const steps = $derived<Step[]>(
    items.length > 0
      ? ["pick", "read", "preview", "tried", "send"]
      : unknownDevice && recorder
        ? ["pick", "read", "record", "preview", "send"]
        : ["pick", "read", "preview", "send"],
  );
  const form = $derived(
    probed && version
      ? prepareForm(
          formAddress(probed.candidate, probed.live, version, system, unknownDevice ? recorded : null),
          checklistText(items, answers),
          names,
          writes ? writeLogLines(writes, readOnly.on) : [],
        )
      : null,
  );
  const paste = $derived(form ? needsPaste(form) : false);

  async function loadUnknown() {
    try {
      unknown = await api.devicesForReport();
    } catch (e) {
      error = errorText(e);
    }
  }

  onMount(() => {
    loadUnknown();
    api.usbRecordInfo().then((i) => (recorder = i), () => {});
    // Asked for one device that is there: nothing to pick, the first reading starts.
    if (start && list.some((c) => c.id === start)) {
      pickedId = start;
      go("read");
    }
    getVersion().then((v) => (version = version || v));
    api
      .appSettings()
      .then(async (s) => {
        system = s.system;
        version = reportVersion(await getVersion(), s.build);
        names = userNames([s.config_dir, s.log_dir]);
      })
      .catch(() => (system = "unknown system"));
    // Not while the window is minimized, and only while the list is on screen.
    const timer = setInterval(() => step === "pick" && !document.hidden && loadUnknown(), REFRESH_MS);
    return () => clearInterval(timer);
  });

  async function read() {
    const id = ++readId;
    const target = picked;
    if (!target) return;
    reading = true;
    probed = null;
    readings = [];
    const result = await readDevice(target, api, (r) => id === readId && (readings = r));
    if (id !== readId) return;
    readings = result.readings;
    // A device that is gone cannot be reported: stay here until it is back.
    probed = result.readings[0].state === "ok" ? result : null;
    reading = false;
  }

  /** Read again when the report is shown, so the preview holds what is sent. */
  async function loadWrites() {
    try {
      writes = await api.writeLog();
    } catch {
      // The report goes without them.
    }
  }

  function go(next: Step) {
    error = null;
    step = next;
    if (next === "read") read();
    if (next === "preview") loadWrites();
    if (next === "pick") readId++;
  }

  const move = (by: number) => go(steps[steps.indexOf(step) + by]);

  function pick(id: string) {
    if (id !== pickedId) {
      pickedId = id;
      probed = null;
      answers = {};
      recorded = null;
      recordingDevice = null;
      copied = opened = false;
    }
  }

  // Browsers allow writing to the clipboard only straight after the click: no waiting before it.
  async function copy() {
    if (!form) return;
    try {
      await navigator.clipboard.writeText(reportText(form));
      error = null;
      copied = true;
      setTimeout(() => (copied = false), COPIED_MS);
    } catch (e) {
      error = errorText(e);
    }
  }

  async function openForm() {
    if (!form) return;
    // Too long for an address: the report goes to the clipboard first, and is pasted into the form.
    if (paste) {
      try {
        await navigator.clipboard.writeText(reportText(form));
        copied = true;
        setTimeout(() => (copied = false), COPIED_MS);
      } catch (e) {
        error = errorText(e);
        return;
      }
    }
    try {
      await api.openTestForm(formUrl(form, !paste));
      error = null;
      opened = true;
    } catch (e) {
      error = errorText(e);
    }
  }

  const fieldName = (name: string) => FIELD_NAMES[name] ?? name;
  const hiddenText = (hidden: { what: string; count: number }[]) =>
    hidden.map((h) => `${h.count} ${h.what}${h.count === 1 ? "" : "s"}`).join(", ");
</script>

{#snippet preview()}
  {#if form}
    <div class="card col">
      <dl>
        {#each form.fields.filter(([name]) => name !== form.textField) as [name, value] (name)}
          <dt>{fieldName(name)}</dt>
          <dd>{value}</dd>
        {/each}
      </dl>
      <b class="small">{fieldName(form.textField)}</b>
      <pre class="report">{reportText(form)}</pre>
    </div>
    {#if form.hidden.length > 0}
      <div class="notice ok">
        <b>Hidden before sharing:</b> {hiddenText(form.hidden)}. They show as [user], [id hidden] and so on below.
      </div>
    {:else}
      <div class="notice info">PeriHub found nothing private in this report to hide.</div>
    {/if}
  {/if}
{/snippet}

<header class="page-head">
  <h1>Test my device</h1>
</header>

<ol class="steps">
  {#each steps as s, i (s)}
    <li class:current={s === step} class:done={steps.indexOf(step) > i}>
      <span class="number">{steps.indexOf(step) > i ? "✓" : i + 1}</span>
      <span class="title">{TITLES[s]}</span>
    </li>
  {/each}
</ol>

{#if error}<div class="notice error" role="alert">{error}</div>{/if}

{#if step === "pick"}
  <p class="lead">
    Pick the mouse, keyboard or receiver you want to test. PeriHub reads it, shows you what the report holds, and
    opens a form on GitHub (a free account is needed to submit it). Nothing is sent before you press Submit there.
  </p>
  {#if !status.scanned || (unknown === null && list.length === 0)}
    <p class="loading">Looking for devices…</p>
  {:else if list.length === 0}
    <div class="card col">
      <p>
        PeriHub found no device. If yours is connected, plug it straight into the computer (not through a hub or a
        dock) and wait a few seconds.
      </p>
    </div>
  {:else}
    <div class="col list" role="radiogroup" aria-label="Devices">
      {#each list as c (c.id)}
        <button
          type="button"
          class="card row device"
          class:picked={c.id === pickedId}
          role="radio"
          aria-checked={c.id === pickedId}
          onclick={() => pick(c.id)}
        >
          <span class="col tight grow">
            <b>{c.name}</b>
            <span class="muted small">{c.kind === "other" ? "Other device" : c.kind === "mouse" ? "Mouse" : "Keyboard"}</span>
          </span>
          <Status kind={stateStatus(c.state)} label={c.state} chip />
        </button>
      {/each}
    </div>
  {/if}
  <div class="row nav">
    <button type="button" class="primary" disabled={!picked || !version} onclick={() => move(1)}>Next</button>
  </div>
{:else if step === "read"}
  <div class="notice info">
    PeriHub only reads {picked?.name ?? "the device"} here. Nothing is written to it, no setting changes and nothing
    is saved.
  </div>
  <div class="card col">
    {#each readings as r (r.label)}
      <div class="row reading">
        <span class="mark" class:ok={r.state === "ok"} class:fail={r.state === "fail"}>
          {r.state === "ok" ? "✓" : r.state === "fail" ? "✗" : r.state === "skip" ? "–" : r.state === "run" ? "…" : ""}
        </span>
        <span class="col tight">
          <span>{r.label}</span>
          {#if r.detail}<span class="small muted">{r.detail}</span>{/if}
        </span>
      </div>
    {/each}
    {#if readings.length === 0}<p class="muted">Starting…</p>{/if}
  </div>
  {#if !reading && !probed}
    <div class="notice error" role="alert">PeriHub cannot report this device now. Go back, check that it is connected, and try again.</div>
  {:else if !reading && readings.some((r) => r.state === "fail" || r.state === "skip")}
    <div class="notice warn">
      Something could not be read. You can go on: the report then says what PeriHub could not read, which helps too.
    </div>
  {/if}
  <div class="row nav">
    <button type="button" onclick={() => move(-1)}>Back</button>
    <button type="button" disabled={reading} onclick={read}>Read again</button>
    <button type="button" class="primary" disabled={reading || !probed} onclick={() => move(1)}>Next</button>
  </div>
{:else if step === "record"}
  <div class="notice info">
    PeriHub does not know {picked?.name ?? "this device"} yet. If its official software changes settings on it (DPI,
    lighting, buttons…), a recording of that software is what lets it be added. It is optional: <b>Next</b> goes on
    without one.
  </div>
  {#if recordingDevice && recorder}
    <UsbRecorder
      device={recordingDevice}
      info={recorder}
      nextButton="Next"
      nextAction="see it"
      onrecorded={(text) => (recorded = text)}
      onbusy={(b) => (recorderBusy = b)}
      onclose={() => (recordingDevice = null)}
    />
  {:else}
    {#if recorded}<div class="notice ok">The recording goes into the request.</div>{/if}
    <button
      type="button"
      class="self-start"
      disabled={!unknownDevice}
      onclick={() => unknownDevice && (recordingDevice = $state.snapshot(unknownDevice))}
    >
      {recorded ? "Record again" : "Record the official software"}
    </button>
  {/if}
  <div class="row nav">
    <!-- Only while a recording runs: once it is done (or not started), the recorder may close with the step. -->
    <button type="button" disabled={recorderBusy} onclick={() => move(-1)}>Back</button>
    <button type="button" class="primary" disabled={recorderBusy} onclick={() => move(1)}>Next</button>
  </div>
{:else if step === "preview"}
  <div class="notice info">
    This is everything that goes into the form, as it will be shared. PeriHub leaves out serial numbers, the unit id
    of a device, hardware addresses and the user name in file paths.
  </div>
  {@render preview()}
  <div class="row nav">
    <button type="button" onclick={() => move(-1)}>Back</button>
    <button type="button" class="primary" onclick={() => move(1)}>Next</button>
  </div>
{:else if step === "tried"}
  <div class="notice info">
    Tell us what you tried with {probed?.candidate.name}, if you like. Leave what you did not try as it is.
  </div>
  <div class="card col">
    {#each items as item (item.id)}
      <label class="row check">
        <select bind:value={answers[item.id]} aria-label={item.label}>
          <option value="">Not tried</option>
          <option value="worked">Worked</option>
          <option value="failed">Did not work</option>
        </select>
        <span>{item.label}</span>
      </label>
    {/each}
  </div>
  <div class="row nav">
    <button type="button" onclick={() => move(-1)}>Back</button>
    <button type="button" class="primary" onclick={() => move(1)}>Next</button>
  </div>
{:else if step === "send"}
  {#if paste}
    <div class="notice warn">
      <b>This report is too long for a link.</b> PeriHub copies it to the clipboard and opens the form with that box
      empty: paste the report into the box called {form ? fieldName(form.textField) : "the report"}.
    </div>
  {:else}
    <div class="notice info">
      The form opens with these fields filled in. You can still change them there before you press Submit.
    </div>
  {/if}
  {@render preview()}
  <div class="row wrap nav">
    <button type="button" onclick={() => move(-1)}>Back</button>
    <button type="button" disabled={!form} onclick={copy}>{copied ? "Copied" : "Copy report"}</button>
    <button type="button" class="primary" disabled={!form} onclick={openForm}>
      {paste ? "Copy report and open the form" : "Open the form"}
    </button>
  </div>
  {#if opened}
    <div class="notice ok">
      The form is open in your browser.{paste ? " Paste the report into it." : ""} Press Submit new issue there to
      send it, or close the page to send nothing.
    </div>
  {/if}
{/if}

<style>
  .steps {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 18px;
    list-style: none;
    margin: 14px 0 18px;
    padding: 0;
    color: var(--faint);
    font-size: 13px;
  }

  .steps li {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .number {
    display: inline-grid;
    place-items: center;
    width: 22px;
    height: 22px;
    border-radius: 50%;
    border: 1px solid var(--line-strong);
    font-size: 12px;
    font-weight: 600;
  }

  .steps .current {
    color: var(--text);
    font-weight: 600;
  }

  .steps .current .number {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--on-accent);
  }

  .steps .done .number {
    color: var(--ok);
    border-color: var(--ok);
  }

  .notice {
    margin-bottom: 14px;
  }

  .list {
    gap: 10px;
    margin-bottom: 14px;
  }

  .device {
    gap: 12px;
    text-align: left;
    width: 100%;
  }

  .device.picked {
    border-color: var(--accent);
    background: var(--accent-soft);
  }

  .tight {
    gap: 1px;
  }

  .nav {
    margin-top: 16px;
    gap: 10px;
  }

  .reading {
    gap: 12px;
    align-items: flex-start;
  }

  .mark {
    width: 16px;
    text-align: center;
    color: var(--muted);
  }

  .mark.ok {
    color: var(--ok);
  }

  .mark.fail {
    color: var(--bad);
  }

  dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 6px 16px;
    margin: 0;
    font-size: 14px;
  }

  dt {
    color: var(--muted);
  }

  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }

  .report {
    margin: 0;
    max-height: 320px;
    overflow: auto;
    padding: 12px;
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    background: var(--field);
    font-size: 12.5px;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .check {
    gap: 12px;
  }

  .check select {
    flex: none;
    width: 140px;
  }

  .self-start {
    align-self: flex-start;
  }
</style>
