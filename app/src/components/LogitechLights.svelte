<script lang="ts">
  // The lights of a Logitech keyboard PeriHub drives in host mode, as G HUB does with its on-board memory
  // mode off: an effect for each zone, a colour for each group of keys, and the brightness. Everything
  // stays in the keyboard's RAM; the core gives the lights back when asked, when G HUB or Solaar starts,
  // and when PeriHub quits. Experimental: the "No warranty" box comes first.
  import { api, errorText, type LightsLook, type LogitechLive, type UnsupportedReceiverDevice } from "../lib/api";
  import {
    EFFECT,
    effectName,
    effectTakesColor,
    effectTakesPeriod,
    keyGroups,
    perKeyKeys,
    settableEffect,
    waveDirections,
    zoneName,
  } from "../lib/logitechFeatures";
  import { acceptNoWarranty, noWarrantyAccepted } from "../lib/noWarranty";
  import NoWarranty from "./NoWarranty.svelte";

  interface Props {
    receiverId: number;
    device: UnsupportedReceiverDevice;
    live: LogitechLive | null;
    /** Called after every change, to read the keyboard again. */
    changed: () => Promise<void>;
  }

  let { receiverId, device, live, changed }: Props = $props();

  /** "Colours per key" in the keys' zone: a fixed colour with a colour per group of keys. */
  const PER_KEY = -1;
  /** Seconds a period can take, by effect. */
  const PERIODS: Record<number, [number, number, number]> = {
    [EFFECT.breathing]: [1, 20, 5],
    [EFFECT.cycle]: [1, 30, 5],
    [EFFECT.wave]: [1, 30, 5],
  };

  interface ZoneEdit {
    effect: number;
    color: string;
    seconds: number;
    direction: number;
  }

  const name = $derived(device.name ?? "keyboard");
  const lighting = $derived(device.lighting!);
  const zones = $derived(lighting.effects?.zones ?? []);
  const groups = $derived(keyGroups(perKeyKeys(lighting)));
  const host = $derived(live?.host ?? null);
  const warrantyKey = $derived(`lights:${name}`);

  let edits = $state<Record<number, ZoneEdit>>({});
  let groupColors = $state<Record<string, string>>({});
  let brightness = $state<number | null>(null);
  let busy = $state<string | null>(null);
  let message = $state<string | null>(null);
  let error = $state<string | null>(null);
  /** The change waiting for the "No warranty" box. */
  let waiting = $state<(() => void) | null>(null);
  let understood = $state(false);
  let loaded = false;

  // The editors start from what PeriHub shows, or the last look it showed (kept in its settings, also
  // after a restart), or a white fixed colour.
  $effect(() => {
    if (loaded || !live) return;
    loaded = true;
    const look = live.host?.look ?? live.saved_look;
    for (const zone of zones) {
      const shown = look?.zones.find((z) => z.zone === zone.index);
      const perKey = shown?.effect === EFFECT.fixed && zone.location === 1 && !!look?.keys.length;
      edits[zone.index] = {
        effect: perKey ? PER_KEY : (shown?.effect ?? EFFECT.fixed),
        color: shown?.color ?? "#ffffff",
        seconds: shown?.period_ms ? shown.period_ms / 1000 : 5,
        direction: shown?.direction ?? 0,
      };
    }
    for (const group of groups) {
      const key = look?.keys.find(([k]) => group.keys.includes(k));
      groupColors[group.name] = key?.[1] ?? "#ffffff";
    }
    brightness = live.lighting?.brightness ?? null;
  });

  function effectsOf(zone: (typeof zones)[number]) {
    const list = zone.effects.filter((e) => settableEffect(e.id)).map((e) => ({ value: e.id, label: effectName(e.id) }));
    if (zone.location === 1 && lighting.keys.length && zone.effects.some((e) => e.id === EFFECT.fixed)) {
      list.splice(2, 0, { value: PER_KEY, label: "Colours per key" });
    }
    return list;
  }

  function look(): LightsLook {
    const out: LightsLook = { zones: [], keys: [] };
    for (const zone of zones) {
      const edit = edits[zone.index];
      if (!edit) continue;
      if (edit.effect === PER_KEY) {
        out.zones.push({ zone: zone.index, effect: EFFECT.fixed, color: edit.color, period_ms: null, direction: null });
        for (const group of groups) {
          for (const key of group.keys) out.keys.push([key, groupColors[group.name] ?? edit.color]);
        }
        continue;
      }
      out.zones.push({
        zone: zone.index,
        effect: edit.effect,
        color: effectTakesColor(edit.effect) ? edit.color : null,
        period_ms: effectTakesPeriod(edit.effect) ? Math.round(edit.seconds * 1000) : null,
        direction: edit.effect === EFFECT.wave ? edit.direction : null,
      });
    }
    return out;
  }

  function allGroups(color: string) {
    for (const group of groups) groupColors[group.name] = color;
  }

  async function run(label: string, task: () => Promise<void>, done: string) {
    busy = label;
    error = message = null;
    try {
      await task();
      message = done;
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = null;
    }
    await changed();
  }

  /** Asks for the "No warranty" box once per model, then runs `action`. */
  function guarded(action: () => void) {
    if (noWarrantyAccepted(warrantyKey)) action();
    else waiting = action;
  }

  function confirm() {
    const action = waiting;
    waiting = null;
    understood = false;
    acceptNoWarranty(warrantyKey);
    action?.();
  }

  function show() {
    guarded(() => {
      const wanted = look();
      run(
        host ? "Showing it on the keyboard…" : "Taking the lights over…",
        () => api.logitechDriveLights(receiverId, device.slot, wanted),
        `The ${name} shows it.`,
      );
    });
  }

  function giveBack() {
    run(
      "Giving the lights back…",
      () => api.logitechGiveBackLights(receiverId, device.slot),
      `The ${name} has its own lights back.`,
    );
  }

  function setBrightness() {
    if (brightness === null) return;
    const value = brightness;
    guarded(() =>
      run("Setting the brightness…", () => api.logitechSetBrightness(receiverId, device.slot, value), `Brightness set to ${value}.`),
    );
  }

  function powerText(power: "full" | "dimmed" | "off"): string {
    if (power === "dimmed") return "dimmed: nobody is using it";
    if (power === "off") return "lights off: nobody has used it for a while";
    return "lights on";
  }
</script>

<div class="col lights">
  {#if live?.gave_back}
    <div class="notice info">PeriHub gave the {name} its own lights back: {live.gave_back}.</div>
  {/if}
  {#if host}
    <p class="status">
      <b>PeriHub drives the lights</b> · {powerText(host.power)}
      {#if host.retaken}<span class="muted">· taken again {host.retaken} {host.retaken === 1 ? "time" : "times"}</span>{/if}
    </p>
  {:else}
    <p class="small muted">
      PeriHub can drive the lights as G HUB does with its on-board memory mode off. Everything stays in the keyboard's
      RAM. PeriHub dims the lights and then turns them off when you stop typing, as the keyboard's own timers say. It
      gives them back when you press "Give the lights back", when you quit PeriHub, and when G HUB or Solaar starts.
      Turning the keyboard off and on also brings its own lights back.
    </p>
  {/if}

  {#each zones as zone (zone.index)}
    {@const edit = edits[zone.index]}
    {#if edit}
      <fieldset class="col zone">
        <legend>{zoneName(zone.location, device.kind)}</legend>
        <div class="row fields">
          <label class="col">
            <span class="small muted">Effect</span>
            <select bind:value={edit.effect} disabled={!!busy} aria-label={`Effect, ${zoneName(zone.location, device.kind)}`}>
              {#each effectsOf(zone) as option (option.value)}<option value={option.value}>{option.label}</option>{/each}
            </select>
          </label>
          {#if edit.effect === PER_KEY || effectTakesColor(edit.effect)}
            <label class="col">
              <span class="small muted">{edit.effect === PER_KEY ? "All keys" : "Colour"}</span>
              <input
                type="color"
                value={edit.color}
                disabled={!!busy}
                aria-label={`Colour, ${zoneName(zone.location, device.kind)}`}
                oninput={(e) => {
                  edit.color = (e.target as HTMLInputElement).value;
                  if (edit.effect === PER_KEY) allGroups(edit.color);
                }}
              />
            </label>
          {/if}
          {#if effectTakesPeriod(edit.effect)}
            {@const [min, max] = PERIODS[edit.effect]}
            <label class="col">
              <span class="small muted">One round, in seconds</span>
              <input type="number" {min} {max} step="0.5" bind:value={edit.seconds} disabled={!!busy} aria-label="Seconds" />
            </label>
          {/if}
          {#if edit.effect === EFFECT.wave}
            {@const caps = zone.effects.find((e) => e.id === EFFECT.wave)?.caps ?? 0}
            <label class="col">
              <span class="small muted">Direction</span>
              <select bind:value={edit.direction} disabled={!!busy} aria-label="Direction">
                {#each waveDirections(caps) as option (option.value)}<option value={option.value}>{option.label}</option>{/each}
              </select>
            </label>
          {/if}
        </div>
        {#if edit.effect === PER_KEY}
          <div class="groups">
            {#each groups as group (group.name)}
              <label class="row group">
                <input
                  type="color"
                  value={groupColors[group.name] ?? edit.color}
                  disabled={!!busy}
                  aria-label={group.name}
                  oninput={(e) => (groupColors[group.name] = (e.target as HTMLInputElement).value)}
                />
                {group.name} <span class="muted small">({group.keys.length})</span>
              </label>
            {/each}
          </div>
        {/if}
      </fieldset>
    {/if}
  {/each}

  <div class="row actions">
    <button class="primary" disabled={!!busy} onclick={show}>{host ? "Show on the keyboard" : "Drive the lights"}</button>
    {#if host}<button disabled={!!busy} onclick={giveBack}>Give the lights back</button>{/if}
    <span class="grow"></span>
    {#if busy}
      <span class="muted small" role="status">{busy}</span>
    {:else if message}
      <span class="saved small" role="status">{message}</span>
    {/if}
  </div>

  {#if lighting.brightness && brightness !== null}
    {@const b = lighting.brightness}
    <div class="row actions">
      <label class="row grow bright">
        Brightness
        <input type="range" min={b.min} max={b.max} step="1" bind:value={brightness} disabled={!!busy} aria-label="Brightness" />
        <span class="value">{brightness}</span>
      </label>
      <button disabled={!!busy || brightness === live?.lighting?.brightness} onclick={setBrightness}>Set brightness</button>
    </div>
    <p class="small muted">The {name} keeps the brightness, as it does with its own brightness key.</p>
  {/if}

  {#if waiting}
    <div class="ask col">
      <p>
        <b>Change the lights of the {name}?</b> Nobody has tried this on such a keyboard yet. PeriHub only sends effects
        and colours the keyboard says it has, and keeps them in its RAM: turning it off and on again brings its own
        lights back.
      </p>
      <NoWarranty device={`Logitech ${name}`} bind:accepted={understood} />
      <div class="row end">
        <button onclick={() => ((waiting = null), (understood = false))}>Cancel</button>
        <button class="primary" disabled={!understood} onclick={confirm}>Continue</button>
      </div>
    </div>
  {/if}
  {#if error}<div class="notice error">{error}</div>{/if}
</div>

<style>
  .lights {
    gap: 12px;
    margin-top: 14px;
  }

  p {
    margin: 0;
  }

  .zone {
    gap: 10px;
    border: 1px solid var(--line);
    border-radius: var(--radius);
    padding: 10px 14px 14px;
    margin: 0;
  }

  legend {
    padding: 0 6px;
    color: var(--muted);
  }

  .fields {
    gap: 16px;
    flex-wrap: wrap;
    align-items: flex-end;
  }

  .fields label {
    gap: 4px;
  }

  .fields input[type="number"] {
    width: 90px;
  }

  .groups {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: 6px 16px;
  }

  .group {
    gap: 8px;
    align-items: center;
  }

  .actions {
    gap: 8px;
    align-items: center;
  }

  .bright {
    gap: 10px;
    align-items: center;
  }

  .bright input {
    flex: 1;
    max-width: 260px;
  }

  .value {
    min-width: 3ch;
    font-variant-numeric: tabular-nums;
  }

  .ask {
    gap: 10px;
  }

  .end {
    justify-content: flex-end;
    gap: 8px;
  }
</style>
