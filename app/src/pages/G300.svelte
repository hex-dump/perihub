<script lang="ts">
  // Logitech G300: its three profiles as stored in the mouse. A profile can be edited and saved, and
  // the save is undone unless it is kept.
  import { onDestroy } from "svelte";
  import {
    api, errorText, type G300Button, type G300Info, type G300Profile, type G300Settings, type KeyInfo,
  } from "../lib/api";
  import { keyLabel } from "../lib/keys";
  import { g300Test } from "../lib/testReport";
  import DeviceTestReport from "../components/DeviceTestReport.svelte";
  import G300ProfileEditor from "../components/G300ProfileEditor.svelte";
  import KeepChangesDialog from "../components/KeepChangesDialog.svelte";

  interface Props {
    info: G300Info | null;
    error: string | null;
  }

  let { info: initial, error: connectError }: Props = $props();

  /** The mouse's buttons change the active profile and DPI level. */
  const LIVE_MS = 2000;

  let live = $state<G300Info | null>(null);
  const info = $derived(live ?? initial);
  // The mouse was unplugged or opened again: what was read before is old. A new status for another
  // device changes nothing here.
  const opened = $derived(initial ? (initial.editable ? "editable" : "read-only") : "gone");
  $effect(() => {
    void opened;
    live = null;
    editing = null;
    confirmKeep = false;
  });

  const timer = setInterval(() => {
    if (!initial) return;
    api.g300Info().then((i) => (live = i)).catch(() => {});
  }, LIVE_MS);
  onDestroy(() => clearInterval(timer));

  /** The profile being edited. */
  let editing = $state<number | null>(null);
  /** A save waits to be kept or undone. */
  let confirmKeep = $state(false);
  let busy = $state<string | null>(null);
  let error = $state<string | null>(null);
  let message = $state<string | null>(null);
  let keys = $state<KeyInfo[]>([]);

  api.keyCatalog().then((c) => (keys = c.targets)).catch(() => {});

  async function run(label: string, task: () => Promise<void>) {
    busy = label;
    error = message = null;
    try {
      await task();
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = null;
    }
  }

  function save(number: number, settings: G300Settings) {
    return run("Saving to the mouse…", async () => {
      live = await api.g300Save(number, settings);
      editing = null;
      confirmKeep = true;
    });
  }

  async function keep() {
    confirmKeep = false;
    await api.g300KeepChanges().catch(() => {});
    message = "Changes kept.";
  }

  function undo() {
    confirmKeep = false;
    return run("Putting the backup back…", async () => {
      live = await api.g300UndoLastWrite();
      message = "Changes undone: the backup is back on the mouse.";
    });
  }

  /** G4 to G9 are printed on the mouse; 1 to 3 are the clicks. */
  const BUTTON_NAMES = ["Left button", "Right button", "Wheel button", "G4", "G5", "G6", "G7", "G8", "G9"];
  const MODIFIER_NAMES = ["Ctrl", "Shift", "Alt", "Win", "Right Ctrl", "Right Shift", "Right Alt", "Right Win"];
  const MOUSE_NAMES: Record<number, string> = {
    1: "Left click",
    2: "Right click",
    3: "Middle click",
    4: "Back",
    5: "Forward",
  };

  function action(b: G300Button): string {
    switch (b.type) {
      case "none":
        return "Nothing";
      case "mouse":
        return MOUSE_NAMES[b.button] ?? `Mouse button ${b.button}`;
      case "dpi_up":
        return "DPI +";
      case "dpi_down":
        return "DPI –";
      case "dpi_cycle":
        return "Cycle DPI";
      case "profile_cycle":
        return "Next profile";
      case "dpi_shift":
        return "DPI shift (hold)";
      case "dpi_default":
        return "Default DPI";
      case "key": {
        const mods = MODIFIER_NAMES.filter((_, i) => b.modifiers & (1 << i));
        return [...mods, ...(b.key ? [keyLabel(b.key)] : [])].join(" + ");
      }
      case "unknown":
        return `Unknown (${b.code.toString(16)})`;
    }
  }

  function colorName(color: string): string {
    const names: Record<string, string> = {
      "#ffffff": "White",
      "#ff0000": "Red",
      "#00ff00": "Green",
      "#0000ff": "Blue",
      "#ffff00": "Yellow",
      "#00ffff": "Cyan",
      "#ff00ff": "Magenta",
      "#000000": "Off",
    };
    return names[color.toLowerCase()] ?? color;
  }

  function describe(p: G300Profile): string {
    const levels = p.dpi_levels.map((d, i) => (i === p.default_level ? `${d}*` : `${d}`)).join(" / ");
    const buttons = p.buttons.map((b, i) => `${BUTTON_NAMES[i]}: ${action(b)}`).join("; ");
    const shift = p.shift_dpi ? `, shift ${p.shift_dpi}` : "";
    return `${colorName(p.color)}, ${p.report_rate_hz ?? "?"} Hz, DPI ${levels}${shift}, ${buttons}`;
  }
</script>

<header>
  <h1>Logitech G300</h1>
  {#if info && !info.editable}<span class="chip test">Read-only</span>{/if}
</header>

{#if connectError && !info}<div class="notice error">{connectError}</div>{/if}

{#if info?.editable}
  <div class="notice info">
    <b>Press Edit on a profile to change it.</b> PeriHub backs up the three profiles before every save, reads the save
    back and asks whether to keep it: without an answer the backup goes back after 15 seconds. Quit Logitech Gaming
    Software first, which keeps its own copy of the profiles. Saving was confirmed on a real G300; if something does
    not work on yours, please send a test report.
    <div class="row report">
      <DeviceTestReport fields={(version, system) => g300Test(info!, describe, version, system)} />
    </div>
  </div>
{:else if info}
  <div class="notice info">
    <b>Read-only.</b> PeriHub shows the three profiles stored in the G300 and changes nothing.
    {#if info.pending_undo}
      <p class="small"><b>A save to this mouse was not kept</b> before PeriHub closed or the mouse was unplugged.</p>
    {/if}
    <div class="row report">
      <DeviceTestReport fields={(version, system) => g300Test(info!, describe, version, system)} />
    </div>
  </div>
{/if}
{#if confirmKeep}
  <KeepChangesDialog model="G300" checkText="move it, try the buttons and DPI you changed" onkeep={keep} onundo={undo} />
{/if}
{#if busy}<div class="notice info">{busy}</div>{/if}
{#if error}<div class="notice error">{error}</div>{/if}
{#if message}<div class="notice ok">{message}</div>{/if}

{#if info}

  <div class="profiles">
    {#each info.profiles as profile (profile.number)}
      {@const active = profile.number === info.active_profile}
      <section class="card" class:active>
        <h2>
          Profile {profile.number}
          {#if active}<span class="chip ok">Active</span>{/if}
          {#if info.editable && editing === null}
            <button class="tiny edit" disabled={!!busy || confirmKeep} onclick={() => (editing = profile.number)}>Edit</button>
          {/if}
        </h2>
        {#if editing === profile.number}
          <G300ProfileEditor {profile} {keys} names={BUTTON_NAMES} describe={action} busy={!!busy}
            onsave={(settings) => save(profile.number, settings)} oncancel={() => (editing = null)} />
        {:else}
        <dl>
          <dt>Light</dt>
          <dd><span class="swatch" style:background={profile.color}></span>{colorName(profile.color)}</dd>
          <dt>Report rate</dt>
          <dd>{profile.report_rate_hz ? `${profile.report_rate_hz} Hz` : "unknown"}</dd>
          <dt>DPI levels</dt>
          <dd>
            {#each profile.dpi_levels as dpi, i (i)}
              <span class="level" class:current={active && i === info.active_level}>
                {dpi}{#if i === profile.default_level}<span title="Default level">*</span>{/if}
              </span>
            {/each}
          </dd>
          <dt>DPI shift</dt>
          <dd>{profile.shift_dpi ? `${profile.shift_dpi} DPI` : "Off"}</dd>
        </dl>
        <table>
          <tbody>
            {#each profile.buttons as button, i (i)}
              <tr><th>{BUTTON_NAMES[i]}</th><td>{action(button)}</td></tr>
            {/each}
          </tbody>
        </table>
        {/if}
      </section>
    {/each}
  </div>
  <p class="small muted">* default DPI level. The highlighted level is the one in use.</p>
{:else if !connectError}
  <div class="notice info">The G300 is not connected.</div>
{/if}

<style>
  header {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-bottom: 16px;
  }

  h1 {
    margin: 0;
  }

  .edit {
    margin-left: auto;
  }

  h2 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 0 12px;
    font-size: 1rem;
  }

  .report {
    margin-top: 10px;
  }

  .profiles {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
    gap: 16px;
    margin-top: 16px;
  }

  .card.active {
    border-color: var(--ok);
  }

  dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 6px 16px;
    margin: 0 0 12px;
  }

  dt,
  th {
    color: var(--muted);
    font-weight: normal;
    text-align: left;
  }

  dd {
    margin: 0;
  }

  .swatch {
    display: inline-block;
    width: 12px;
    height: 12px;
    margin-right: 6px;
    border-radius: 3px;
    border: 1px solid var(--line-strong);
    vertical-align: -1px;
  }

  .level {
    margin-right: 8px;
  }

  .level.current {
    color: var(--ok);
    font-weight: 700;
  }

  table {
    width: 100%;
    border-collapse: collapse;
  }

  th,
  td {
    padding: 3px 0;
  }

  th {
    width: 45%;
  }
</style>
