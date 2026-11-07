<script lang="ts">
  import { onMount } from "svelte";
  import {
    api, errorText, type KeyBinding, type KeyCatalog, type KeyChange, type Keymap, type Macro, type SoftMacrosInfo,
    type SoftwareMacro,
  } from "../lib/api";
  import { bindingLabel, keyLabel, LAYOUT_HEIGHT, LAYOUT_WIDTH, layoutCaps, type KeyCap } from "../lib/keys";
  import { softSummary } from "../lib/macros";
  import { LAYOUTS, prefs, setLayout, type LayoutId } from "../lib/prefs.svelte";
  import BackupsDialog from "../components/BackupsDialog.svelte";
  import ConfirmButton from "../components/ConfirmButton.svelte";
  import { keymapRestoreMessage } from "../lib/backups";
  import KeyAccessPermission from "../components/KeyAccessPermission.svelte";
  import MacroEditor from "../components/MacroEditor.svelte";
  import SoftMacros from "../components/SoftMacros.svelte";

  const UNIT = 46;
  // The board shrinks to fit narrow windows.
  let fitWidth = $state(0);
  const unit = $derived(fitWidth > 0 ? Math.min(UNIT, (fitWidth - 26) / LAYOUT_WIDTH) : UNIT);
  const STEELSERIES_KEY = 0xf0;
  // Factory functions of the SteelSeries layer (same as perihub-core/src/apex/keys.rs).
  const FACTORY_FUNCTIONS: Record<number, KeyBinding> = {
    0x42: { type: "function", code: 1, param: 0 },
    0x43: { type: "function", code: 2, param: 1 },
    0x44: { type: "function", code: 3, param: 1 },
    0x45: { type: "function", code: 4, param: 1 },
    0xe3: { type: "function", code: 5, param: 0 },
  };

  let catalog = $state<KeyCatalog | null>(null);
  let keymap = $state<Keymap | null>(null);
  let layer = $state<0 | 1>(0);
  let selected = $state<number | null>(null);
  /** Unsaved changes, per layer: usage → new function. */
  let drafts = $state<[Record<number, KeyBinding>, Record<number, KeyBinding>]>([{}, {}]);
  let busy = $state<string | null>("Reading the key mapping…");
  let error = $state<string | null>(null);
  let message = $state<string | null>(null);
  /** Macros run by PeriHub, which a key can start. */
  let soft = $state<SoftMacrosInfo | null>(null);
  /** Macro run by PeriHub open in the editor, and the key it is for. */
  let editingSoft = $state<{ macro: SoftwareMacro; layer: 0 | 1; usage: number } | null>(null);
  const EMPTY_MACRO: Macro = { kind: "once", press: [], release: [] };

  const refreshSoft = () => api.softMacros().then((i) => (soft = i), () => {});

  onMount(() => {
    (async () => {
      try {
        [catalog, keymap] = await Promise.all([api.keyCatalog(), api.keymap()]);
      } catch (e) {
        error = errorText(e);
      } finally {
        busy = null;
      }
    })();
    refreshSoft();
    // Which macros are running, while the window is in front.
    const timer = setInterval(() => document.hasFocus() && refreshSoft(), 2000);
    return () => clearInterval(timer);
  });

  /** The macro run by PeriHub that a key with this function starts. */
  function macroOf(binding: KeyBinding | undefined): SoftwareMacro | undefined {
    return binding?.type === "key" ? soft?.macros.find((m) => m.key === binding.usage) : undefined;
  }

  function label(binding: KeyBinding | undefined): string {
    const m = macroOf(binding);
    return m ? `Macro: ${m.name}` : bindingLabel(binding);
  }

  const keyName = (l: number, usage: number) => `${keyLabel(usage)}${l === 1 ? " with SteelSeries" : ""}`;

  function factory(l: number, usage: number): KeyBinding {
    if (l === 0) return { type: "key", usage };
    return FACTORY_FUNCTIONS[usage] ?? { type: "none" };
  }

  const same = (a: KeyBinding | undefined, b: KeyBinding | undefined) => JSON.stringify(a) === JSON.stringify(b);

  function saved(l: number, usage: number): KeyBinding | undefined {
    if (!keymap) return undefined;
    return (l === 0 ? keymap.normal : keymap.steelseries)[usage];
  }

  function current(l: number, usage: number): KeyBinding | undefined {
    return drafts[l][usage] ?? saved(l, usage);
  }

  function remappable(cap: KeyCap): boolean {
    return !cap.locked && cap.usage !== STEELSERIES_KEY && !!catalog?.sources.includes(cap.usage);
  }

  function setBinding(usage: number, binding: KeyBinding, l: 0 | 1 = layer) {
    const next = { ...drafts[l] };
    if (same(binding, saved(l, usage))) delete next[usage];
    else next[usage] = binding;
    drafts[l] = next;
  }

  function onTarget(event: Event) {
    if (selected === null) return;
    const select = event.target as HTMLSelectElement;
    const value = select.value;
    if (value === "") return;
    if (value === "soft:new") {
      // The key gets the macro's key once the macro is saved.
      select.value = selectedValue;
      editingSoft = { macro: { key: 0, name: keyName(layer, selected), mode: { type: "once" }, steps: [] }, layer, usage: selected };
    } else if (value.startsWith("soft:")) {
      setBinding(selected, { type: "key", usage: Number(value.slice(5)) });
    } else {
      setBinding(selected, value === "factory" ? factory(layer, selected) : { type: "key", usage: Number(value) });
    }
  }

  async function saveSoft(item: SoftwareMacro) {
    if (!editingSoft) return;
    const { layer: l, usage } = editingSoft;
    error = null;
    try {
      const savedMacro = await api.saveSoftMacro(item);
      setBinding(usage, { type: "key", usage: savedMacro.key }, l);
      editingSoft = null;
      await refreshSoft();
    } catch (e) {
      error = errorText(e);
    }
  }

  const pending = $derived(
    drafts.flatMap((d, l) =>
      Object.entries(d).map(([usage, binding]): KeyChange => ({ layer: l, source: Number(usage), binding })),
    ),
  );

  async function save() {
    busy = "Saving to the keyboard…";
    error = message = null;
    try {
      keymap = await api.applyKeymap(pending);
      drafts = [{}, {}];
      message = "Key mapping saved to the keyboard.";
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = null;
    }
  }

  async function resetAll() {
    busy = "Restoring factory keys…";
    error = message = null;
    try {
      keymap = await api.resetKeymap();
      drafts = [{}, {}];
      message = "All keys are back to their factory settings.";
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = null;
    }
  }

  let showBackups = $state(false);

  async function restoreBackup(id: string) {
    busy = "Restoring the backup…";
    error = message = null;
    try {
      const restored = await api.restoreKeyboardBackup(id);
      keymap = restored.keymap;
      drafts = [{}, {}];
      message = keymapRestoreMessage(restored.keys);
      showBackups = false;
    } finally {
      busy = null;
    }
  }

  type Group = { name: string; keys: { usage: number; label: string }[] };
  const groups = $derived.by((): Group[] => {
    if (!catalog) return [];
    const ranges: [string, (u: number) => boolean][] = [
      ["Letters", (u) => u >= 0x04 && u <= 0x1d],
      ["Numbers and symbols", (u) => (u >= 0x1e && u <= 0x38 && ![0x28, 0x29, 0x2a, 0x2b, 0x2c].includes(u)) || u === 0x64],
      ["Special keys", (u) => [0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x39, 0x65].includes(u)],
      ["Function keys F1–F24", (u) => (u >= 0x3a && u <= 0x45) || (u >= 0x68 && u <= 0x73)],
      ["Navigation", (u) => u >= 0x46 && u <= 0x52],
      ["Modifiers", (u) => u >= 0xe0 && u <= 0xe7],
      ["Numpad", (u) => (u >= 0x53 && u <= 0x63) || u === 0x67 || u === 0x85],
      ["International", (u) => u >= 0x87 && u <= 0x91],
    ];
    return ranges.map(([name, test]) => ({
      name,
      keys: catalog!.targets.filter((k) => test(k.usage)).map((k) => ({ usage: k.usage, label: keyLabel(k.usage) })),
    }));
  });

  function capClass(cap: KeyCap): string {
    if (!remappable(cap)) return "locked";
    const binding = current(layer, cap.usage);
    if (drafts[layer][cap.usage]) return "draft";
    if (!same(binding, factory(layer, cap.usage))) return "custom";
    return "";
  }

  function capSub(cap: KeyCap): string {
    const binding = current(layer, cap.usage);
    if (layer === 0) return binding && !same(binding, factory(0, cap.usage)) ? `→ ${label(binding)}` : "";
    return label(binding);
  }

  const selectedBinding = $derived(selected === null ? undefined : current(layer, selected));
  const selectedMacro = $derived(macroOf(selectedBinding));
  const selectedValue = $derived(
    selectedMacro ? `soft:${selectedMacro.key}` : selectedBinding?.type === "key" ? String(selectedBinding.usage) : "",
  );
</script>

<div class="col gap">
  <div class="row">
    <div class="segmented">
      <button class:active={layer === 0} onclick={() => (layer = 0)}>Normal</button>
      <button class:active={layer === 1} onclick={() => (layer = 1)}>With SteelSeries key</button>
    </div>
    <span class="muted small grow">
      {layer === 0
        ? "What each key does when you press it."
        : "What each key does while you hold the SteelSeries key."}
    </span>
    <label class="row small muted">
      Layout
      <select value={prefs.layout} onchange={(e) => setLayout((e.target as HTMLSelectElement).value as LayoutId)}>
        {#each LAYOUTS as l (l.id)}
          <option value={l.id}>{l.label}</option>
        {/each}
      </select>
    </label>
  </div>

  {#if error}<div class="notice error">{error}</div>{/if}
  {#if message}<div class="notice ok">{message}</div>{/if}
  {#if busy}<div class="notice info">{busy}</div>{/if}

  {#if keymap}
    <div class="fit" bind:clientWidth={fitWidth}>
      <div class="board" style:width="{LAYOUT_WIDTH * unit}px" style:height="{LAYOUT_HEIGHT * unit}px">
        {#each layoutCaps(prefs.layout) as cap (cap.usage)}
          <button
            class="cap {capClass(cap)}"
            class:selected={selected === cap.usage}
            disabled={!remappable(cap) || !!busy}
            style:left="{cap.x * unit}px"
            style:top="{cap.y * unit}px"
            style:width="{cap.w * unit - 4}px"
            style:height="{cap.h * unit - 4}px"
            title={cap.locked ? "The firmware cannot remap this key" : keyLabel(cap.usage)}
            onclick={() => (selected = cap.usage)}
          >
            <span>{keyLabel(cap.usage)}</span>
            {#if capSub(cap)}<span class="sub">{capSub(cap)}</span>{/if}
          </button>
        {/each}
      </div>
    </div>

    <div class="row legend small muted">
      <span><i class="custom"></i> Customized</span>
      <span><i class="draft"></i> Not saved yet</span>
      <span><i class="locked"></i> Not remappable</span>
    </div>

    <div class="card row editor">
      {#if selected === null}
        <span class="muted">Select a key to change what it does.</span>
      {:else}
        <div class="col tight">
          <span class="small muted">Key</span>
          <strong>{keyLabel(selected)}{layer === 1 ? " (with SteelSeries)" : ""}</strong>
        </div>
        <div class="col tight">
          <span class="small muted">Function</span>
          <strong>{selectedBinding?.type === "none" ? "Same as without SteelSeries" : label(selectedBinding) || "No effect"}</strong>
          {#if selectedMacro}<span class="small muted">{softSummary(selectedMacro)} · sends {keyLabel(selectedMacro.key)}</span>{/if}
        </div>
        <label class="field grow">
          New function
          <select value={selectedValue} onchange={onTarget}>
            <option value="" disabled>Choose a key…</option>
            <option value="factory">Factory default</option>
            {#if soft}
              <optgroup label="Macro run by PeriHub">
                {#each soft.macros as m (m.key)}
                  <option value="soft:{m.key}">{m.name}</option>
                {/each}
                {#if soft.free_keys.length}<option value="soft:new">New macro…</option>{/if}
              </optgroup>
            {/if}
            {#each groups as group (group.name)}
              <optgroup label={group.name}>
                {#each group.keys as key (key.usage)}
                  <option value={String(key.usage)}>{key.label}</option>
                {/each}
              </optgroup>
            {/each}
          </select>
        </label>
        {#if selectedMacro && soft && selected !== null}
          <button
            onclick={() =>
              selectedMacro && selected !== null &&
              (editingSoft = { macro: JSON.parse(JSON.stringify(selectedMacro)), layer, usage: selected })}
          >
            Edit macro…
          </button>
        {/if}
      {/if}
    </div>

    {#if selectedMacro && selected !== null}
      {#if layer === 0}
        <div class="notice info">
          This key no longer types “{keyLabel(selected)}”. To keep it, start the macro from the SteelSeries layer instead
          (“With SteelSeries key” above).
        </div>
      {/if}
      <KeyAccessPermission
        title="Starting macros from the keyboard needs permission to read what you type on it."
        ongranted={refreshSoft}
      />
    {/if}

    <div class="row">
      <ConfirmButton class="danger" disabled={!!busy} onconfirm={resetAll} confirmLabel="Reset all keys?">
        Reset all to factory
      </ConfirmButton>
      <button disabled={!!busy} onclick={() => (showBackups = true)}>Backups…</button>
      <div class="grow"></div>
      {#if pending.length}
        <span class="muted small">{pending.length} unsaved {pending.length === 1 ? "change" : "changes"}</span>
        <button disabled={!!busy} onclick={() => (drafts = [{}, {}])}>Discard</button>
      {/if}
      <button class="primary" disabled={!pending.length || !!busy} onclick={save}>Save to keyboard</button>
    </div>
    <p class="muted small">
      The mapping is stored in the keyboard's memory and works without PeriHub, on any computer, except keys that start
      a macro run by PeriHub. A backup is created before every save.
    </p>

    {#if soft}<SoftMacros info={soft} onchange={refreshSoft} />{/if}
  {/if}
</div>

{#if showBackups}
  <BackupsDialog device="Apex 3 TKL" load={api.keyboardBackups} restore={restoreBackup} onclose={() => (showBackups = false)} />
{/if}

{#if editingSoft}
  <MacroEditor content={EMPTY_MACRO} soft={editingSoft.macro} onsavesoft={saveSoft} keys={catalog?.targets ?? []}
    button={keyName(editingSoft.layer, editingSoft.usage)} keyboard onsave={() => {}} oncancel={() => (editingSoft = null)} />
{/if}

<style>
  .gap {
    gap: 16px;
  }

  .tight {
    gap: 2px;
  }

  .fit {
    min-width: 0;
  }

  .board {
    position: relative;
    padding: 12px;
    box-sizing: content-box;
    border-radius: var(--radius);
    background: #0b0c10;
    border: 1px solid var(--line);
  }

  .cap {
    position: absolute;
    margin: 12px 0 0 12px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 2px;
    padding: 2px;
    font-size: 11px;
    line-height: 1.1;
    border-radius: 6px;
    background: var(--panel-2);
    border: 1px solid var(--line-strong);
    overflow: hidden;
  }

  .cap .sub {
    font-size: 9px;
    color: var(--accent-2);
  }

  .cap.custom {
    border-color: var(--accent);
  }

  .cap.draft {
    border-color: var(--warn);
    background: var(--warn-soft);
  }

  .cap.locked {
    opacity: 0.35;
  }

  .cap.selected {
    outline: 2px solid var(--accent);
    background: var(--panel-3);
  }

  .legend {
    gap: 18px;
  }

  .legend i {
    display: inline-block;
    width: 10px;
    height: 10px;
    border-radius: 3px;
    margin-right: 4px;
    border: 1px solid;
  }

  .legend i.custom { border-color: var(--accent); }
  .legend i.draft { border-color: var(--warn); background: var(--warn-soft); }
  .legend i.locked { opacity: 0.35; background: var(--panel-2); }

  .editor {
    gap: 28px;
    min-height: 72px;
  }

  .editor select {
    max-width: 320px;
  }
</style>
