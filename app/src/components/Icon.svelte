<script lang="ts">
  // Line icons for the sidebar, drawn on a 24×24 grid.
  interface Props {
    name: "keyboard" | "mouse" | "profiles" | "settings" | "report";
  }

  let { name }: Props = $props();
</script>

<svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" stroke-width="1.8"
  stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
  {#if name === "keyboard"}
    <rect x="2.5" y="6" width="19" height="12" rx="2.5" />
    <path d="M6.5 10h.01M10 10h.01M13.5 10h.01M17 10h.01M8 14h8" />
  {:else if name === "mouse"}
    <rect x="6.5" y="3" width="11" height="18" rx="5.5" />
    <path d="M12 7v3" />
  {:else if name === "profiles"}
    <path d="M12 3.5 3.5 8 12 12.5 20.5 8z" />
    <path d="m3.5 12.5 8.5 4.5 8.5-4.5" />
    <path d="m3.5 16.5 8.5 4.5 8.5-4.5" />
  {:else if name === "report"}
    <path d="M21 3 3 10.5l7 3 3 7.5z" />
    <path d="M10 13.5 21 3" />
  {:else}
    <path d="M4 7h9M17 7h3M4 17h3M11 17h9" />
    <circle cx="15" cy="7" r="2" />
    <circle cx="9" cy="17" r="2" />
  {/if}
</svg>
