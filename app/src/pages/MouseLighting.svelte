<script lang="ts">
  import { onDestroy } from "svelte";
  import { api, debounce, errorText, type LedEffect, type LedZone, type MouseInfo, type ProfileSettings } from "../lib/api";
  import { ledZoneName } from "../lib/keys";

  interface Props {
    info: MouseInfo;
    settings: ProfileSettings;
    onpreview: () => void;
  }

  let { info, settings = $bindable(), onpreview }: Props = $props();
  let error = $state<string | null>(null);

  type Mode = LedEffect["mode"];
  // HID++ effect codes: 0 off, 1 fixed, 10 breathing, 3 color cycle.
  const MODES: { mode: Exclude<Mode, "raw">; id: number; label: string }[] = [
    { mode: "off", id: 0, label: "Off" },
    { mode: "fixed", id: 1, label: "Fixed" },
    { mode: "breathing", id: 10, label: "Breathing" },
    { mode: "cycle", id: 3, label: "Color cycle" },
  ];

  // Zones are sent one after the other, so animated effects start together and stay in step.
  let destroyed = false;
  const preview = debounce(async (zones: number[], effect: LedEffect) => {
    // Marked before sending, so that reverting also covers a preview on its way.
    onpreview();
    try {
      for (const zone of zones) {
        if (destroyed) return;
        await api.previewLed(zone, effect);
      }
      error = null;
    } catch (e) {
      error = errorText(e);
    }
  }, 120);

  const zones = $derived(info.led_zones.filter((z) => z.index < settings.leds.length));
  const isAnimated = (e: LedEffect) => e.mode === "breathing" || e.mode === "cycle";
  const supports = (zone: LedZone, effect: LedEffect) =>
    zone.effects.includes(MODES.find((m) => m.mode === effect.mode)?.id ?? -1);

  /**
   * Breathing and color cycle are shared by all zones, so the DPI indicator and the logo pulse
   * in sync: choosing one of them sets every zone, and editing it updates the zones using it.
   */
  function set(zone: LedZone, effect: LedEffect, linkAll = false) {
    const targets = isAnimated(effect)
      ? zones.filter((z) => z.index === zone.index ||
          (supports(z, effect) && (linkAll || settings.leds[z.index].mode === effect.mode)))
      : [zone];
    for (const z of targets) {
      settings.leds[z.index] = effect;
      delete fullColors[z.index];
    }
    preview(targets.map((z) => z.index), effect);
  }

  function colorOf(effect: LedEffect): string {
    return effect.mode === "fixed" || effect.mode === "breathing" ? effect.color : "#ff5200";
  }

  function changeMode(zone: LedZone, mode: Mode) {
    const current = settings.leds[zone.index];
    const color = colorOf(current);
    const period = "period_ms" in current ? current.period_ms : 4000;
    const brightness = "brightness" in current ? current.brightness : 100;
    switch (mode) {
      case "off": return set(zone, { mode: "off" });
      case "fixed": return set(zone, { mode: "fixed", color });
      case "breathing": return set(zone, { mode: "breathing", color, period_ms: period, brightness }, true);
      case "cycle": return set(zone, { mode: "cycle", period_ms: period, brightness }, true);
    }
  }

  function patch(zone: LedZone, change: Partial<{ color: string; period_ms: number; brightness: number }>) {
    set(zone, { ...settings.leds[zone.index], ...change } as LedEffect);
  }

  // Fixed colors have no brightness setting in the mouse, so brightness scales the color itself.
  const channels = (hex: string) => [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16));
  const toHex = (rgb: number[]) => "#" + rgb.map((v) => Math.round(v).toString(16).padStart(2, "0")).join("");

  /** Brightness of a fixed color in percent: its brightest channel. */
  function fixedBrightness(color: string): number {
    return Math.max(1, Math.round((Math.max(...channels(color)) / 255) * 100));
  }

  /** Full-brightness version of each zone's fixed color while its slider is moved, to avoid drift. */
  let fullColors: Record<number, string> = {};
  $effect(() => {
    void settings;
    fullColors = {};
  });

  function setFixedBrightness(zone: LedZone, color: string, percent: number) {
    const rgb = channels(color);
    const max = Math.max(...rgb);
    const full = fullColors[zone.index] ?? (max === 0 ? "#ffffff" : toHex(rgb.map((v) => (v * 255) / max)));
    patch(zone, { color: toHex(channels(full).map((v) => (v * percent) / 100)) });
    fullColors[zone.index] = full;
  }

  const num = (e: Event) => Number((e.target as HTMLInputElement).value);
  onDestroy(() => {
    destroyed = true;
    preview.cancel();
  });
</script>

<div class="col gap">
  <p class="muted small">
    Changes are previewed on the mouse right away and become permanent with “Save to mouse”.
    The preview uses the connected mouse even when you are editing a profile that is not active.
    {#if zones.length > 1}Breathing and color cycle apply to all zones at once, so they stay in sync.{/if}
  </p>
  {#if error}<div class="notice error">{error}</div>{/if}

  {#each zones as zone (zone.index)}
    {@const effect = settings.leds[zone.index]}
    <div class="card col">
      <h2>{ledZoneName(info.product_id, zone.location)}</h2>
      {#if effect.mode === "raw"}
        <p class="muted small">Effect set by another program: pick one to replace it.</p>
      {/if}
      <div class="segmented">
        {#each MODES.filter((m) => zone.effects.includes(m.id)) as m (m.mode)}
          <button class:active={effect.mode === m.mode} onclick={() => changeMode(zone, m.mode)}>{m.label}</button>
        {/each}
      </div>

      {#if effect.mode === "fixed"}
        <div class="row sliders">
          <label class="field">
            Color
            <input type="color" value={effect.color} oninput={(e) => patch(zone, { color: (e.target as HTMLInputElement).value })} />
          </label>
          <label class="field grow">
            Brightness: {fixedBrightness(effect.color)}%
            <input type="range" min="1" max="100" step="1" value={fixedBrightness(effect.color)}
              oninput={(e) => setFixedBrightness(zone, effect.color, num(e))} />
          </label>
        </div>
      {:else if effect.mode === "breathing"}
        <label class="field">
          Color
          <input type="color" value={effect.color} oninput={(e) => patch(zone, { color: (e.target as HTMLInputElement).value })} />
        </label>
      {/if}
      {#if effect.mode === "breathing" || effect.mode === "cycle"}
        <div class="row sliders">
          <label class="field grow">
            Cycle duration: {(effect.period_ms / 1000).toFixed(1)} s
            <input type="range" min="1000" max="20000" step="250" value={effect.period_ms}
              oninput={(e) => patch(zone, { period_ms: num(e) })} />
          </label>
          <label class="field grow">
            Brightness: {effect.brightness}%
            <input type="range" min="1" max="100" step="1" value={effect.brightness}
              oninput={(e) => patch(zone, { brightness: num(e) })} />
          </label>
        </div>
      {/if}
    </div>
  {:else}
    <div class="notice info">PeriHub cannot control the lighting of this mouse.</div>
  {/each}
</div>

<style>
  .gap {
    gap: 16px;
  }

  .segmented {
    align-self: flex-start;
  }

  input[type="color"] {
    width: 80px;
    height: 36px;
    padding: 2px;
  }

  .sliders {
    gap: 30px;
  }
</style>
