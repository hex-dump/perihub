<script lang="ts">
  // A quiet "Support" entry at the bottom of the sidebar: two links, opened only on a click.
  import { api, errorText } from "../lib/api";
  import Icon from "./Icon.svelte";

  const COFFEE_URL = "https://buymeacoffee.com/hexdump1";
  const GITHUB_URL = "https://github.com/hex-dump/perihub";

  let open = $state(false);
  let error = $state<string | null>(null);
  let root = $state<HTMLDivElement>();

  function openLink(event: Event, url: string) {
    event.preventDefault();
    error = null;
    api.openSupportLink(url).catch((e) => (error = errorText(e)));
  }

  function close(event: Event) {
    if (!open) return;
    if (event instanceof KeyboardEvent) {
      if (event.key !== "Escape") return;
      root?.querySelector("button")?.focus();
    } else if (root?.contains(event.target as Node)) return;
    open = false;
  }
</script>

<svelte:window onkeydown={close} onpointerdown={close} />

<div class="support" bind:this={root}>
  <button class="ghost toggle" aria-expanded={open} aria-controls="support-links" onclick={() => (open = !open)}>
    <Icon name="heart" />
    <span>Support</span>
  </button>
  {#if open}
    <div class="pop" id="support-links" role="group" aria-label="Support PeriHub">
      <p class="small muted">PeriHub is free. These are optional.</p>
      <a href={COFFEE_URL} onclick={(e) => openLink(e, COFFEE_URL)}><Icon name="coffee" /> Buy me a coffee</a>
      <a href={GITHUB_URL} onclick={(e) => openLink(e, GITHUB_URL)}><Icon name="star" /> Star on GitHub</a>
      {#if error}<p class="small" role="alert">{error}</p>{/if}
    </div>
  {/if}
</div>

<style>
  .support {
    position: relative;
    display: flex;
    flex-direction: column;
  }

  .toggle {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    padding: 8px 12px;
    font-size: 13px;
    text-align: left;
  }

  .toggle:hover,
  .toggle[aria-expanded="true"] {
    background: var(--panel-2);
    color: var(--text);
  }

  .toggle :global(svg) {
    flex: none;
  }

  .pop {
    position: absolute;
    left: 0;
    right: 0;
    bottom: calc(100% + 6px);
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 10px;
    background: var(--panel-2);
    border: 1px solid var(--line-strong);
    border-radius: 10px;
    z-index: 10;
  }

  .pop p {
    margin: 0 2px 6px;
  }

  .pop a {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px;
    border-radius: 8px;
    color: var(--text);
    text-decoration: none;
  }

  .pop a:hover {
    background: var(--panel-3);
  }
</style>
