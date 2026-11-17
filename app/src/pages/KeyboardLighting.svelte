<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { cubicOut } from "svelte/easing";
  import { Tween } from "svelte/motion";
  import { api, debounce, errorText, type KeyboardLighting, type LightingPreset, type Rgb } from "../lib/api";
  import { ZONES } from "../lib/effects";
  import KeyAccessPermission from "../components/KeyAccessPermission.svelte";
  import ZoneStrip from "../components/ZoneStrip.svelte";
  import { readOnly } from "../lib/readOnly.svelte";

  interface Props {
    connected: boolean;
  }

  let { connected }: Props = $props();

  type Kind = KeyboardLighting["type"];
  const KINDS: { kind: Kind; label: string; hint: string }[] = [
    { kind: "static", label: "Static", hint: "One color per zone" },
    { kind: "gradient", label: "Gradient", hint: "From one color to another" },
    { kind: "breathing", label: "Breathing", hint: "Slow pulse" },
    { kind: "color_cycle", label: "Color cycle", hint: "All zones change together" },
    { kind: "wave", label: "Wave", hint: "Scrolling rainbow" },
    { kind: "reactive", label: "Reactive", hint: "Keys light up when pressed" },
    { kind: "hardware_rainbow", label: "Built-in rainbow", hint: "Works even when PeriHub is closed" },
    { kind: "off", label: "Off", hint: "Lights off" },
  ];

  const PRESETS: Rgb[] = ["#ff5200", "#ff0000", "#ffdc00", "#00ff6a", "#00dcff", "#2f5bff", "#8000ff", "#ff00b4", "#ffffff"];

  let loaded = $state(false);
  let kind = $state<Kind>("static");
  let zones = $state<Rgb[]>(Array(ZONES).fill("#ff5200"));
  let from = $state<Rgb>("#ff0000");
  let to = $state<Rgb>("#2f5bff");
  let period = $state(3000);
  let reverse = $state(false);
  let base = $state<Rgb>("#100818");
  let flash = $state<Rgb>("#ff5200");
  let fade = $state(600);
  /** Error shown instead of the generic one while the permission is missing. */
  const NO_KEY_ACCESS = "key presses cannot be read";
  /** Keyboard brightness level, 0–16. */
  let brightness = $state(16);
  /** Slider position: follows `brightness` smoothly and moves freely while dragged. */
  const brightnessShown = new Tween(16, { duration: 200, easing: cubicOut });
  let error = $state<string | null>(null);
  /** When the slider was last moved, so polling does not fight with the user. */
  let brightnessTouched = 0;
  /** Saved lightings, for app profiles. */
  let saved = $state<LightingPreset[]>([]);
  let presetName = $state("");

  const lighting = $derived.by((): KeyboardLighting => {
    switch (kind) {
      case "static": return { type: "static", colors: [...zones] };
      case "gradient": return { type: "gradient", from, to };
      case "breathing": return { type: "breathing", colors: [...zones], period_ms: period };
      case "color_cycle": return { type: "color_cycle", period_ms: period };
      case "wave": return { type: "wave", period_ms: period, reverse };
      case "reactive": return { type: "reactive", base, flash, fade_ms: fade };
      case "hardware_rainbow": return { type: "hardware_rainbow" };
      case "off": return { type: "off" };
    }
  });

  const pushLighting = debounce((l: KeyboardLighting) => {
    api.setLighting(l).then(
      () => (error = null),
      (e) => (error = l.type === "reactive" && errorText(e).includes(NO_KEY_ACCESS) ? null : errorText(e)),
    );
  }, 60);

  const pushBrightness = debounce((level: number) => {
    api.setBrightness(level).then(() => (error = null), (e) => (error = errorText(e)));
  }, 60);

  /** Lighting the keyboard already shows: sending it again would restart the effect (a visible
   * glitch) and undo a lighting set by an app profile. */
  let applied = "";

  $effect(() => {
    const l = lighting;
    const json = JSON.stringify(l);
    if (loaded && json !== applied) {
      applied = json;
      pushLighting(l);
    }
  });

  /** Shows a lighting in the editor (which then applies it). */
  function show(l: KeyboardLighting) {
    kind = l.type;
    if (l.type === "static" || l.type === "breathing") zones = [...l.colors];
    if (l.type === "gradient") {
      from = l.from;
      to = l.to;
    }
    if ("period_ms" in l) period = l.period_ms;
    if (l.type === "wave") reverse = l.reverse;
    if (l.type === "reactive") {
      base = l.base;
      flash = l.flash;
      fade = l.fade_ms;
    }
  }

  async function savePreset() {
    try {
      saved = await api.savePreset(presetName, lighting);
      presetName = "";
      error = null;
    } catch (e) {
      error = errorText(e);
    }
  }

  async function deletePreset(name: string) {
    try {
      saved = await api.deletePreset(name);
    } catch (e) {
      error = errorText(e);
    }
  }

  onMount(async () => {
    const config = await api.keyboardConfig();
    show(config.lighting);
    applied = JSON.stringify(lighting);
    saved = await api.presets();
    brightness = config.brightness;
    // Brightness can also be changed from the keyboard (SteelSeries + F11/F12): read the actual value.
    if (connected) {
      try {
        brightness = await api.readBrightness();
      } catch {
        /* keep the saved value */
      }
    }
    brightnessShown.set(brightness, { duration: 0 });
    loaded = true;
  });

  // Brightness can change at any time from the keyboard: follow it while this page is in front
  // (it is read again as soon as the window gets the focus back).
  const brightnessPoll = setInterval(async () => {
    if (!connected || document.hidden || !document.hasFocus() || Date.now() - brightnessTouched < 1500) return;
    try {
      const level = await api.readBrightness();
      if (Date.now() - brightnessTouched >= 1500 && level !== brightness) {
        brightness = level;
        brightnessShown.target = level;
      }
    } catch {
      /* keyboard busy or unplugged: try again later */
    }
  }, 250);
  onDestroy(() => clearInterval(brightnessPoll));

  function setAll(color: Rgb) {
    zones = Array(ZONES).fill(color);
  }

  // The keyboard has 17 levels: the slider moves freely and a new level is sent when it is crossed.
  function onBrightness(event: Event) {
    brightnessTouched = Date.now();
    const position = Number((event.target as HTMLInputElement).value);
    brightnessShown.set(position, { duration: 0 });
    const level = Math.round(position);
    if (level !== brightness) {
      brightness = level;
      pushBrightness(level);
    }
  }

  /** On release, the slider settles on the level the keyboard actually uses. */
  function onBrightnessEnd() {
    brightnessShown.target = brightness;
  }

  const usesZones = $derived(kind === "static" || kind === "breathing");
  const usesPeriod = $derived(kind === "breathing" || kind === "color_cycle" || kind === "wave");
</script>

<div class="col gap">
  <ZoneStrip {lighting} brightness={brightnessShown.current} />

  {#if error}<div class="notice error">{error}</div>{/if}

  <!-- Read-only mode: the lighting is shown, nothing can be changed. -->
  <fieldset class="guard" disabled={readOnly.on}>
  <div class="kinds">
    {#each KINDS as k (k.kind)}
      <button class="kind" class:active={kind === k.kind} onclick={() => (kind = k.kind)}>
        <strong>{k.label}</strong>
        <span class="small muted">{k.hint}</span>
      </button>
    {/each}
  </div>

  <div class="card col">
    {#if usesZones}
      <h2>Zone colors <span class="muted small">left to right</span></h2>
      <div class="zones">
        {#each zones as _, i (i)}
          <label class="zone">
            <input type="color" bind:value={zones[i]} />
            <span class="small muted">Zone {i + 1}</span>
          </label>
        {/each}
      </div>
      <div class="row wrap">
        <span class="small muted">All zones:</span>
        {#each PRESETS as preset (preset)}
          <button class="swatch" style:background={preset} aria-label="Color {preset}" onclick={() => setAll(preset)}></button>
        {/each}
        <input type="color" value={zones[0]} oninput={(e) => setAll((e.target as HTMLInputElement).value)} />
      </div>
    {/if}

    {#if kind === "gradient"}
      <div class="row">
        <label class="field">Start color <input type="color" bind:value={from} /></label>
        <span class="muted">→</span>
        <label class="field">End color <input type="color" bind:value={to} /></label>
      </div>
    {/if}

    {#if usesPeriod}
      <label class="field">
        Cycle duration: {(period / 1000).toFixed(1)} s
        <input type="range" min="500" max="15000" step="100" bind:value={period} />
      </label>
    {/if}

    {#if kind === "reactive"}
      <div class="row">
        <label class="field">Background <input type="color" bind:value={base} /></label>
        <label class="field">Key press <input type="color" bind:value={flash} /></label>
      </div>
      <label class="field">
        Fade: {(fade / 1000).toFixed(1)} s
        <input type="range" min="100" max="3000" step="50" bind:value={fade} />
      </label>
      <p class="muted small">
        The zone of each key you press lights up. PeriHub only uses key presses to light the keyboard while this
        effect is on: they are never stored or sent anywhere.
      </p>
      <!-- With the permission granted, the effect starts reading keys once it is applied again. -->
      <KeyAccessPermission
        title="Reactive lighting needs permission to read what you type on this keyboard."
        ongranted={() => api.setLighting(lighting).catch((e) => (error = errorText(e)))}
      />
    {/if}

    {#if kind === "wave"}
      <label class="row small"><input type="checkbox" bind:checked={reverse} /> Reverse direction (right to left)</label>
    {/if}

    {#if kind === "hardware_rainbow"}
      <p class="muted small">Generated by the keyboard itself: it keeps running even if you close PeriHub.</p>
    {:else if kind !== "off" && kind !== "static" && kind !== "gradient" && kind !== "reactive"}
      <p class="muted small">Animated effects are rendered by PeriHub: keep it running in the system tray.</p>
    {/if}

    <label class="field">
      Brightness: {Math.round((brightness / 16) * 100)}%
      <input type="range" min="0" max="16" step="any" value={brightnessShown.current}
        oninput={onBrightness} onchange={onBrightnessEnd} />
    </label>
  </div>
  </fieldset>

  <div class="card col">
    <h2>Saved lightings</h2>
    <p class="muted small">Save the current lighting to use it in app profiles. Click a saved lighting to show it again.</p>
    {#if saved.length}
      <div class="row wrap">
        {#each saved as preset (preset.name)}
          <span class="chip">
            <button class="ghost tight" disabled={readOnly.on} onclick={() => show(preset.lighting)}>{preset.name}</button>
            <button class="ghost tight" aria-label="Delete {preset.name}" title="Delete" onclick={() => deletePreset(preset.name)}>×</button>
          </span>
        {/each}
      </div>
    {/if}
    <form class="row" onsubmit={(e) => { e.preventDefault(); savePreset(); }}>
      <input placeholder="Name, e.g. Gaming" maxlength="40" bind:value={presetName} />
      <button type="submit" disabled={!presetName.trim()}>Save current lighting</button>
    </form>
  </div>
</div>

<style>
  .gap {
    gap: 18px;
  }

  /* Only there to disable the controls: it adds no box of its own. */
  .guard {
    display: contents;
  }

  .kinds {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(170px, 1fr));
    gap: 8px;
  }

  .kind {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 4px;
    padding: 12px 14px;
    text-align: left;
  }

  .kind.active,
  .kind.active:hover:not(:disabled) {
    border-color: var(--accent);
    background: var(--panel-3);
    box-shadow: inset 0 0 0 1px var(--accent);
  }

  .zones {
    display: grid;
    grid-template-columns: repeat(8, 1fr);
    gap: 8px;
  }

  .zone {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
  }

  input[type="color"] {
    width: 100%;
    min-width: 44px;
    height: 36px;
    padding: 2px;
    cursor: pointer;
  }

  .swatch {
    width: 26px;
    height: 26px;
    padding: 0;
    border-radius: 50%;
    border: 2px solid var(--line);
  }

  .row input[type="color"] {
    width: 44px;
  }

  .field input[type="range"] {
    max-width: 420px;
  }

  .tight {
    padding: 2px 4px;
  }
</style>
