<script lang="ts">
  import { onMount } from "svelte";
  import { api, debounce, errorText, type AppProfiles, type DeviceStatus, type ProfilesInfo } from "../lib/api";

  interface Props {
    status: DeviceStatus;
  }

  let { status }: Props = $props();
  let info = $state<ProfilesInfo | null>(null);
  let profiles = $state<AppProfiles>({ enabled: false, rules: [] });
  let focused = $state<string | null>(null);
  let error = $state<string | null>(null);

  /** PeriHub's own window: never a useful rule. */
  const SELF = ["perihub", "perihub.exe"];

  const save = debounce((p: AppProfiles) => {
    api.setProfiles(p).then(() => (error = null), (e) => (error = errorText(e)));
  }, 400);

  function changed() {
    save($state.snapshot(profiles));
  }

  onMount(() => {
    api.profiles().then((i) => {
      info = i;
      profiles = i.profiles;
      focused = i.focused;
    }, (e) => (error = errorText(e)));
    const unlisten = api.onFocus((app) => {
      focused = app;
      if (info && app && !SELF.includes(app)) info.recent = [app, ...info.recent.filter((a) => a !== app)].slice(0, 12);
    });
    return () => {
      save.cancel();
      unlisten.then((fn) => fn());
    };
  });

  const recentApps = $derived((info?.recent ?? []).filter((a) => !SELF.includes(a)));
  const mouse = $derived(status.mouse);
  const mouseProfiles = $derived(mouse?.verified ? mouse.onboard.profile_count : 0);

  function addRule() {
    const used = new Set(profiles.rules.map((r) => r.app.toLowerCase()));
    const app = recentApps.find((a) => !used.has(a)) ?? "";
    profiles.rules.push({ app, lighting: null, mouse_profile: null });
    changed();
  }

  function removeRule(i: number) {
    profiles.rules.splice(i, 1);
    changed();
  }

  const value = (e: Event) => (e.target as HTMLInputElement | HTMLSelectElement).value;
</script>

<h1>App profiles</h1>

<div class="col gap">
  <p class="muted">
    Change the keyboard lighting and the mouse profile automatically while an app or a game is in focus.
    When you switch to an app without a rule, both go back to how they were.
  </p>

  {#if error}<div class="notice error">{error}</div>{/if}

  {#if info}
    {#if info.support.kind === "unsupported"}
      <div class="notice info">{info.support.reason}</div>
    {:else}
      <div class="card col">
        <label class="row">
          <input type="checkbox" bind:checked={profiles.enabled} onchange={changed} />
          Switch automatically
        </label>
        <span class="small muted">
          App in focus: <code>{focused ?? "none"}</code>
          {info.support.desktop === "Windows" ? "(executable name)" : "(window class)"}
        </span>
      </div>

      <div class="card col">
        <h2>Rules</h2>
        {#if profiles.rules.length}
          <div class="rules">
            <span class="small muted">App</span>
            <span class="small muted">Keyboard lighting</span>
            <span class="small muted">Mouse profile</span>
            <span></span>
            {#each profiles.rules as rule, i (i)}
              <input list="recent-apps" placeholder={info.support.desktop === "Windows" ? "game.exe" : "window class"}
                value={rule.app} oninput={(e) => { rule.app = value(e); changed(); }} />
              <select value={rule.lighting ?? ""} onchange={(e) => { rule.lighting = value(e) || null; changed(); }}>
                <option value="">Don't change</option>
                {#each info.presets as name (name)}
                  <option value={name}>{name}</option>
                {/each}
              </select>
              <select value={rule.mouse_profile === null ? "" : String(rule.mouse_profile)} disabled={!mouseProfiles}
                onchange={(e) => { rule.mouse_profile = value(e) ? Number(value(e)) : null; changed(); }}>
                <option value="">Don't change</option>
                {#each Array.from({ length: mouseProfiles }, (_, n) => n + 1) as n (n)}
                  <option value={String(n)}>Profile {n}</option>
                {/each}
              </select>
              <button class="ghost" aria-label="Remove rule" title="Remove" onclick={() => removeRule(i)}>×</button>
            {/each}
          </div>
          <datalist id="recent-apps">
            {#each recentApps as app (app)}<option value={app}></option>{/each}
          </datalist>
        {:else}
          <p class="muted small">No rules yet.</p>
        {/if}
        <div class="row">
          <button onclick={addRule}>Add rule</button>
          <span class="small muted">The new rule starts with the last app you used: switch to the game, then come back here.</span>
        </div>
        {#if !info.presets.length}
          <p class="muted small">To change the keyboard lighting, first save a lighting on the Apex 3 TKL page (Lighting → Saved lightings).</p>
        {/if}
        {#if mouse && !mouse.verified}
          <p class="muted small">
            PeriHub switches profiles by itself only on mice whose memory format has been tested, so the {mouse.model}'s
            profile cannot be switched here yet.
          </p>
        {/if}
      </div>
    {/if}
  {/if}
</div>

<style>
  h1 {
    margin-bottom: 10px;
  }

  .gap {
    gap: 16px;
  }

  .rules {
    display: grid;
    grid-template-columns: minmax(160px, 1.4fr) 1fr 1fr auto;
    gap: 8px;
    align-items: center;
  }

  code {
    color: var(--text);
  }
</style>
