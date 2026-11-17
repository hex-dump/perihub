<script lang="ts">
  import { onMount } from "svelte";
  import { api, debounce, errorText, type AppProfiles, type DeviceStatus, type InstalledGame, type ProfilesInfo } from "../lib/api";

  interface Props {
    status: DeviceStatus;
  }

  let { status }: Props = $props();
  let info = $state<ProfilesInfo | null>(null);
  let profiles = $state<AppProfiles>({ enabled: false, rules: [] });
  let focused = $state<string | null>(null);
  let error = $state<string | null>(null);
  let settingUp = $state(false);
  let setupMessage = $state<string | null>(null);
  let copied = $state(false);
  let games = $state<InstalledGame[] | null>(null);
  let pickingGame = $state(false);
  let gameSearch = $state("");
  let gameMessage = $state<string | null>(null);

  /** PeriHub's own window: never a useful rule. */
  const SELF = ["perihub", "perihub.exe"];

  const save = debounce((p: AppProfiles) => {
    api.setProfiles(p).then(() => (error = null), (e) => (error = errorText(e)));
  }, 400);

  function changed() {
    save($state.snapshot(profiles));
  }

  function load() {
    return api.profiles().then((i) => {
      info = i;
      profiles = i.profiles;
      focused = i.focused;
    }, (e) => (error = errorText(e)));
  }

  /** E.g. on GNOME: installs and enables PeriHub's Shell extension. */
  async function setUp() {
    settingUp = true;
    error = null;
    try {
      setupMessage = await api.setupFocus();
      await load();
    } catch (e) {
      error = errorText(e);
    } finally {
      settingUp = false;
    }
  }

  // Browsers allow writing to the clipboard only straight after the click: no waiting before it.
  async function copyCommand(command: string) {
    try {
      await navigator.clipboard.writeText(command);
      error = null;
      copied = true;
      setTimeout(() => (copied = false), 2000);
    } catch {
      error = "The command could not be copied: select it and copy it by hand.";
    }
  }

  onMount(() => {
    load();
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

  async function openGames() {
    pickingGame = !pickingGame;
    gameMessage = null;
    if (!pickingGame || games) return;
    try {
      games = await api.installedGames();
    } catch (e) {
      error = errorText(e);
      pickingGame = false;
    }
  }

  function addGame(game: InstalledGame) {
    if (profiles.rules.some((r) => r.app.toLowerCase() === game.app.toLowerCase())) {
      gameMessage = `There is already a rule for ${game.app}.`;
      return;
    }
    profiles.rules.push({ app: game.app, lighting: null, mouse_profile: null });
    changed();
    gameMessage = game.certain
      ? `Rule added for ${game.name}.`
      : `Rule added for ${game.name}. The app name is a guess: check it against "App in focus" while the game runs.`;
  }

  function removeRule(i: number) {
    profiles.rules.splice(i, 1);
    changed();
  }

  const shownGames = $derived.by(() => {
    const q = gameSearch.trim().toLowerCase();
    return (games ?? []).filter((g) => !q || g.name.toLowerCase().includes(q) || g.app.toLowerCase().includes(q));
  });

  const value = (e: Event) => (e.target as HTMLInputElement | HTMLSelectElement).value;
</script>

<header class="page-head">
  <h1>App profiles</h1>
</header>

<p class="lead">
  Change the keyboard lighting and the mouse profile automatically while an app or a game is in focus.
  When you switch to an app without a rule, both go back to how they were.
</p>

<div class="col gap">

  {#if error}<div class="notice error" role="alert">{error}</div>{/if}

  {#if info}
    {#if setupMessage}<div class="notice info">{setupMessage}</div>{/if}
    {#if info.support.kind === "unsupported"}
      {#if !setupMessage}<div class="notice info">{info.support.reason}</div>{/if}
    {:else if info.support.kind === "setup"}
      <div class="card col">
        <p>{info.support.reason}</p>
        <div class="row">
          <button onclick={setUp} disabled={settingUp}>{info.support.action}</button>
        </div>
      </div>
    {:else if info.support.kind === "host_command"}
      {@const command = info.support.command}
      <div class="card col">
        <p>{info.support.reason}</p>
        <code class="command">{command}</code>
        <div class="row">
          <button onclick={() => copyCommand(command)}>{copied ? "Copied" : "Copy command"}</button>
        </div>
      </div>
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
          <p class="empty">No rules yet. Add one for an app or a game, or pick from your installed games.</p>
        {/if}
        <div class="row">
          <button onclick={addRule}>Add rule</button>
          <button onclick={openGames}>Add from installed games</button>
          <span class="small muted">The new rule starts with the last app you used: switch to the game, then come back here.</span>
        </div>
        {#if pickingGame}
          <div class="col games">
            <input type="search" placeholder="Search games" aria-label="Search games" bind:value={gameSearch} />
            {#if gameMessage}<div class="notice info">{gameMessage}</div>{/if}
            {#if !games}
              <p class="loading">Looking for games…</p>
            {:else if !games.length}
              <p class="muted small">No games found. PeriHub looks at Steam, Lutris and Heroic.</p>
            {:else if !shownGames.length}
              <p class="muted small">No game matches.</p>
            {:else}
              <ul class="game-list">
                {#each shownGames as game (game.launcher + game.app + game.name)}
                  <li>
                    <button class="ghost game" onclick={() => addGame(game)}>
                      <span>{game.name}</span>
                      <span class="small muted">{game.launcher} · <code>{game.app}</code>{game.certain ? "" : " · not sure"}</span>
                    </button>
                    {#if game.note}<span class="small muted">{game.note}</span>{/if}
                  </li>
                {/each}
              </ul>
            {/if}
          </div>
        {/if}
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
  {:else if !error}
    <p class="loading">Reading app profiles…</p>
  {/if}
</div>

<style>
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

  .game-list {
    list-style: none;
    margin: 0;
    padding: 0;
    max-height: 280px;
    overflow-y: auto;
  }

  .game {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    width: 100%;
    text-align: left;
  }

  .command {
    user-select: all;
    overflow-wrap: anywhere;
  }
</style>
