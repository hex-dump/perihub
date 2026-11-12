<script lang="ts">
  import { onDestroy, untrack } from "svelte";
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

  const zones = $derived(info.led_zones.filter((z) => z.index < settings.leds.length));
  const modeId = (effect: LedEffect) => MODES.find((m) => m.mode === effect.mode)?.id ?? -1;
  const supports = (zone: LedZone, effect: LedEffect) => zone.effects.includes(modeId(effect));
  /** The modes every zone can show: the only ones offered while the zones are in sync. */
  const sharedModes = $derived(MODES.filter((m) => zones.every((z) => z.effects.includes(m.id))));
  const same = (a: LedEffect, b: LedEffect) => JSON.stringify(a) === JSON.stringify(b);

  /**
   * In sync, one card sets every zone at once, so breathing and color cycle start together and pulse
   * in step. Out of sync, each zone keeps its own effect. A profile opens in sync when all its zones
   * already show the same effect.
   */
  let synced = $state(false);
  const canSync = $derived(zones.length > 1 && sharedModes.length > 0);

  /** Full-brightness version of each zone's fixed color while its slider is moved, to avoid drift. */
  let fullColors: Record<number, string> = {};
  $effect.pre(() => {
    void settings;
    untrack(() => {
      fullColors = {};
      const first = zones[0] ? settings.leds[zones[0].index] : undefined;
      synced = canSync && !!first && first.mode !== "raw" && zones.every((z) => same(settings.leds[z.index], first));
    });
  });

  // Effects waiting to be previewed, by zone: an edit of another zone within the wait is sent too.
  // Batches go one after the other, in the order they were made.
  let destroyed = false;
  const waiting = new Map<number, LedEffect>();
  let sending = Promise.resolve();
  const flush = debounce(() => {
    const batch = [...waiting];
    waiting.clear();
    // Marked before sending, so that reverting also covers a preview on its way.
    onpreview();
    sending = sending.then(async () => {
      try {
        for (const [zone, effect] of batch) {
          if (destroyed) return;
          await api.previewLed(zone, effect);
        }
        error = null;
      } catch (e) {
        error = errorText(e);
      }
    });
  }, 120);

  function set(targets: LedZone[], effect: LedEffect) {
    for (const z of targets) {
      settings.leds[z.index] = { ...effect };
      delete fullColors[z.index];
      waiting.set(z.index, { ...effect });
    }
    flush();
  }

  /** The zones an edit made on `zone`'s card goes to. */
  const targets = (zone: LedZone) => (synced ? zones : [zone]);

  function colorOf(effect: LedEffect): string {
    return effect.mode === "fixed" || effect.mode === "breathing" ? effect.color : "#ff5200";
  }

  function changeMode(zone: LedZone, mode: Mode) {
    const current = settings.leds[zone.index];
    const color = colorOf(current);
    const period = "period_ms" in current ? current.period_ms : 4000;
    const brightness = "brightness" in current ? current.brightness : 100;
    switch (mode) {
      case "off": return set(targets(zone), { mode: "off" });
      case "fixed": return set(targets(zone), { mode: "fixed", color });
      case "breathing": return set(targets(zone), { mode: "breathing", color, period_ms: period, brightness });
      case "cycle": return set(targets(zone), { mode: "cycle", period_ms: period, brightness });
    }
  }

  function patch(zone: LedZone, change: Partial<{ color: string; period_ms: number; brightness: number }>) {
    set(targets(zone), { ...$state.snapshot(settings.leds[zone.index]), ...change } as LedEffect);
  }

  /**
   * Turning sync on gives every zone the first effect they can all show; with none (an effect set by
   * another program) nothing is sent until a mode is picked. Turning it off sends nothing.
   */
  function setSynced(on: boolean) {
    synced = on;
    if (!on) return;
    const effect = zones
      .map((z) => $state.snapshot(settings.leds[z.index]) as LedEffect)
      .find((e) => e.mode !== "raw" && zones.every((z) => supports(z, e)));
    if (effect && zones.some((z) => !same(settings.leds[z.index], effect))) set(zones, effect);
  }

  // Fixed colors have no brightness setting in the mouse, so brightness scales the color itself.
  const channels = (hex: string) => [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16));
  const toHex = (rgb: number[]) => "#" + rgb.map((v) => Math.round(v).toString(16).padStart(2, "0")).join("");

  /** Brightness of a fixed color in percent: its brightest channel. */
  function fixedBrightness(color: string): number {
    return Math.max(1, Math.round((Math.max(...channels(color)) / 255) * 100));
  }

  function setFixedBrightness(zone: LedZone, color: string, percent: number) {
    const rgb = channels(color);
    const max = Math.max(...rgb);
    const full = fullColors[zone.index] ?? (max === 0 ? "#ffffff" : toHex(rgb.map((v) => (v * 255) / max)));
    patch(zone, { color: toHex(channels(full).map((v) => (v * percent) / 100)) });
    fullColors[zone.index] = full;
  }

  /** The cards on screen: one for all the zones in sync, one per zone otherwise. */
  const zoneName = (zone: LedZone) => ledZoneName(info.product_id, zone.location);
  const cards = $derived(
    synced && zones.length > 0
      ? [{ zone: zones[0], title: "All zones", subtitle: zones.map(zoneName).join(" and "), modes: sharedModes }]
      : zones.map((zone) => ({
          zone,
          title: zoneName(zone),
          subtitle: "",
          modes: MODES.filter((m) => zone.effects.includes(m.id)),
        })),
  );

  const num = (e: Event) => Number((e.target as HTMLInputElement).value);
  onDestroy(() => {
    destroyed = true;
    flush.cancel();
  });
</script>

<div class="col gap">
  <p class="muted small">
    Changes are previewed on the mouse right away and become permanent with “Save to mouse”.
    The preview uses the connected mouse even when you are editing a profile that is not active.
  </p>
  {#if canSync}
    <label class="row small sync">
      <input type="checkbox" checked={synced} onchange={(e) => setSynced((e.target as HTMLInputElement).checked)} />
      <span>
        Same effect on all zones
        <span class="muted">
          {synced
            ? "· breathing and color cycle pulse together"
            : "· each zone has its own effect, and animations may not pulse together"}
        </span>
      </span>
    </label>
  {/if}
  {#if error}<div class="notice error">{error}</div>{/if}

  {#each cards as { zone, title, subtitle, modes } (synced ? "all" : zone.index)}
    {@const effect = settings.leds[zone.index]}
    <div class="card col">
      <h2>{title}</h2>
      {#if subtitle}<p class="muted small">{subtitle}</p>{/if}
      {#if effect.mode === "raw"}
        <p class="muted small">Effect set by another program: pick one to replace it.</p>
      {/if}
      <div class="segmented">
        {#each modes as m (m.mode)}
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

  .sync {
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
