<script lang="ts">
  // "Record the official software": PeriHub records what the device's official software sends it while
  // the user changes one setting at a time, and gives the text for the device request
  // (perihub_core::usbcap, docs/captures.md). It never sends anything: the user sends the request.
  import { onDestroy, onMount } from "svelte";
  import { api, errorText, type ReportDevice, type UsbRecorderInfo, type UsbRecordProgress } from "../lib/api";
  import { CAPABILITIES, recordingSteps, stepName } from "../lib/usbRecording";

  interface Props {
    device: ReportDevice;
    info: UsbRecorderInfo;
    /** The recording as text, for the request. */
    onrecorded: (text: string) => void;
    onclose: () => void;
    /** Called when a recording starts or ends (true while one runs). */
    onbusy?: (busy: boolean) => void;
    /** The button the end of the recording points to, and what it does. */
    nextButton?: string;
    nextAction?: string;
  }

  let { device, info, onrecorded, onclose, onbusy, nextButton = "Request support", nextAction = "open it" }: Props = $props();

  type Phase = "setup" | "starting" | "recording" | "finishing" | "done" | "failed" | "cancelled";
  let phase = $state<Phase>("setup");
  let picked = $state<string[]>([]);
  let other = $state("");
  let understood = $state(false);
  let steps = $state<string[]>([]);
  let current = $state(0);
  let note = $state("");
  /** When each step started, and its name once it is done. */
  let started: number[] = [];
  let names: string[] = [];
  let status = $state("");
  let cleanup = $state<string[]>([]);
  let result = $state<string | null>(null);
  let error = $state<string | null>(null);

  const planned = $derived(recordingSteps(picked, other));
  const busy = $derived(phase === "starting" || phase === "recording" || phase === "finishing");
  $effect(() => onbusy?.(busy));

  function toggle(id: string, on: boolean) {
    picked = on ? [...picked, id] : picked.filter((p) => p !== id);
  }

  function received(p: UsbRecordProgress) {
    // The removal of a leftover USBPcap (App.svelte) speaks on the same channel.
    if (!busy) return;
    if (p.state === "status") status = p.text;
    else if (p.state === "cleanup") cleanup = [...cleanup, p.text];
    else if (p.state === "recording") {
      phase = "recording";
      current = 0;
      started = [Date.now()];
      names = [];
      status = "";
    } else if (p.state === "done") {
      result = p.text;
      phase = "done";
      onrecorded(p.text);
    } else if (p.state === "cancelled") phase = "cancelled";
    else if (p.state === "failed") {
      error = p.text;
      phase = "failed";
    }
  }

  let unlisten: (() => void) | null = null;
  onMount(() => {
    api.onUsbRecord(received).then((u) => (unlisten = u));
  });
  onDestroy(() => {
    unlisten?.();
    // Leaving the page ends a recording: USBPcap is removed all the same.
    if (busy) api.usbRecordCancel().catch(() => {});
  });

  async function start() {
    steps = planned;
    cleanup = [];
    result = null;
    error = null;
    status = info.windows ? "Waiting for administrator rights…" : "Waiting for your password…";
    phase = "starting";
    try {
      await api.usbRecordStart(device);
    } catch (e) {
      error = errorText(e);
      phase = "failed";
    }
  }

  async function next() {
    names[current] = stepName(steps[current], note);
    note = "";
    if (current < steps.length - 1) {
      current += 1;
      started[current] = Date.now();
      return;
    }
    phase = "finishing";
    status = "Finishing the recording…";
    try {
      await api.usbRecordStop(started.map((at_ms, i) => ({ at_ms, name: names[i] ?? stepName(steps[i], "") })));
    } catch (e) {
      error = errorText(e);
      phase = "failed";
    }
  }

  function cancel() {
    status = "Cancelling…";
    phase = "finishing";
    api.usbRecordCancel().catch((e) => {
      error = errorText(e);
      phase = "failed";
    });
  }
</script>

<div class="card col recorder" role="region" aria-label="Record the official software">
  <div class="row">
    <b class="grow">Record the official software</b>
    {#if !busy}<button class="ghost tiny" onclick={onclose}>Close</button>{/if}
  </div>

  {#if phase === "setup"}
    {#if !info.available}
      <div class="notice warn">Recording is not available here: {info.reason}.</div>
    {:else}
      <p class="small">
        PeriHub records the messages between the official software and this device while you change one setting at a
        time, and puts them in the request as text. With them a maintainer can add what PeriHub cannot read from the
        device alone.
      </p>
      <ul class="small">
        {#if info.windows}
          <li>
            PeriHub downloads <b>USBPcap</b> 1.5.4.0 (a free USB capture driver by Tomasz Moń) from its own GitHub
            release, checks that it is the expected file, installs it <b>for this recording only</b> and removes it
            completely at the end. Windows asks once for administrator rights. If USBPcap is already installed (with
            Wireshark, say), PeriHub uses it and leaves it installed.
          </li>
          <li>
            Your USB devices <b>disconnect for a couple of seconds twice</b>: when the recording starts and when it
            ends. A keyboard or mouse on the same port works again right after.
          </li>
        {:else}
          <li>
            PeriHub reads the device's USB bus with the kernel's <b>usbmon</b>, which asks for your password once.
            Nothing is installed, and usbmon is unloaded at the end if it was not loaded before. This works when the
            official software runs in a Windows virtual machine or in Wine.
          </li>
        {/if}
        <li>
          Only what the official software and this device send each other goes into the report. <b>Never what you
          type</b>: key presses, mouse movements, sound, every other device and the serial number are left out. You
          can read the text before you send the request; nothing is sent by PeriHub.
        </li>
        <li>Open the official software yourself. Do not type on a keyboard while recording.</li>
      </ul>
      <span class="small muted">What can its official software change? Each box is a step.</span>
      <div class="row wrap caps">
        {#each CAPABILITIES as c (c.id)}
          <label class="row small">
            <input
              type="checkbox"
              checked={picked.includes(c.id)}
              onchange={(e) => toggle(c.id, (e.target as HTMLInputElement).checked)}
            />
            {c.label}
          </label>
        {/each}
      </div>
      <label class="field">
        Something else (one per line)
        <textarea rows="2" bind:value={other} placeholder="Sidetone, lift-off distance, sensor settings…"></textarea>
      </label>
      <label class="row small">
        <input type="checkbox" bind:checked={understood} />
        {info.windows
          ? "I understand that USBPcap is installed for the recording and removed after it, and what is recorded."
          : "I understand what is recorded."}
      </label>
      <div class="row">
        <button disabled={!understood || planned.length === 0} onclick={start}>Start recording</button>
        {#if planned.length}<span class="small muted">{planned.length} steps</span>{/if}
      </div>
    {/if}
  {:else if phase === "recording"}
    <div class="col tight">
      <span class="small muted">Step {current + 1} of {steps.length}</span>
      <b>{steps[current]}</b>
      <span class="small muted">
        Make the change in the official software, wait about 3 seconds, then press Done. One change at a time.
      </span>
    </div>
    <label class="field">
      What you changed (optional)
      <input type="text" bind:value={note} maxlength="200" placeholder="e.g. 800 to 1600, Breathing, red" />
    </label>
    <ol class="small steps">
      {#each steps as step, i (i)}
        <li class:done={i < current} class:current={i === current}>{step}</li>
      {/each}
    </ol>
    <div class="row">
      <button onclick={next}>{current < steps.length - 1 ? "Done, next step" : "Done, finish"}</button>
      <button class="ghost" onclick={cancel}>Cancel</button>
    </div>
  {:else if phase === "starting" || phase === "finishing"}
    <p class="loading">{status}</p>
    {#if phase === "starting"}<button class="ghost self-start" onclick={cancel}>Cancel</button>{/if}
  {:else if phase === "done"}
    <div class="notice ok">The recording is in the request: press <b>{nextButton}</b> to {nextAction}.</div>
    <details>
      <summary class="small">See the recording</summary>
      <pre class="term recording">{result}</pre>
    </details>
  {:else if phase === "cancelled"}
    <div class="notice info">Recording cancelled: nothing was kept.</div>
  {:else if phase === "failed"}
    <div class="notice error" role="alert">The recording failed: {error}</div>
  {/if}

  {#each cleanup as line, i (i)}
    <div class="notice small" class:ok={line.startsWith("USBPcap was removed:")} class:warn={!line.startsWith("USBPcap was removed:")}>
      {line}
    </div>
  {/each}

  {#if phase === "done" || phase === "failed" || phase === "cancelled"}
    <button class="ghost self-start" onclick={() => (phase = "setup")}>Record again</button>
  {/if}
</div>

<style>
  .recorder ul {
    margin: 0;
    padding-left: 18px;
  }

  .caps {
    gap: 6px 16px;
  }

  .tight {
    gap: 4px;
  }

  .steps {
    margin: 0;
    padding-left: 20px;
  }

  .steps .done {
    color: var(--muted);
    text-decoration: line-through;
  }

  .steps .current {
    font-weight: 600;
  }

  .recording {
    max-height: 320px;
    overflow: auto;
    white-space: pre;
    font-size: 12px;
  }

  .self-start {
    align-self: flex-start;
  }
</style>
