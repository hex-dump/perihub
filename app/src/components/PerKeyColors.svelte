<script lang="ts">
  // A keyboard's colours per LED, for its per-key lighting mode, as a grid of rows and columns: LED i is in row
  // i / columns, column i % columns. Keys are named where a source names the position; the rest by row and column.
  import type { PeripheralLimits } from "../lib/api";
  import { keyActionLabel } from "../lib/keys";

  interface Props {
    colors: string[];
    /** Missing: shown, not changed. */
    limits: PeripheralLimits["key_colors"];
    editable: boolean;
    disabled: boolean;
    onchange: (index: number, color: string) => void;
    onfill: (color: string) => void;
  }

  let { colors, limits, editable, disabled, onchange, onfill }: Props = $props();

  /** Without limits the keyboard gave no map: the rows of OpenRGB's EVision matrix are 21 wide. */
  const columns = $derived(limits?.columns ?? 21);
  const canChange = $derived(editable && !!limits);
  let fill = $state("#ffffff");

  function name(i: number): string {
    const where = `row ${Math.floor(i / columns) + 1}, column ${(i % columns) + 1}`;
    const key = limits?.keys[i];
    return key ? `${keyActionLabel(key)} (${where})` : `LED ${i + 1} (${where})`;
  }
</script>

<p class="muted small">
  They show when the lighting mode is Custom (per key).{#if limits} {limits.note}{/if}
</p>
<div class="grid" style:grid-template-columns="repeat({columns}, minmax(0, 1fr))">
  {#each colors as color, i (i)}
    {#if canChange}
      <input type="color" class="cell" class:named={!!limits?.keys[i]} aria-label={name(i)} title={name(i)} value={color}
        {disabled} onchange={(e) => onchange(i, e.currentTarget.value)} />
    {:else}
      <span class="cell" role="img" aria-label={`${name(i)}: ${color}`} title={`${name(i)}: ${color}`}
        style:background={color}></span>
    {/if}
  {/each}
</div>
{#if canChange}
  <div class="row fill">
    <input type="color" aria-label="Colour for every LED" bind:value={fill} {disabled} />
    <button {disabled} onclick={() => onfill(fill)}>Set every LED</button>
  </div>
{/if}

<style>
  .grid {
    display: grid;
    gap: 3px;
    max-width: 640px;
  }

  .cell {
    display: block;
    width: 100%;
    aspect-ratio: 1;
    min-width: 0;
    padding: 0;
    border: 1px solid var(--line);
    border-radius: 4px;
    cursor: pointer;
    opacity: 0.6;
  }

  span.cell {
    cursor: default;
  }

  /* A position a source names is a key: drawn at full strength, the others fainter. */
  .cell.named,
  span.cell {
    opacity: 1;
  }

  .fill {
    gap: 8px;
    align-items: center;
    margin-top: 8px;
  }
</style>
