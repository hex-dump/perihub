<script lang="ts">
  // The entries of a keyboard's key table: which keys send something else than when PeriHub first read the keyboard,
  // and a key to change. A key is named by what it sent then; the Fn key and the entries no source describes are
  // never offered.
  import type { KeyAction, PeripheralLimits } from "../lib/api";
  import { keyActionLabel } from "../lib/keys";

  interface Props {
    keys: KeyAction[];
    limits: NonNullable<PeripheralLimits["keys"]>;
    editable: boolean;
    disabled: boolean;
    onchange: (index: number, action: KeyAction) => void;
  }

  let { keys, limits, editable, disabled, onchange }: Props = $props();

  const same = (a: KeyAction | null | undefined, b: KeyAction | null | undefined) => JSON.stringify(a) === JSON.stringify(b);

  /** The entries that may be changed, named by what they sent first; a name two entries share gets the entry's number. */
  const entries = $derived.by(() => {
    const named = limits.factory.flatMap((f, i) => (f ? [{ index: i, name: keyActionLabel(f) }] : []));
    const count = new Map<string, number>();
    for (const e of named) count.set(e.name, (count.get(e.name) ?? 0) + 1);
    return named.map((e) => ((count.get(e.name) ?? 0) > 1 ? { ...e, name: `${e.name} (entry ${e.index})` } : e));
  });
  const nameOf = (i: number) => entries.find((e) => e.index === i)?.name ?? `Entry ${i}`;
  const changed = $derived(entries.filter((e) => !same(keys[e.index], limits.factory[e.index])));

  const GROUPS: [KeyAction["type"], string][] = [["key", "Keys"], ["modifier", "Modifiers"], ["media", "Media"]];
  const groups = $derived(
    GROUPS.map(([type, label]) => ({
      label,
      actions: limits.actions.filter((a) => a.type === type && !(a.type === "key" && a.usage === 0)),
    })),
  );
  const disabledKey: KeyAction = { type: "key", usage: 0 };

  let selected = $state<number | null>(null);
  const current = $derived(selected === null ? null : keys[selected]);

  function pick(value: string) {
    if (selected === null || !value) return;
    onchange(selected, JSON.parse(value) as KeyAction);
  }
</script>

{#if changed.length > 0}
  <table>
    <tbody>
      {#each changed as entry (entry.index)}
        <tr>
          <th>{entry.name}</th>
          <td>
            → {keyActionLabel(keys[entry.index])}
            {#if editable}
              <button class="small" {disabled} onclick={() => onchange(entry.index, limits.factory[entry.index]!)}>
                Put back
              </button>
            {/if}
          </td>
        </tr>
      {/each}
    </tbody>
  </table>
{:else}
  <p class="muted small">Every key sends what it sent when PeriHub first read this keyboard.</p>
{/if}

{#if editable}
  <div class="row change">
    <select aria-label="Key to change" value={selected === null ? "" : String(selected)} {disabled}
      onchange={(e) => (selected = e.currentTarget.value === "" ? null : Number(e.currentTarget.value))}>
      <option value="">Choose a key…</option>
      {#each entries as entry (entry.index)}<option value={String(entry.index)}>{entry.name}</option>{/each}
    </select>
    {#if selected !== null && current}
      <span>sends</span>
      <select aria-label={`What ${nameOf(selected)} sends`} value={JSON.stringify(current)} {disabled}
        onchange={(e) => pick(e.currentTarget.value)}>
        {#if !limits.actions.some((a) => same(a, current))}
          <option value={JSON.stringify(current)} disabled>{keyActionLabel(current)}</option>
        {/if}
        <option value={JSON.stringify(limits.factory[selected])}>{keyActionLabel(limits.factory[selected]!)} (as first read)</option>
        <option value={JSON.stringify(disabledKey)}>Nothing (disabled)</option>
        {#each groups as group (group.label)}
          <optgroup label={group.label}>
            {#each group.actions as action (JSON.stringify(action))}
              <option value={JSON.stringify(action)}>{keyActionLabel(action)}</option>
            {/each}
          </optgroup>
        {/each}
      </select>
    {/if}
  </div>
  <p class="muted small">
    The Fn key and the entries no source describes (such as the macro keys) are not offered.
  </p>
{/if}

<style>
  table {
    width: 100%;
    border-collapse: collapse;
  }

  th,
  td {
    padding: 4px 0;
    text-align: left;
  }

  th {
    width: 35%;
    color: var(--muted);
    font-weight: normal;
  }

  .change {
    gap: 8px;
    align-items: center;
    margin-top: 8px;
    flex-wrap: wrap;
  }
</style>
