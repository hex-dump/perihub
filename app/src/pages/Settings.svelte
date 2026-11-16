<script lang="ts">
  import { onMount } from "svelte";
  import { getVersion } from "@tauri-apps/api/app";
  import { api, errorText, type AppSettings, type DeviceStatus } from "../lib/api";
  import { INSTALL_NOTE, checkForUpdate, installUpdate, installingText, updates } from "../lib/updates.svelte";
  import TestReport from "../components/TestReport.svelte";
  import SupportedDevices from "../components/SupportedDevices.svelte";
  import { ACCENTS, THEMES } from "../lib/theme";
  import { prefs, setAccent, setTheme } from "../lib/prefs.svelte";

  interface Props {
    status: DeviceStatus;
  }

  let { status }: Props = $props();
  let settings = $state<AppSettings | null>(null);
  let error = $state<string | null>(null);
  let version = $state("");
  let showCredits = $state(false);

  // Autostart can also change outside PeriHub, for example in Task Manager.
  async function refresh() {
    settings = await api.appSettings();
  }

  onMount(() => {
    getVersion().then((v) => (version = v));
    refresh();
    window.addEventListener("focus", refresh);
    return () => window.removeEventListener("focus", refresh);
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

  <div class="card col">
    <h2>Appearance</h2>
    <div class="row wrap">
      <span class="label" id="theme-label">Theme</span>
      <div class="segmented" role="group" aria-labelledby="theme-label">
        {#each THEMES as theme (theme.id)}
          <button class:active={prefs.theme === theme.id} aria-pressed={prefs.theme === theme.id}
            onclick={() => setTheme(theme.id)}>{theme.label}</button>
        {/each}
      </div>
    </div>
    <div class="row wrap">
      <span class="label" id="accent-label">Accent color</span>
      <div class="row swatches" role="radiogroup" aria-labelledby="accent-label">
        {#each ACCENTS as accent (accent.id)}
          <input type="radio" name="accent" class="swatch" style:--swatch={accent.swatch} value={accent.id}
            aria-label={accent.label} title={accent.label} checked={prefs.accent === accent.id}
            onchange={() => setAccent(accent.id)} />
        {/each}
      </div>
    </div>
    <p class="muted small">
      System follows the light or dark setting of your computer and changes with it. The accent color is used for
      the main buttons, the selection and the value in use.
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
            {updates.installing ? installingText(updates.progress) : "Update and restart"}
          </button>
        {:else}
          <button disabled={updates.checking} onclick={checkForUpdate}>
            {updates.checking ? "Checking…" : "Check now"}
          </button>
          {#if updates.checked}<span class="muted small">PeriHub {version} is the latest version.</span>{/if}
        {/if}
      </div>
      {#if updates.installing}<p class="muted small">{INSTALL_NOTE}</p>{/if}
      {#if updates.error}<div class="notice error">{updates.error}</div>{/if}
      <p class="muted small">
        PeriHub asks GitHub for the latest release: nothing else is sent. When it finds a new version it downloads it
        right away, so the update starts at once, but installs it only when you click “Update and restart”, after its
        signature is checked. The installer shows no window: PeriHub closes and opens again by itself. Your settings and the devices' memory are not touched.
      </p>
    </div>
  {/if}

  <div class="card col">
    <h2>Where settings are stored</h2>
    <ul class="small">
      <li><b>In the device</b>: a mouse keeps its DPI, polling, buttons, macros and lighting in its own memory, and the Apex keeps its key mapping. They work without PeriHub. The Apex does not keep its lighting: PeriHub reapplies it.</li>
      <li>
        <b>PeriHub's settings</b>: <code>config.json</code> in the settings folder. It holds the keyboard lighting, saved lighting, app profiles, macros run by PeriHub and the options on this page.
        {#if settings}<br /><code>{settings.config_dir}</code>{/if}
      </li>
      <li>
        <b>Backups</b>: before every write to a device, PeriHub saves what it is about to replace in the <code>backups</code> folder inside the settings folder, one folder per device. They are never deleted. While an experimental save is neither kept nor undone, a <code>pending-undo</code> file in the same folder lets PeriHub undo it after a restart.
      </li>
      <li>
        <b>Log</b>: <code>perihub.log</code>, and <code>perihub.old.log</code> once it passes 1 MB, in the log folder (see Problems below).
      </li>
      <li><b>Macro files</b>: only where you choose, when you export a macro. PeriHub keeps no other copy.</li>
      <li>
        <b>Start with the system</b>:
        {#if status.sandboxed}
          the desktop's background permission, asked through the Flatpak portal. PeriHub also keeps an <code>autostart-requested</code> file in the settings folder.
        {:else if settings?.system.startsWith("windows")}
          a PeriHub value under <code>HKCU\Software\Microsoft\Windows\CurrentVersion\Run</code> in the registry. Task Manager can turn it off.
        {:else}
          an entry in <code>~/.config/autostart</code>.
        {/if}
      </li>
      {#if settings?.system.startsWith("windows")}
        <li><b>Updates</b>: a downloaded update waits in PeriHub's cache folder until you install it.</li>
      {:else}
        <li>
          <b>Linux permissions</b>: the udev rules, in <code>/etc/udev/rules.d</code> (<code>70-perihub.rules</code> comes with the package).
          The optional ones, <code>71</code> to <code>73</code>, are added only when you allow them in PeriHub{#if status.sandboxed}; in a Flatpak you run the commands PeriHub shows{/if}.
        </li>
      {/if}
    </ul>
  </div>

  <div class="card col">
    <h2>Devices</h2>
    <div class="small">
      <div>Apex 3 TKL: {status.keyboard ? `firmware ${status.keyboard.firmware}` : status.keyboard_error ?? "not connected"}</div>
      <div>{status.mouse?.model ?? "Mouse"}: {status.mouse ? status.mouse.firmware.map((f) => `${f.name} ${f.version}`).join(", ") : status.mouse_error ?? (status.mouse_asleep ? "asleep" : "not connected")}</div>
    </div>
    <SupportedDevices />
    <p class="small">Is your mouse or keyboard missing? <a href="#report">Send a report for it</a>.</p>
    <p class="muted small">
      Quit SteelSeries GG, Logitech G HUB and Logitech Gaming Software while using PeriHub: two programs talking to the same device get in each other's way.
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
    {#if status.mouse}
      <p class="small">
        Testing a mouse? The report below has what the test form asks about it: no key presses, apps or log lines.
      </p>
      <TestReport mouse={status.mouse} />
    {/if}
  </div>

  <div class="card col">
    <h2>About</h2>
    <div class="row wrap about">
      <div class="col about-text">
        <span>PeriHub {version}</span>
        <span class="small muted">Free and open source (MIT). It builds on the work of other projects: Credits lists every one, with its license.</span>
      </div>
      <button onclick={() => (showCredits = true)}>Credits</button>
    </div>
  </div>

  <div>
    <button class="danger" onclick={() => api.quit()}>Quit PeriHub</button>
  </div>
</div>

<!-- Loaded when first opened: most people never open the credits. -->
{#if showCredits}
  {#await import("../components/CreditsDialog.svelte") then { default: CreditsDialog }}
    <CreditsDialog onclose={() => (showCredits = false)} />
  {/await}
{/if}

<style>
  .about { justify-content: space-between; align-items: center; }
  .about-text { gap: 2px; flex: 1 1 260px; }
  h1 {
    margin-bottom: 18px;
  }

  .gap {
    gap: 16px;
  }

  .label {
    min-width: 110px;
  }

  .swatches {
    gap: 12px;
  }

  /* A round swatch of the accent; the ring says which one is in use, and the keyboard focus has its own. */
  .swatch {
    appearance: none;
    width: 24px;
    height: 24px;
    border-radius: 50%;
    background: var(--swatch);
    cursor: pointer;
    box-shadow: 0 0 0 1px var(--line-strong);
  }

  .swatch:checked {
    box-shadow: 0 0 0 2px var(--panel), 0 0 0 4px var(--text);
  }

  ul {
    margin: 0;
    padding-left: 18px;
    line-height: 1.7;
  }
</style>
