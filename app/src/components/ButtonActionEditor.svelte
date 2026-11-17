<script lang="ts">
  import { api, errorText, type ButtonAction, type KeyInfo, type Macro, type SoftwareMacro } from "../lib/api";
  import { CONSUMER_KEYS, keyLabel, MODIFIERS, MOUSE_CLICKS, SPECIAL_FUNCTIONS } from "../lib/keys";
  import { macroSummary, softSummary, type MacroSource } from "../lib/macros";

  interface Props {
    action: ButtonAction;
    keys: KeyInfo[];
    /** Name of the button, for the macro editor. */
    name: string;
    /** Macros on other buttons, to start from. */
    library?: MacroSource[];
    /** Bytes free for new macros in the mouse. */
    macroSpace?: number;
    /** Macros run by PeriHub, and what to do after one is saved. */
    softMacros?: SoftwareMacro[];
    onsoftsaved?: () => void;
    allowDisabledHint?: boolean;
    onchange: (action: ButtonAction) => void;
  }

  let {
    action, keys, name, library = [], macroSpace, softMacros = [], onsoftsaved, allowDisabledHint = false, onchange,
  }: Props = $props();
  /** Macro open in the editor. */
  let editing = $state<Macro | null>(null);
  /** Macro run by PeriHub open in the editor. */
  let editingSoft = $state<SoftwareMacro | null>(null);
  let softError = $state<string | null>(null);
  const EMPTY: Macro = { kind: "once", press: [], release: [] };

  /** The macro run by PeriHub that this button starts: it sends that macro's key, without modifiers. */
  const soft = $derived(
    action.type === "key" && action.modifiers === 0 ? softMacros.find((m) => m.key === action.key) : undefined,
  );
  let kindSelect: HTMLSelectElement;

  type Kind = ButtonAction["type"] | "software";
  const KINDS: { kind: Kind; label: string }[] = [
    { kind: "mouse", label: "Mouse click" },
    { kind: "key", label: "Keyboard key" },
    { kind: "consumer", label: "Media key" },
    { kind: "macro", label: "Macro" },
    { kind: "software", label: "Macro run by PeriHub" },
    { kind: "special", label: "Mouse function" },
    { kind: "disabled", label: "Disabled" },
  ];

  function changeKind(kind: Kind) {
    switch (kind) {
      case "mouse": return onchange({ type: "mouse", button: 1 });
      case "key": return onchange({ type: "key", modifiers: 0, key: 0x04 });
      case "consumer": return onchange({ type: "consumer", usage: CONSUMER_KEYS[0].usage });
      case "macro": editing = { kind: "once", press: [], release: [] }; return;
      case "software": editingSoft = { key: 0, name, mode: { type: "once" }, steps: [] }; return;
      case "special": return onchange({ type: "special", function: "dpi_cycle" });
      case "disabled": return onchange({ type: "disabled" });
    }
  }

  function toggleModifier(bit: number, on: boolean) {
    if (action.type !== "key") return;
    onchange({ ...action, modifiers: on ? action.modifiers | bit : action.modifiers & ~bit });
  }

  function saveMacro(content: Macro) {
    // A changed macro keeps its old address: saving the profile writes it to free memory.
    const at = action.type === "macro" ? { sector: action.sector, offset: action.offset } : { sector: 0, offset: 0 };
    onchange({ type: "macro", ...at, content });
    editing = null;
  }

  function cancelMacro() {
    editing = editingSoft = null;
    kindSelect.value = soft ? "software" : action.type;
  }

  async function saveSoft(item: SoftwareMacro) {
    softError = null;
    try {
      const saved = await api.saveSoftMacro(item);
      onchange({ type: "key", modifiers: 0, key: saved.key });
      editingSoft = null;
      onsoftsaved?.();
    } catch (e) {
      softError = errorText(e);
    }
  }

  const value = (e: Event) => (e.target as HTMLSelectElement).value;
</script>

<div class="row wrap editor">
  <select bind:this={kindSelect} value={soft ? "software" : action.type} onchange={(e) => changeKind(value(e) as Kind)}>
    {#if action.type === "raw"}<option value="raw" disabled>Custom value</option>{/if}
    {#each KINDS as k (k.kind)}
      <option value={k.kind}>{k.label}</option>
    {/each}
  </select>

  {#if soft}
    <span class="small summary">{softSummary(soft)} · sends {keyLabel(soft.key)}</span>
    <button onclick={() => soft && (editingSoft = JSON.parse(JSON.stringify(soft)))}>Edit…</button>
  {:else if action.type === "mouse"}
    <select value={String(action.button)} onchange={(e) => onchange({ type: "mouse", button: Number(value(e)) })}>
      {#each MOUSE_CLICKS as click (click.button)}
        <option value={String(click.button)}>{click.label}</option>
      {/each}
    </select>
  {:else if action.type === "key"}
    {#each MODIFIERS as mod (mod.bit)}
      <label class="row small mod">
        <input type="checkbox" checked={(action.modifiers & mod.bit) !== 0}
          onchange={(e) => toggleModifier(mod.bit, (e.target as HTMLInputElement).checked)} />
        {mod.label}
      </label>
    {/each}
    <span class="muted">+</span>
    <select value={String(action.key)} onchange={(e) => action.type === "key" && onchange({ ...action, key: Number(value(e)) })}>
      {#each keys as key (key.usage)}
        <option value={String(key.usage)}>{keyLabel(key.usage)}</option>
      {/each}
    </select>
  {:else if action.type === "consumer"}
    <select value={String(action.usage)} onchange={(e) => onchange({ type: "consumer", usage: Number(value(e)) })}>
      {#if !CONSUMER_KEYS.some((c) => action.type === "consumer" && c.usage === action.usage)}
        <option value={String(action.usage)}>Code 0x{action.usage.toString(16)}</option>
      {/if}
      {#each CONSUMER_KEYS as c (c.usage)}
        <option value={String(c.usage)}>{c.label}</option>
      {/each}
    </select>
  {:else if action.type === "special"}
    <select value={action.function} onchange={(e) => onchange({ type: "special", function: value(e) as never })}>
      {#each SPECIAL_FUNCTIONS as f (f.value)}
        <option value={f.value}>{f.label}</option>
      {/each}
    </select>
  {:else if action.type === "macro"}
    <span class="small summary">{macroSummary(action.content)}</span>
    {#if action.content}
      <button onclick={() => action.type === "macro" && (editing = $state.snapshot(action.content))}>Edit…</button>
    {:else}
      <span class="muted small">Set by another program: pick a type to replace it.</span>
    {/if}
  {:else if action.type === "raw"}
    <span class="muted small">Set by another program: pick a type to replace it.</span>
  {:else if action.type === "disabled" && allowDisabledHint}
    <span class="muted small">The button does nothing while G-Shift is held.</span>
  {/if}
</div>

{#if softError}<div class="notice error" role="alert">{softError}</div>{/if}

<!-- The macro editor is loaded when first opened. -->
{#if editing}
  {#await import("./MacroEditor.svelte") then { default: MacroEditor }}
    <MacroEditor content={editing} {keys} {library} space={macroSpace} button={name} onsave={saveMacro} oncancel={cancelMacro} />
  {/await}
{/if}
{#if editingSoft}
  {#await import("./MacroEditor.svelte") then { default: MacroEditor }}
    <MacroEditor content={EMPTY} soft={editingSoft} onsavesoft={saveSoft} {keys} {library} button={name}
      onsave={() => {}} oncancel={cancelMacro} />
  {/await}
{/if}

<style>
  .editor {
    gap: 8px;
  }

  .mod {
    gap: 4px;
  }

  .summary {
    max-width: 420px;
  }
</style>
