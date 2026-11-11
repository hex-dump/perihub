<script lang="ts">
  import { onMount } from "svelte";
  import { api, type DeviceStatus } from "./lib/api";
  import { updates } from "./lib/updates.svelte";
  import Keyboard from "./pages/Keyboard.svelte";
  import AppProfiles from "./pages/AppProfiles.svelte";
  import Mouse from "./pages/Mouse.svelte";
  import CorsairDevice from "./pages/CorsairDevice.svelte";
  import LogitechDevice from "./pages/LogitechDevice.svelte";
  import { kindIcon, kindName } from "./lib/logitechFeatures";
  import type { ReceiverReport, UnsupportedReceiverDevice } from "./lib/api";
  import G300 from "./pages/G300.svelte";
  import Settings from "./pages/Settings.svelte";
  import Icon from "./components/Icon.svelte";

  type Page = "keyboard" | "mouse" | "g300" | "corsair" | "logitech" | "profiles" | "settings";
  const PAGES: Page[] = ["keyboard", "mouse", "g300", "corsair", "logitech", "profiles", "settings"];

  // Navigation through the URL hash (#page or #page:tab), so a page can be opened directly.
  function fromHash(): [Page, string | undefined] {
    const [p, tab] = location.hash.slice(1).split(":");
    return [PAGES.includes(p as Page) ? (p as Page) : "keyboard", tab || undefined];
  }

  let [initialPage, initialTab] = fromHash();
  let page = $state<Page>(initialPage);
  let tab = $state<string | undefined>(initialTab);

  function go(next: Page, nextTab?: string) {
    page = next;
    tab = nextTab;
    history.replaceState(null, "", nextTab ? `#${next}:${nextTab}` : `#${next}`);
  }

  let status = $state<DeviceStatus>({
    keyboard: null,
    mouse: null,
    keyboard_error: null,
    mouse_error: null,
    mouse_asleep: null,
    unsupported_keyboard: null,
    conflicts: [],
    corsair: [],
    g300: null,
    g300_error: null,
    corsair_error: null,
    receivers: [],
    logitech_programs: [],
  });

  const hex = (id: number) => id.toString(16).padStart(4, "0");
  /** Logitech devices behind a receiver that PeriHub only reads, each with a page of its own. */
  const logitechDevices = $derived(
    status.receivers.flatMap((receiver) => receiver.unsupported.map((device) => ({ receiver, device }))),
  );
  const logitechKey = (receiver: ReceiverReport, device: UnsupportedReceiverDevice) =>
    `${hex(receiver.product_id)}-${device.slot}`;
  /** The one on screen: the one in the address, or the first one. */
  const logitechShown = $derived(
    logitechDevices.find((d) => logitechKey(d.receiver, d.device) === tab) ??
      (page === "logitech" ? logitechDevices[0] : undefined) ??
      null,
  );

  /** The Corsair device on screen: the one in the address, or the first one. */
  const corsairShown = $derived(
    status.corsair.find((d) => hex(d.product_id) === tab) ?? (page === "corsair" ? status.corsair[0] : undefined) ?? null,
  );

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

    {#if status.g300 || status.g300_error || page === "g300"}
      <button class:active={page === "g300"} onclick={() => go("g300")}>
        <Icon name="mouse" />
        <span class="col tight">
          <span class="name">Logitech G300</span>
          <span class="state" class:on={!!status.g300}>{status.g300 ? "Connected" : "Not connected"}</span>
        </span>
      </button>
    {/if}
    {#each logitechDevices as { receiver, device } (logitechKey(receiver, device))}
      <button
        class:active={page === "logitech" && logitechShown?.device === device}
        onclick={() => go("logitech", logitechKey(receiver, device))}
      >
        <Icon name={kindIcon(device.kind)} />
        <span class="col tight">
          <span class="name">Logitech {device.name ?? kindName(device.kind)}</span>
          {#if !receiver.open_slots.includes(device.slot)}
            <span class="state">Asleep</span>
          {:else if device.dpi}
            <span class="state on">Experimental</span>
          {:else}
            <span class="state on">Read-only</span>
          {/if}
        </span>
      </button>
    {/each}
    {#each status.corsair as device (device.product_id)}
      <button
        class:active={page === "corsair" && corsairShown?.product_id === device.product_id}
        onclick={() => go("corsair", hex(device.product_id))}
      >
        <Icon name={device.kind} />
        <span class="col tight">
          <span class="name">Corsair {device.model}</span>
          <span class="state on">{device.receiver ? "Experimental · wireless" : "Experimental"}</span>
        </span>
      </button>
    {/each}
    {#if status.corsair.length === 0 && (status.corsair_error || page === "corsair")}
      <button class:active={page === "corsair"} onclick={() => go("corsair")}>
        <Icon name="mouse" />
        <span class="col tight">
          <span class="name">Corsair</span>
          <span class="state">Not connected</span>
        </span>
      </button>
    {/if}

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
          {#if name === "Logitech Gaming Software"}
            <b>{name} is running.</b> It keeps its own copy of a mouse's profiles and may write it back to the mouse,
            or take control of it, while you use PeriHub. Quit it from its tray icon and turn off its autostart (Task
            Manager → Startup apps).
          {:else}
            <b>{name} is running.</b> It can take control of your devices when it starts and after every wake-up, so the
            mouse stops using its onboard profiles, buttons and macros. Quit it from its tray icon and turn off its
            autostart (Task Manager → Startup apps).
          {/if}
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
        {#key tab}<Mouse
            info={status.mouse}
            error={status.mouse_error}
            asleep={status.mouse_asleep}
            receivers={status.receivers}
            initialTab={tab}
          />{/key}
      {:else if page === "g300"}
        <G300 info={status.g300} error={status.g300_error} />
      {:else if page === "logitech"}
        {#key logitechShown ? logitechKey(logitechShown.receiver, logitechShown.device) : null}<LogitechDevice
            receiver={logitechShown?.receiver ?? null}
            device={logitechShown?.device ?? null}
            programs={status.logitech_programs}
          />{/key}
      {:else if page === "corsair"}
        {#key corsairShown?.product_id}<CorsairDevice info={corsairShown} error={status.corsair_error} />{/key}
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
