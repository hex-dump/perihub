<script lang="ts">
  import { onDestroy, untrack } from "svelte";
  import {
    api,
    errorText,
    type Battery,
    type MouseInfo,
    type MouseState,
    type ProfileSettings,
    type ReceiverReport,
  } from "../lib/api";
  import { reportRateLabel } from "../lib/keys";
  import BackupsDialog from "../components/BackupsDialog.svelte";
  import ConfirmButton from "../components/ConfirmButton.svelte";
  import { mouseRestoreMessage } from "../lib/backups";
  import TestReport from "../components/TestReport.svelte";
  import DeviceTestReport from "../components/DeviceTestReport.svelte";
  import { receiverTest } from "../lib/testReport";
  import ExperimentalDialog from "../components/ExperimentalDialog.svelte";
  import KeepChangesDialog from "../components/KeepChangesDialog.svelte";
  import MousePerformance from "./MousePerformance.svelte";
  import MouseButtons from "./MouseButtons.svelte";
  import MouseLighting from "./MouseLighting.svelte";

  interface Props {
    info: MouseInfo | null;
    error: string | null;
    /** A wireless mouse that is turned off or asleep behind its receiver. */
    asleep: string | null;
    /** Connected Logitech receivers, with the paired devices PeriHub does not support yet. */
    receivers?: ReceiverReport[];
    initialTab?: string;
  }

  let { info, error: connectError, asleep, receivers = [], initialTab }: Props = $props();
  /** Paired devices PeriHub can only describe: their notice replaces the connection error. */
  const unsupported = $derived(receivers.some((r) => r.unsupported.length > 0));

  const MESSAGE_MS = 4000;

  let mouse = $state<MouseState | null>(null);
  let selected = $state(1);
  let draft = $state<ProfileSettings | null>(null);
  type Tab = "performance" | "buttons" | "lighting";
  const startTab = untrack(() => initialTab);
  let tab = $state<Tab>(startTab === "buttons" || startTab === "lighting" ? startTab : "performance");
  let busy = $state<string | null>(null);
  let error = $state<string | null>(null);
  let message = $state<string | null>(null);
  /** A preview (DPI or LED) is active on the mouse and must be reverted. */
  let previewing = $state(false);
  /** Changing it recreates the editors that preview, which drops the previews they have not sent. */
  let previewEpoch = $state(0);
  let battery = $state<Battery | null>(null);

  /** Untested models with experimental editing turned on (USB product ids). */
  let experimentalMice = $state<number[]>([]);
  let showExperimental = $state(false);
  /** A save to an untested mouse waits to be kept or undone. */
  let confirmKeep = $state(false);
  /** Tested, or untested with experimental editing: the controls are enabled. */
  const editable = $derived(!!info && (info.verified || info.experimental));
  const experimentalOn = $derived(!!info && experimentalMice.includes(info.product_id));

  api.experimentalMice().then((ids) => (experimentalMice = ids)).catch(() => {});
  const unlistenExperimental = api.onExperimental((ids) => (experimentalMice = ids));
  onDestroy(() => unlistenExperimental.then((off) => off()));

  async function setExperimental(enabled: boolean) {
    showExperimental = false;
    await run(enabled ? "Turning on experimental editing…" : "Turning off experimental editing…", async () => {
      // The mouse is opened again: the page reloads when the new status arrives.
      experimentalMice = await api.setExperimental(info!.product_id, enabled);
    });
  }

  /** After a save or a factory reset on an untested mouse. */
  function askToKeep() {
    if (info?.experimental) confirmKeep = true;
  }

  async function keep() {
    confirmKeep = false;
    await api.keepChanges().catch(() => {});
  }

  async function undo() {
    confirmKeep = false;
    busy = "Putting the backup back…";
    error = message = null;
    try {
      const restored = await api.undoLastWrite();
      previewing = false;
      adopt(restored.state);
      message = "Changes undone: the backup is back on the mouse.";
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = null;
    }
  }

  function batteryText(b: Battery): string {
    if (b.state === "full") return "Charged";
    return `${b.estimated ? "About " : ""}${b.percent}%`;
  }

  const clone = <T,>(value: T): T => JSON.parse(JSON.stringify(value));
  const savedSettings = $derived(mouse?.profiles.find((p) => p.number === selected)?.settings ?? null);
  const dirty = $derived(!!draft && JSON.stringify(draft) !== JSON.stringify(savedSettings));
  const leftClickMissing = $derived(!!draft && !draft.buttons.some((b) => b.type === "mouse" && b.button === 1));

  // Shown next to Save, where it does not move the page, and only for a few seconds.
  $effect(() => {
    if (!message) return;
    const timer = setTimeout(() => (message = null), MESSAGE_MS);
    return () => clearTimeout(timer);
  });

  async function run<T>(label: string, task: () => Promise<T>): Promise<T | undefined> {
    busy = label;
    error = message = null;
    try {
      return await task();
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = null;
    }
  }

  function adopt(next: MouseState, keepSelection = true) {
    mouse = next;
    const exists = (number: number) => next.profiles.some((p) => p.number === number);
    // While another program controls the mouse no profile is active (0): the selection stays.
    if (!keepSelection && exists(next.active_profile)) selected = next.active_profile;
    if (!exists(selected)) selected = next.profiles[0].number;
    draft = clone(next.profiles.find((p) => p.number === selected)!.settings);
  }

  // The mouse as it was when its profiles were read. Every device status carries the mouse in a new
  // object, also when it is about the keyboard or another program: only a mouse that changed (opened
  // again, taken by another program) is read again, or unsaved changes would be lost.
  let loadedFor: string | null = null;
  $effect(() => {
    const now = info ? JSON.stringify(info) : null;
    if (now && now !== loadedFor) {
      loadedFor = now;
      // The mouse was opened again (experimental editing turned off): the core already undid a
      // save that was not kept.
      confirmKeep = false;
      run("Reading profiles from the mouse…", api.mouseState).then((s) => s && adopt(s, false));
    }
    if (!info) {
      loadedFor = null;
      mouse = null;
      draft = null;
    }
  });

  async function revertPreview() {
    if (!previewing) return;
    previewing = false;
    await api.revertPreview().catch(() => {});
  }

  async function selectProfile(number: number) {
    if (number === selected) return;
    previewEpoch++;
    await revertPreview();
    selected = number;
    draft = clone(mouse!.profiles.find((p) => p.number === number)!.settings);
    message = null;
  }

  async function save() {
    if (!draft) return;
    previewEpoch++;
    const settings = clone(draft);
    const next = await run(`Saving profile ${selected} to the mouse…`, () => api.saveProfile(selected, settings));
    if (next) {
      previewing = false;
      adopt(next);
      message = `Profile ${selected} saved to the mouse.`;
      askToKeep();
    }
  }

  async function discard() {
    // The page goes back at once, whatever the mouse answers.
    previewEpoch++;
    draft = clone(savedSettings!);
    await revertPreview();
  }

  async function factory() {
    previewEpoch++;
    const next = await run(`Restoring profile ${selected}…`, () => api.restoreFactory(selected));
    if (next) {
      previewing = false;
      adopt(next);
      message = `Profile ${selected} restored to factory settings.`;
      askToKeep();
    }
  }

  let showBackups = $state(false);

  async function restoreBackup(id: string) {
    previewEpoch++;
    busy = "Restoring the backup…";
    error = message = null;
    try {
      const restored = await api.restoreMouseBackup(id);
      previewing = false;
      adopt(restored.state);
      message = mouseRestoreMessage(restored);
      showBackups = false;
    } finally {
      busy = null;
    }
  }

  async function activate(number: number) {
    previewEpoch++;
    const next = await run("Switching profile…", () => api.setActiveProfile(number));
    if (next) {
      previewing = false;
      mouse = next;
    }
  }

  async function toggleEnabled(number: number, box: HTMLInputElement) {
    const next = await run("Updating profiles…", () => api.setProfileEnabled(number, box.checked));
    if (next) mouse = next;
    // A change the mouse did not take: the box goes back to what the mouse has.
    else box.checked = mouse?.profiles.find((p) => p.number === number)?.enabled ?? box.checked;
  }

  async function changeTab(next: typeof tab) {
    if (next === tab) return;
    tab = next;
    await revertPreview();
  }

  // The mouse's own buttons change the active profile and DPI: follow them while this page is visible.
  const livePoll = setInterval(async () => {
    if (!mouse || busy || document.hidden) return;
    try {
      const live = await api.mouseLive();
      if (!mouse || busy) return;
      // Show the new active profile if the one on screen was the active one and has no unsaved changes.
      const follow = live.active_profile !== mouse.active_profile && selected === mouse.active_profile && !dirty && !previewing;
      const next = mouse.profiles.find((p) => p.number === live.active_profile);
      mouse.active_profile = live.active_profile;
      mouse.active_dpi_index = live.active_dpi_index;
      mouse.current_dpi = live.current_dpi;
      battery = live.battery;
      if (follow && next) {
        selected = next.number;
        draft = clone(next.settings);
      }
    } catch {
      /* mouse busy or unplugged: try again later */
    }
  }, 1000);

  // When leaving the page (for another section of the app), the mouse goes back to the saved settings.
  onDestroy(() => {
    clearInterval(livePoll);
    if (previewing) api.revertPreview().catch(() => {});
  });
</script>

<header class="row">
  <div class="col tight grow">
    <h1>Logitech {info?.model ?? asleep ?? "mouse"}</h1>
    <span class="muted small">
      {#if info}
        Firmware {info.firmware.map((f) => `${f.name} ${f.version}`).join(" · ")}
        {#if info.receiver}· through its LIGHTSPEED receiver{/if}
      {:else if asleep}
        Turned off or asleep
      {:else}
        Not connected
      {/if}
    </span>
  </div>
  {#if info && mouse && battery}
    <div class="now battery" class:low={battery.percent <= 15 && battery.state === "discharging"} title="The mouse's battery">
      <strong>{batteryText(battery)}</strong>
      <span class="small muted">{battery.state === "charging" ? "Battery, charging" : "Battery"}</span>
    </div>
  {/if}
  {#if info && mouse}
    <div class="now" title="The DPI the mouse is using right now">
      <strong>{mouse.current_dpi}</strong>
      <span class="small muted">DPI now</span>
    </div>
  {/if}
</header>

{#if asleep && !info}
  <div class="notice info">
    <b>{asleep === "Wireless mouse" ? "The wireless mouse" : `The ${asleep}`} is turned off or asleep.</b> Turn it on
    or move it: PeriHub finds it again within a few seconds.
  </div>
{/if}
{#if connectError && !info && !unsupported}<div class="notice error">{connectError}</div>{/if}
{#if !info}
  {#each receivers as receiver (receiver.product_id)}
    {#each receiver.unsupported as device (device.slot)}
      <div class="notice info">
        <b>{device.name ?? "A device"} is paired with your {receiver.name}.</b> PeriHub does not support it yet, so it
        only reads what the device is and changes nothing. A test report on PeriHub's GitHub page helps add it.
        <div class="row report">
          <DeviceTestReport fields={(version, system) => receiverTest(receiver, device, version, system)} />
        </div>
      </div>
    {/each}
    {#if receiver.unsupported.length === 0 && (receiver.bolt || !receiver.reachable)}
      <div class="notice info">
        {#if receiver.reachable}
          <b>PeriHub sees your {receiver.name}, but nothing paired with it has answered.</b> Move the mouse to wake it up.
          If this stays, a test report on PeriHub's GitHub page helps find out why.
        {:else}
          <b>PeriHub sees your {receiver.name} but cannot reach what is paired with it.</b> A test report on PeriHub's
          GitHub page helps find out why.
        {/if}
        <div class="row report">
          <DeviceTestReport fields={(version, system) => receiverTest(receiver, null, version, system)} />
        </div>
      </div>
    {/if}
  {/each}
{/if}
{#if info && !info.verified && info.experimental}
  <div class="notice error">
    <b>Experimental editing is on.</b> PeriHub has not been tested on the {info.model} yet: you can change and save its
    profiles at your own risk. Every save is backed up, and undone after 15 seconds unless you keep it. If it works,
    please send a mouse test report.
    <div class="row report">
      <TestReport mouse={info} />
      <button class="tiny" disabled={!!busy} onclick={() => setExperimental(false)}>Turn off</button>
    </div>
  </div>
{:else if info && !info.verified}
  <div class="notice info">
    <b>Experimental support.</b> PeriHub has not been tested on the {info.model}
    {info.receiver ? "through its LIGHTSPEED receiver" : ""} yet, so it only reads the mouse: you can look at its
    profiles, but nothing can be changed. A mouse test report on PeriHub's GitHub page{info.receiver
      ? " (issue #4)"
      : ""} helps get it fully supported.
    {#if info.pending_undo}
      <p class="small">
        <b>A save to this mouse was not kept</b> before PeriHub closed or the mouse was unplugged. Turn experimental
        editing on again to put the backup back.
      </p>
    {/if}
    {#if experimentalOn && info.receiver}
      <p class="small">Experimental editing is on for this model, but it needs the USB cable: connect the mouse with it.</p>
    {:else if experimentalOn}
      <p class="small">
        Experimental editing is on for this model, but PeriHub does not know how to write its memory yet (profile
        format {info.onboard.profile_format}, {info.onboard.sector_size}-byte sectors), so it stays read-only. A mouse
        test report helps add it.
      </p>
    {/if}
    <div class="row report">
      <TestReport mouse={info} />
      {#if experimentalOn}
        <button class="tiny" disabled={!!busy} onclick={() => setExperimental(false)}>Turn off experimental editing</button>
      {:else if info.pending_undo && !info.receiver}
        <button class="tiny" disabled={!!busy} onclick={() => setExperimental(true)}>Turn on and undo</button>
      {:else if !info.receiver}
        <button class="tiny" disabled={!!busy} onclick={() => (showExperimental = true)}>Edit anyway…</button>
      {/if}
    </div>
  </div>
  {#if info.host_mode}
    <div class="notice info">
      Another program, such as G HUB, left this mouse under software control: it is not using the profiles shown here.
    </div>
  {/if}
{:else if info && !info.writable}
  <div class="notice error">This model uses a profile format that has not been verified yet: editing is disabled.</div>
{/if}
{#if info && showExperimental}
  <ExperimentalDialog model={info.model} onconfirm={() => setExperimental(true)} onclose={() => (showExperimental = false)} />
{/if}
{#if info?.verified && info.host_mode}
  <div class="notice error">
    Another program controls the mouse right now, so it is not using the profiles shown here: buttons, macros and DPI
    levels may not work as saved. When that program is closed, PeriHub gives the mouse back its profiles within a few
    seconds.
  </div>
{/if}

{#if info && mouse?.profile_list === "damaged"}
  <div class="notice error">
    <b>The profile list in this mouse's memory is damaged</b> (its checksum does not match), so PeriHub cannot tell
    which profiles the mouse uses and will not save to it. Restore a backup with <b>Backups…</b>, or save a profile
    once with Logitech G HUB or Onboard Memory Manager. Please send a mouse test report too (Settings → Problems).
  </div>
{:else if info && mouse?.profile_list === "factory"}
  <div class="notice info">
    <b>Nothing has been saved to this mouse yet</b>, so it uses its factory profiles: those are the ones shown here.
    {#if info.verified}
      Your first save also creates the mouse's own profile list, with a copy of the factory profiles, so everything
      you don't change stays as it is.
    {:else if info.experimental}
      PeriHub creates that first profile list only on tested models: save a profile once with Logitech G HUB or
      Onboard Memory Manager, then PeriHub can save to this mouse.
    {/if}
  </div>
{/if}

{#if info && mouse && draft}
  <div class="profiles">
    {#each mouse.profiles as profile (profile.number)}
      {@const active = profile.number === mouse.active_profile}
      <div class="profile" class:selected={profile.number === selected} class:disabled={!profile.enabled}>
        <button class="pick" onclick={() => selectProfile(profile.number)} disabled={!!busy}
          title={profile.number === selected ? "Shown below" : "Show and edit this profile"}>
          <span class="title">
            <strong>Profile {profile.number}</strong>
            {#if active}<span class="chip ok">Active</span>{/if}
          </span>
          <span class="small muted">{profile.settings.dpi_levels.join(" / ")} DPI</span>
          <span class="small muted">{reportRateLabel(profile.settings.report_rate_ms)}</span>
        </button>
        <div class="actions">
          {#if !active}
            <button class="tiny" disabled={!!busy || !profile.enabled || !editable} onclick={() => activate(profile.number)}>
              Make active
            </button>
          {/if}
          <label class="row enabled" title="Disabled profiles are skipped by the profile switch button">
            <input
              type="checkbox"
              checked={profile.enabled}
              disabled={!!busy || !info.writable || !info.verified}
              title={info.verified ? undefined : "Needs a tested mouse model"}
              onchange={(e) => toggleEnabled(profile.number, e.target as HTMLInputElement)}
            />
            Enabled
          </label>
        </div>
      </div>
    {/each}
  </div>

  {#if error}<div class="notice error">{error}</div>{/if}

  <div class="tabs">
    <button class:active={tab === "performance"} onclick={() => changeTab("performance")}>DPI and polling</button>
    <button class:active={tab === "buttons"} onclick={() => changeTab("buttons")}>Buttons</button>
    <button class:active={tab === "lighting"} onclick={() => changeTab("lighting")}>Lighting</button>
  </div>

  <!-- Unverified models are read-only unless experimental editing is on: every control in the editor is disabled. -->
  <fieldset disabled={!editable}>
    {#if tab === "performance"}
      {#key previewEpoch}
        <MousePerformance {info} bind:settings={draft} onpreview={() => (previewing = true)}
          activeLevel={selected === mouse.active_profile ? mouse.active_dpi_index : null} />
      {/key}
    {:else if tab === "buttons"}
      <MouseButtons {info} bind:settings={draft} profiles={mouse.profiles} profileNumber={selected}
        macroSpace={mouse.macro_space} />
    {:else}
      {#key previewEpoch}
        <MouseLighting {info} bind:settings={draft} onpreview={() => (previewing = true)} />
      {/key}
    {/if}
  </fieldset>

  <footer class="row">
    <ConfirmButton class="danger" disabled={!!busy || !info.writable} onconfirm={factory} confirmLabel="Reset this profile?">
      Factory settings
    </ConfirmButton>
    <button disabled={!!busy || !info.writable} onclick={() => (showBackups = true)}>Backups…</button>
    <div class="grow"></div>
    {#if busy}
      <span class="muted small" role="status">{busy}</span>
    {:else if message}
      <span class="saved small" role="status">{message}</span>
    {:else if leftClickMissing}
      <span class="warn small">No button does a left click any more: assign one before saving.</span>
    {:else if dirty}
      <span class="unsaved">Unsaved changes</span>
    {/if}
    <button disabled={!dirty || !!busy} onclick={discard}>Discard</button>
    <button class="primary" disabled={!dirty || !!busy || leftClickMissing || !info.writable} onclick={save}>
      Save to mouse
    </button>
  </footer>

  {#if confirmKeep}
    <KeepChangesDialog model={info.model} onkeep={keep} onundo={undo} />
  {/if}

  {#if showBackups}
    <BackupsDialog device={info.model} load={api.mouseBackups} restore={restoreBackup} onclose={() => (showBackups = false)} />
  {/if}
{:else if info}
  {#if error}<div class="notice error">{error}</div>{:else}<div class="notice info">{busy ?? "Loading…"}</div>{/if}
{/if}

<style>
  header {
    margin-bottom: 20px;
    align-items: flex-start;
  }

  .tight {
    gap: 4px;
  }

  .now {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    padding: 8px 14px;
    border-radius: var(--radius);
    background: var(--panel);
    border: 1px solid var(--line);
    line-height: 1.15;
  }

  .now strong {
    font-size: 24px;
    font-variant-numeric: tabular-nums;
    color: var(--accent);
  }

  .now + .now {
    margin-left: 10px;
  }

  /* Orange stays for the DPI in use; the battery turns red only when it is low. */
  .now.battery strong {
    color: var(--text);
  }

  .now.battery.low strong {
    color: var(--bad);
  }

  .profiles {
    display: grid;
    grid-template-columns: repeat(5, minmax(0, 1fr));
    gap: 10px;
    margin-bottom: 16px;
  }

  .profile {
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    gap: 10px;
    padding: 12px;
    border-radius: var(--radius);
    background: var(--panel);
    border: 1px solid var(--line);
  }

  .profile.selected {
    border-color: var(--accent);
    background: var(--panel-3);
    box-shadow: inset 0 0 0 1px var(--accent);
  }

  .profile.disabled .pick {
    opacity: 0.5;
  }

  .pick {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: 2px;
    background: transparent;
    border: none;
    padding: 0;
    text-align: left;
    font-weight: 400;
  }

  .pick:hover:not(:disabled) {
    background: transparent;
  }

  .title {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 4px 6px;
    margin-bottom: 2px;
  }

  .title strong {
    font-size: 15px;
  }

  .actions {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 8px;
  }

  .tiny {
    padding: 3px 10px;
    font-size: 13px;
    white-space: nowrap;
  }

  .enabled {
    gap: 6px;
    font-size: 13px;
    color: var(--muted);
  }

  .notice {
    margin-bottom: 14px;
  }

  fieldset {
    border: none;
    margin: 0;
    min-width: 0;
    padding: 20px 0 90px;
  }

  footer {
    position: sticky;
    bottom: calc(-1 * var(--pad-y));
    margin: 0 calc(-1 * var(--pad-x)) calc(-1 * var(--pad-y));
    padding: 14px var(--pad-x);
    background: var(--panel);
    border-top: 1px solid var(--line);
    box-shadow: 0 -8px 24px rgba(0, 0, 0, 0.3);
  }

  .warn {
    color: var(--warn);
  }

  .saved {
    color: var(--ok);
  }

  .report {
    margin-top: 10px;
    gap: 10px;
    flex-wrap: wrap;
  }
</style>
