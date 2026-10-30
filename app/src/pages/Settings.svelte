<script lang="ts">
  import { onMount } from "svelte";
  import { getVersion } from "@tauri-apps/api/app";
  import { api, errorText, type AppSettings, type DeviceStatus } from "../lib/api";
  import { checkForUpdate, installUpdate, updates } from "../lib/updates.svelte";

  interface Props {
    status: DeviceStatus;
  }

  let { status }: Props = $props();
  let settings = $state<AppSettings | null>(null);
  let error = $state<string | null>(null);
  let version = $state("");

  onMount(async () => {
    version = await getVersion();
    settings = await api.appSettings();
  });

  async function update(change: Partial<AppSettings>, box: HTMLInputElement) {
    if (!settings) return;
    try {
      settings = await api.setAppSettings({ ...settings, ...change });
      error = null;
    } catch (e) {
      error = errorText(e);
      // Nothing changed, so the box goes back: `checked` alone would not redo it.
      box.checked = !box.checked;
    }
  }
</script>

<h1>Settings</h1>

<div class="col gap">
  {#if error}<div class="notice error">{error}</div>{/if}

  <div class="card col">
    <h2>Startup</h2>
    {#if settings}
      <label class="row">
        <input type="checkbox" checked={settings.autostart}
          onchange={(e) => update({ autostart: e.currentTarget.checked }, e.currentTarget)} />
        Start PeriHub when I log in (hidden in the system tray)
      </label>
      <label class="row">
        <input type="checkbox" checked={settings.start_hidden}
          onchange={(e) => update({ start_hidden: e.currentTarget.checked }, e.currentTarget)} />
        Always start hidden
      </label>
    {/if}
    <p class="muted small">
      Closing the window does not quit PeriHub: it keeps running to animate the keyboard lighting and to
      reapply colors when the keyboard is plugged back in. Reopen it from the tray icon or by launching it again.
    </p>
  </div>

  {#if settings?.package_manager === "Scoop"}
    <div class="card col">
      <h2>Updates</h2>
      <p>PeriHub was installed with Scoop, which also updates it: <code>scoop update perihub</code>.</p>
    </div>
  {:else if settings?.updates_supported}
    <div class="card col">
      <h2>Updates</h2>
      <label class="row">
        <input type="checkbox" checked={settings.check_updates}
          onchange={(e) => update({ check_updates: e.currentTarget.checked }, e.currentTarget)} />
        Look for a new version when PeriHub starts and once a day
      </label>
      <div class="row wrap">
        {#if updates.available}
          <span>PeriHub {updates.available.version} is available.</span>
          <button class="primary" disabled={updates.installing} onclick={installUpdate}>
            {updates.installing ? "Downloading…" : "Update and restart"}
          </button>
        {:else}
          <button disabled={updates.checking} onclick={checkForUpdate}>
            {updates.checking ? "Checking…" : "Check now"}
          </button>
          {#if updates.checked}<span class="muted small">PeriHub {version} is the latest version.</span>{/if}
        {/if}
      </div>
      {#if updates.error}<div class="notice error">{updates.error}</div>{/if}
      <p class="muted small">
        PeriHub asks GitHub for the latest release: nothing else is sent. Updates are installed only when you click
        “Update and restart”, after their signature is checked. Your settings and the devices' memory are not touched.
      </p>
    </div>
  {/if}

  <div class="card col">
    <h2>Where settings are stored</h2>
    <ul class="small">
      <li><b>Mouse</b>: DPI, polling, buttons and lighting are stored in the mouse's memory and work without PeriHub.</li>
      <li><b>Keyboard</b>: the key mapping is stored in the Apex's memory. Lighting is not kept by the keyboard, so PeriHub reapplies it.</li>
      <li>Before every write to a device, a backup is saved in the <code>perihub/backups</code> folder of your user configuration.</li>
    </ul>
  </div>

  <div class="card col">
    <h2>Devices</h2>
    <div class="small">
      <div>Apex 3 TKL: {status.keyboard ? `firmware ${status.keyboard.firmware}` : status.keyboard_error ?? "not connected"}</div>
      <div>{status.mouse?.model ?? "Mouse"}: {status.mouse ? status.mouse.firmware.map((f) => `${f.name} ${f.version}`).join(", ") : status.mouse_error ?? (status.mouse_asleep ? "asleep" : "not connected")}</div>
    </div>
    <p class="muted small">
      Quit SteelSeries GG and Logitech G HUB while using PeriHub: two programs talking to the same device get in each other's way.
    </p>
  </div>

  <div class="card col">
    <h2>Problems</h2>
    <p class="small">
      If something does not work, open an issue on GitHub and attach <code>perihub.log</code>. It records devices
      connecting, errors and updates, never your key presses or the apps you use.
    </p>
    {#if settings}
      <div class="row wrap">
        <code class="small">{settings.log_dir}</code>
        <button onclick={() => api.openLogFolder().catch((e) => (error = errorText(e)))}>Open log folder</button>
      </div>
    {/if}
  </div>

  <div class="card col">
    <h2>Credits</h2>
    <p class="small">PeriHub {version}</p>
    <p class="muted small">
      Protocols reconstructed thanks to the work of OpenRGB (Apex lighting), libratbag (Logitech HID++) and
      gorkemgun/apex-3-tkl (Apex key mapping). PeriHub is not affiliated with SteelSeries or Logitech.
    </p>
  </div>

  <div>
    <button class="danger" onclick={() => api.quit()}>Quit PeriHub</button>
  </div>
</div>

<style>
  h1 {
    margin-bottom: 18px;
  }

  .gap {
    gap: 16px;
  }

  ul {
    margin: 0;
    padding-left: 18px;
    line-height: 1.7;
  }
</style>
