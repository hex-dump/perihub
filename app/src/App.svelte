<script lang="ts">
  import { onMount } from "svelte";
  import { api, type DeviceStatus } from "./lib/api";
  import { updates } from "./lib/updates.svelte";
  import Keyboard from "./pages/Keyboard.svelte";
  import AppProfiles from "./pages/AppProfiles.svelte";
  import Mouse from "./pages/Mouse.svelte";
  import Settings from "./pages/Settings.svelte";
  import Icon from "./components/Icon.svelte";

  type Page = "keyboard" | "mouse" | "profiles" | "settings";
  const PAGES: Page[] = ["keyboard", "mouse", "profiles", "settings"];

  // Navigation through the URL hash (#page or #page:tab), so a page can be opened directly.
  function fromHash(): [Page, string | undefined] {
    const [p, tab] = location.hash.slice(1).split(":");
    return [PAGES.includes(p as Page) ? (p as Page) : "keyboard", tab || undefined];
  }

  let [initialPage, initialTab] = fromHash();
  let page = $state<Page>(initialPage);
  let tab = $state<string | undefined>(initialTab);

  function go(next: Page) {
    page = next;
    tab = undefined;
    history.replaceState(null, "", `#${next}`);
  }
  let status = $state<DeviceStatus>({
    keyboard: null,
    mouse: null,
    keyboard_error: null,
    mouse_error: null,
    mouse_asleep: null,
    unsupported_keyboard: null,
    conflicts: [],
  });

  onMount(() => {
    api.status().then((s) => (status = s));
    // PeriHub checks for updates in the background, also while this window is closed.
    api.updateAvailable().then((u) => u && (updates.available = u));
    const unlistenUpdate = api.onUpdate((u) => (updates.available = u));
    const onHash = () => ([page, tab] = fromHash());
    window.addEventListener("hashchange", onHash);
    const unlisten = api.onStatus((s) => (status = s));
    return () => {
      unlistenUpdate.then((fn) => fn());
      unlisten.then((fn) => fn());
      window.removeEventListener("hashchange", onHash);
    };
  });
</script>

<div class="shell">
  <nav>
    <div class="brand">
      <span class="logo"></span>
      <span>PeriHub</span>
    </div>

    <h3>Devices</h3>
    <button class:active={page === "keyboard"} onclick={() => go("keyboard")}>
      <Icon name="keyboard" />
      <span class="col tight">
        <span class="name">Apex 3 TKL</span>
        <span class="state" class:on={!!status.keyboard}>{status.keyboard ? "Connected" : "Not connected"}</span>
      </span>
    </button>
    <button class:active={page === "mouse"} onclick={() => go("mouse")}>
      <Icon name="mouse" />
      <span class="col tight">
        <span class="name">{status.mouse?.model ?? status.mouse_asleep ?? "Logitech mouse"}</span>
        <span class="state" class:on={!!status.mouse}>
          {status.mouse ? "Connected" : status.mouse_asleep ? "Asleep" : "Not connected"}
        </span>
      </span>
    </button>

    <div class="grow"></div>
    <button class:active={page === "profiles"} onclick={() => go("profiles")}>
      <Icon name="profiles" />
      <span class="name">App profiles</span>
    </button>
    <button class:active={page === "settings"} onclick={() => go("settings")}>
      <Icon name="settings" />
      <span class="name">Settings</span>
      {#if updates.available}<span class="chip accent update">Update</span>{/if}
    </button>
  </nav>

  <main>
    <div class="content">
      {#each status.conflicts as name (name)}
        <div class="notice error conflict">
          <b>{name} is running.</b> It can take control of your devices when it starts and after every wake-up, so the
          mouse stops using its onboard profiles, buttons and macros. Quit it from its tray icon and turn off its
          autostart (Task Manager → Startup apps).
        </div>
      {/each}
      {#if page === "keyboard"}
        {#key tab}<Keyboard
            info={status.keyboard}
            error={status.keyboard_error}
            unsupported={status.unsupported_keyboard}
            initialTab={tab}
          />{/key}
      {:else if page === "mouse"}
        {#key tab}<Mouse info={status.mouse} error={status.mouse_error} asleep={status.mouse_asleep} initialTab={tab} />{/key}
      {:else if page === "profiles"}
        <AppProfiles {status} />
      {:else}
        <Settings {status} />
      {/if}
    </div>
  </main>
</div>

<style>
  .shell {
    display: grid;
    grid-template-columns: 232px 1fr;
    height: 100vh;
  }

  nav {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 20px 12px 16px;
    background: var(--panel);
    border-right: 1px solid var(--line);
  }

  nav h3 {
    margin: 22px 12px 6px;
  }

  nav button {
    position: relative;
    display: flex;
    align-items: center;
    gap: 12px;
    text-align: left;
    background: transparent;
    border: 1px solid transparent;
    padding: 10px 12px;
    color: var(--muted);
  }

  nav button:hover:not(:disabled) {
    background: var(--panel-2);
    border-color: transparent;
    color: var(--text);
  }

  nav button.active,
  nav button.active:hover:not(:disabled) {
    background: var(--panel-3);
    border-color: var(--line-strong);
    color: var(--text);
  }

  /* The page on screen: an orange edge and icon. */
  nav button.active::before {
    content: "";
    position: absolute;
    left: -12px;
    top: 10px;
    bottom: 10px;
    width: 3px;
    border-radius: 0 3px 3px 0;
    background: var(--accent);
  }

  nav button.active :global(svg) {
    color: var(--accent);
  }

  nav button :global(svg) {
    flex: none;
  }

  .name {
    font-weight: 600;
    color: var(--text);
  }

  .tight {
    gap: 1px;
  }

  .state {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--faint);
  }

  .state::before {
    content: "";
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: #4a4f5e;
  }

  .state.on {
    color: var(--ok);
  }

  .state.on::before {
    background: var(--ok);
    box-shadow: 0 0 6px var(--ok);
  }

  .conflict {
    margin-bottom: 18px;
  }

  .update {
    margin-left: auto;
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    font-weight: 700;
    font-size: 18px;
    letter-spacing: -0.01em;
    padding: 0 10px;
  }

  .logo {
    width: 24px;
    height: 24px;
    border-radius: 7px;
    background: linear-gradient(135deg, #ff5a1f, #d63bff 55%, #2f8bff);
  }

  main {
    --pad-x: 32px;
    --pad-y: 28px;
    overflow: auto;
    padding: var(--pad-y) var(--pad-x);
  }

  /* Narrow windows (down to the minimum, 960 px) keep more room for the page. */
  @media (max-width: 1100px) {
    .shell {
      grid-template-columns: 204px 1fr;
    }

    main {
      --pad-x: 24px;
      --pad-y: 22px;
    }
  }

  .content {
    max-width: 1080px;
    margin: 0 auto;
  }
</style>
