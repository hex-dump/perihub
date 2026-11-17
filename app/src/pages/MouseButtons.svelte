<script lang="ts">
  import { onMount } from "svelte";
  import {
    api, type ButtonAction, type KeyInfo, type MouseInfo, type MouseProfile, type ProfileSettings, type SoftMacrosInfo,
  } from "../lib/api";
  import { mouseButtonName } from "../lib/keys";
  import { macroLibrary } from "../lib/macros";
  import { GLOSSARY } from "../lib/glossary";
  import ButtonActionEditor from "../components/ButtonActionEditor.svelte";
  import SoftMacros from "../components/SoftMacros.svelte";

  interface Props {
    info: MouseInfo;
    settings: ProfileSettings;
    /** Saved profiles and the number of the one being edited, to copy macros from. */
    profiles: MouseProfile[];
    profileNumber: number;
    /** Bytes free for new macros in the mouse. */
    macroSpace: number;
    /** The controls are disabled; the Normal / G-Shift switch still works, to look at the G-Shift layer. */
    readOnly?: boolean;
  }

  let { info, settings = $bindable(), profiles, profileNumber, macroSpace, readOnly = false }: Props = $props();
  let layer = $state<"normal" | "gshift">("normal");
  let keys = $state<KeyInfo[]>([]);
  let soft = $state<SoftMacrosInfo | null>(null);

  const refreshSoft = () => api.softMacros().then((i) => (soft = i), () => {});

  onMount(() => {
    api.keyCatalog().then((c) => (keys = c.targets));
    refreshSoft();
    // Which macros are running, while the window is in front.
    const timer = setInterval(() => document.hasFocus() && refreshSoft(), 2000);
    return () => clearInterval(timer);
  });

  const list = $derived(layer === "normal" ? settings.buttons : settings.gshift_buttons);
  const library = $derived(
    macroLibrary(info.product_id, profiles, { number: profileNumber, enabled: true, settings }),
  );
  const hasGShiftButton = $derived(settings.buttons.some((b) => b.type === "special" && b.function === "g_shift"));
  /**
   * Some mice do not say they have G-Shift in their memory description but still keep a G-Shift layer
   * in the profile (a G502 X wired): show it when something is assigned there too.
   */
  const showLayers = $derived(
    info.onboard.has_gshift || hasGShiftButton || settings.gshift_buttons.some((b) => b.type !== "disabled"),
  );

  function update(i: number, action: ButtonAction) {
    if (layer === "normal") settings.buttons[i] = action;
    else settings.gshift_buttons[i] = action;
  }
</script>

<div class="col gap">
  {#if showLayers}
    <div class="row">
      <div class="segmented">
        <button class:active={layer === "normal"} onclick={() => (layer = "normal")}>Normal</button>
        <button class:active={layer === "gshift"} onclick={() => (layer = "gshift")} title={GLOSSARY.gshift}>G-Shift</button>
      </div>
      <span class="muted small">
        {#if layer === "gshift"}
          Functions used while you hold the button assigned to “G-Shift”.
          {#if !hasGShiftButton}<b class="warn">No button has the G-Shift function yet.</b>{/if}
        {:else}
          Normal button functions.
        {/if}
      </span>
    </div>
  {/if}

  <fieldset disabled={readOnly}>
  <div class="card buttons">
    {#each list as action, i (i)}
      <div class="name">
        <strong>{mouseButtonName(info.product_id, i)}</strong>
        <span class="small muted">Button {i + 1}</span>
      </div>
      <ButtonActionEditor {action} {keys} {library} {macroSpace} softMacros={soft?.macros ?? []} onsoftsaved={refreshSoft} name={mouseButtonName(info.product_id, i)} allowDisabledHint={layer === "gshift"}
        onchange={(a) => update(i, a)} />
    {/each}
  </div>
  {#if soft}<SoftMacros info={soft} onchange={refreshSoft} />{/if}
  </fieldset>
</div>

<style>
  fieldset {
    border: none;
    margin: 0;
    min-width: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .gap {
    gap: 16px;
  }

  .buttons {
    display: grid;
    grid-template-columns: 200px 1fr;
    align-items: center;
    gap: 12px 20px;
  }

  .name {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .warn {
    color: var(--warn);
  }
</style>
