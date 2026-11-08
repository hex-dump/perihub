<script lang="ts">
  // Logitech G300: its three profiles as stored in the mouse, read only.
  import { onDestroy } from "svelte";
  import { getVersion } from "@tauri-apps/api/app";
  import { api, errorText, type G300Button, type G300Info } from "../lib/api";
  import { keyLabel } from "../lib/keys";

  interface Props {
    info: G300Info | null;
    error: string | null;
  }

  let { info: initial, error: connectError }: Props = $props();

  const COPIED_MS = 3000;
  /** The mouse's buttons change the active profile and DPI level. */
  const LIVE_MS = 2000;

  let live = $state<G300Info | null>(null);
  let error = $state<string | null>(null);
  let copied = $state(false);
  const info = $derived(live ?? initial);

  const timer = setInterval(() => {
    if (!initial) return;
    api.g300Info().then((i) => (live = i)).catch(() => {});
  }, LIVE_MS);
  onDestroy(() => clearInterval(timer));

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

  async function copyReport() {
    if (!info) return;
    try {
      const system = (await api.appSettings().catch(() => null))?.system ?? "?";
      const lines = [
        `PeriHub ${await getVersion()} on ${system}`,
        `Mouse: Logitech G300, USB ID 046d:c246`,
        `Active profile: ${info.active_profile}, DPI level ${info.active_level + 1} (${info.active_raw})`,
        ...info.profiles.map(
          (p) =>
            `Profile ${p.number}: ${colorName(p.color)}, ${p.report_rate_hz ?? "?"} Hz, DPI ${p.dpi_levels
              .map((d, i) => (i === p.default_level ? `${d}*` : `${d}`))
              .join(" / ")}, ${p.buttons.map((b, i) => `${BUTTON_NAMES[i]}: ${action(b)}`).join("; ")}\n  ${p.raw}`,
        ),
      ];
      await navigator.clipboard.writeText(lines.join("\n"));
      copied = true;
      setTimeout(() => (copied = false), COPIED_MS);
    } catch (e) {
      error = errorText(e);
    }
  }
</script>

<header>
  <h1>Logitech G300</h1>
  <span class="chip test">Read-only</span>
</header>

{#if connectError && !info}<div class="notice error">{connectError}</div>{/if}

{#if info}
  <div class="notice info">
    <b>Read-only support.</b> PeriHub shows the three profiles stored in the G300 and changes nothing. Please check that
    they match Logitech Gaming Software and tell us on GitHub (issue #16): it is how editing can come next.
    <div class="row report">
      <button class="tiny" onclick={copyReport}>{copied ? "Copied" : "Copy report"}</button>
    </div>
  </div>
  {#if error}<div class="notice error">{error}</div>{/if}

  <div class="profiles">
    {#each info.profiles as profile (profile.number)}
      {@const active = profile.number === info.active_profile}
      <section class="card" class:active>
        <h2>
          Profile {profile.number}
          {#if active}<span class="chip ok">Active</span>{/if}
        </h2>
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
        </dl>
        <table>
          <tbody>
            {#each profile.buttons as button, i (i)}
              <tr><th>{BUTTON_NAMES[i]}</th><td>{action(button)}</td></tr>
            {/each}
          </tbody>
        </table>
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
