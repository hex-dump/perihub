<script lang="ts">
  // A keyboard's layout as PeriHub draws and names its keys, where it was detected, and the user's own
  // choice when detection is wrong (kept for this keyboard).
  import { onMount } from "svelte";
  import { layoutCaps } from "../lib/keys";
  import {
    LEGEND_SETS, PHYSICALS, keyboardLayout, layoutName, layoutSources, layoutStatus, layouts, legendsName,
    loadSystemLayout, physicalName, setLayoutChoice, type DeviceLayout, type LayoutChoice,
  } from "../lib/keyboardLayout.svelte";

  interface Props {
    /** "vvvv:pppp": the choice is kept under it. */
    deviceKey: string;
    /** What the keyboard said about its layout, if its protocol has a read for it. */
    device?: DeviceLayout | null;
    /** PeriHub draws this keyboard's keys: the count of the keys drawn is shown. */
    drawn?: boolean;
  }

  let { deviceKey, device = null, drawn = false }: Props = $props();
  let editing = $state(false);

  // The user may have switched layouts since the last keyboard page.
  onMount(() => void loadSystemLayout(true));

  const layout = $derived(keyboardLayout(deviceKey, device));
  const detected = $derived(keyboardLayout(null, device));
  const choice = $derived<LayoutChoice>(layouts.choices[deviceKey] ?? {});

  function pick(part: keyof LayoutChoice, value: string) {
    setLayoutChoice(deviceKey, { ...choice, [part]: value || undefined });
  }
</script>

<div class="col layout">
  <div class="row small">
    <span class="grow">
      Layout: <b>{layoutName(layout)}</b>{drawn ? `, ${layoutCaps(layout.physical).length} keys` : ""}, {layoutStatus(layout)}
    </span>
    <button type="button" class="tiny" onclick={() => (editing = !editing)}>{editing ? "Done" : "Change"}</button>
  </div>
  {#if editing}
    <div class="row small wrap">
      <label class="row">
        Shape
        <select value={choice.physical ?? ""} onchange={(e) => pick("physical", (e.target as HTMLSelectElement).value)}>
          <option value="">Detected ({physicalName(detected.physical)})</option>
          {#each PHYSICALS as p (p.id)}<option value={p.id}>{p.label}</option>{/each}
        </select>
      </label>
      <label class="row">
        Key legends
        <select value={choice.legends ?? ""} onchange={(e) => pick("legends", (e.target as HTMLSelectElement).value)}>
          <option value="">Detected ({legendsName(detected.legends)})</option>
          {#each LEGEND_SETS as l (l.id)}<option value={l.id}>{l.label}</option>{/each}
        </select>
      </label>
    </div>
    <p class="muted small">{layoutSources(layout, device)}</p>
  {/if}
</div>

<style>
  .layout {
    gap: 6px;
  }

  .wrap {
    flex-wrap: wrap;
    gap: 14px;
  }
</style>
