<script lang="ts">
  // Line icons for the sidebar, drawn on a 24×24 grid.
  interface Props {
    name: "keyboard" | "mouse" | "profiles" | "settings" | "report" | "heart" | "star" | "coffee" | "close";
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
  {:else if name === "heart"}
    <path d="M12 20.5s-8-4.7-8-10.6A4.4 4.4 0 0 1 12 7.4a4.4 4.4 0 0 1 8 2.5c0 5.9-8 10.6-8 10.6z" />
  {:else if name === "star"}
    <path d="m12 3.5 2.6 5.4 5.9.8-4.3 4.1 1 5.9L12 16.9 6.8 19.7l1-5.9-4.3-4.1 5.9-.8z" />
  {:else if name === "coffee"}
    <path d="M4.5 9h12v5.5a4.5 4.5 0 0 1-4.5 4.5h-3a4.5 4.5 0 0 1-4.5-4.5z" />
    <path d="M16.5 10.5h1.5a2.5 2.5 0 0 1 0 5h-1.8M8 3.5v2.5M12 3.5v2.5" />
  {:else if name === "close"}
    <path d="M6 6l12 12M18 6 6 18" />
  {:else if name === "report"}
    <path d="M21 3 3 10.5l7 3 3 7.5z" />
    <path d="M10 13.5 21 3" />
  {:else}
    <path d="M4 7h9M17 7h3M4 17h3M11 17h9" />
    <circle cx="15" cy="7" r="2" />
    <circle cx="9" cy="17" r="2" />
  {/if}
</svg>
