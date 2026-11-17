<script lang="ts">
  // A state as an icon and a word, never as a color alone (see lib/status.ts). `chip` puts it in a neutral
  // pill, for a heading or a card; without it, it is a line of small text, as under a device in the sidebar.
  import { FILLED, STATUS_ICONS, type StatusKind } from "../lib/status";

  interface Props {
    kind: StatusKind;
    label: string;
    chip?: boolean;
    title?: string;
  }

  let { kind, label, chip = false, title }: Props = $props();
</script>

<span class="status" class:chip data-kind={kind} {title}>
  {#if FILLED[kind]}
    <svg viewBox={FILLED[kind].viewBox} width="16" height="16" fill="currentColor" aria-hidden="true">
      {#each STATUS_ICONS[kind] as d, i (i)}<path {d} />{/each}
    </svg>
  {:else}
    <svg viewBox="0 0 16 16" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.5"
      stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
      {#each STATUS_ICONS[kind] as d, i (i)}<path {d} />{/each}
    </svg>
  {/if}
  <span class="label">{label}</span>
</span>
