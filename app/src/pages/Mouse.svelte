<script lang="ts">
  import { onDestroy, untrack } from "svelte";
  import { api, errorText, type Battery, type MouseInfo, type MouseState, type ProfileSettings } from "../lib/api";
  import { reportRateLabel } from "../lib/keys";
  import ConfirmButton from "../components/ConfirmButton.svelte";
  import MousePerformance from "./MousePerformance.svelte";
  import MouseButtons from "./MouseButtons.svelte";
  import MouseLighting from "./MouseLighting.svelte";

  interface Props {
    info: MouseInfo | null;
    error: string | null;
    /** A wireless mouse that is turned off or asleep behind its receiver. */
    asleep: string | null;
    initialTab?: string;
  }

  let { info, error: connectError, asleep, initialTab }: Props = $props();

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
  let battery = $state<Battery | null>(null);

  function batteryText(b: Battery): string {
    if (b.state === "full") return "Charged";
    return `${b.estimated ? "About " : ""}${b.percent}%`;
  }

  const clone = <T,>(value: T): T => JSON.parse(JSON.stringify(value));
  const savedSettings = $derived(mouse?.profiles.find((p) => p.number === selected)?.settings ?? null);
  const dirty = $derived(!!draft && JSON.stringify(draft) !== JSON.stringify(savedSettings));
  const leftClickMissing = $derived(!!draft && !draft.buttons.some((b) => b.type === "mouse" && b.button === 1));

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

  let loadedFor: MouseInfo | null = null;
  $effect(() => {
    if (info && info !== loadedFor) {
      loadedFor = info;
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
    await revertPreview();
    selected = number;
    draft = clone(mouse!.profiles.find((p) => p.number === number)!.settings);
    message = null;
  }

  async function save() {
    if (!draft) return;
    const settings = clone(draft);
    const next = await run(`Saving profile ${selected} to the mouse…`, () => api.saveProfile(selected, settings));
    if (next) {
      previewing = false;
      adopt(next);
      message = `Profile ${selected} saved to the mouse.`;
    }
  }

  async function discard() {
    await revertPreview();
    draft = clone(savedSettings!);
  }

  async function factory() {
    const next = await run(`Restoring profile ${selected}…`, () => api.restoreFactory(selected));
    if (next) {
      previewing = false;
      adopt(next);
      message = `Profile ${selected} restored to factory settings.`;
    }
  }

  async function activate(number: number) {
    const next = await run("Switching profile…", () => api.setActiveProfile(number));
    if (next) {
      previewing = false;
      mouse = next;
    }
  }

  async function toggleEnabled(number: number, enabled: boolean) {
    const next = await run("Updating profiles…", () => api.setProfileEnabled(number, enabled));
    if (next) mouse = next;
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
{#if connectError && !info}<div class="notice error">{connectError}</div>{/if}
{#if info && !info.verified}
  <div class="notice info">
    <b>Experimental support.</b> PeriHub has not been tested on the {info.model}
    {info.receiver ? "through its LIGHTSPEED receiver" : ""} yet, so it only reads the mouse: you can look at its
    profiles, but nothing can be changed. A mouse test report on PeriHub's GitHub page{info.receiver
      ? " (issue #4)"
      : ""} helps get it fully supported.
  </div>
  {#if info.host_mode}
    <div class="notice info">
      Another program, such as G HUB, left this mouse under software control: it is not using the profiles shown here.
    </div>
  {/if}
{:else if info && !info.writable}
  <div class="notice error">This model uses a profile format that has not been verified yet: editing is disabled.</div>
{/if}
{#if info?.verified && info.host_mode}
  <div class="notice error">
    Another program controls the mouse right now, so it is not using the profiles shown here: buttons, macros and DPI
    levels may not work as saved. When that program is closed, PeriHub gives the mouse back its profiles within a few
    seconds.
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
            <button class="tiny" disabled={!!busy || !profile.enabled || !info.verified} onclick={() => activate(profile.number)}>
              Make active
            </button>
          {/if}
          <label class="row enabled" title="Disabled profiles are skipped by the profile switch button">
            <input
              type="checkbox"
              checked={profile.enabled}
              disabled={!!busy || !info.writable}
              onchange={(e) => toggleEnabled(profile.number, (e.target as HTMLInputElement).checked)}
            />
            Enabled
          </label>
        </div>
      </div>
    {/each}
  </div>

  {#if error}<div class="notice error">{error}</div>{/if}
  {#if message}<div class="notice ok">{message}</div>{/if}
  {#if busy}<div class="notice info">{busy}</div>{/if}

  <div class="tabs">
    <button class:active={tab === "performance"} onclick={() => changeTab("performance")}>DPI and polling</button>
    <button class:active={tab === "buttons"} onclick={() => changeTab("buttons")}>Buttons</button>
    <button class:active={tab === "lighting"} onclick={() => changeTab("lighting")}>Lighting</button>
  </div>

  <!-- Unverified models are read-only: every control in the editor is disabled. -->
  <fieldset disabled={!info.verified}>
    {#if tab === "performance"}
      <MousePerformance {info} bind:settings={draft} onpreview={() => (previewing = true)}
        activeLevel={selected === mouse.active_profile ? mouse.active_dpi_index : null} />
    {:else if tab === "buttons"}
      <MouseButtons {info} bind:settings={draft} profiles={mouse.profiles} profileNumber={selected}
        macroSpace={mouse.macro_space} />
    {:else}
      <MouseLighting {info} bind:settings={draft} onpreview={() => (previewing = true)} />
    {/if}
  </fieldset>

  <footer class="row">
    <ConfirmButton class="danger" disabled={!!busy || !info.writable} onconfirm={factory} confirmLabel="Reset this profile?">
      Factory settings
    </ConfirmButton>
    <div class="grow"></div>
    {#if leftClickMissing}
      <span class="warn small">No button does a left click any more: assign one before saving.</span>
    {:else if dirty}
      <span class="unsaved">Unsaved changes</span>
    {/if}
    <button disabled={!dirty || !!busy} onclick={discard}>Discard</button>
    <button class="primary" disabled={!dirty || !!busy || leftClickMissing || !info.writable} onclick={save}>
      Save to mouse
    </button>
  </footer>
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
</style>
