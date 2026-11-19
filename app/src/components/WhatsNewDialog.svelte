<script lang="ts">
  // The release notes of one version: its summary and the title of each entry, with a link to the full
  // notes. Esc or Close closes it and focus goes back to what opened it.
  import { onMount } from "svelte";
  import { api, errorText } from "../lib/api";
  import type { ReleaseNotes } from "../lib/changelog";

  interface Props {
    notes: ReleaseNotes;
    onclose: () => void;
  }

  let { notes, onclose }: Props = $props();

  /** The headings of the release notes on GitHub (tools/release.py). */
  const HEADINGS: Record<string, string> = {
    "New devices": "🖱️ New devices",
    Added: "✨ New features",
    Performance: "⚡ Performance",
    Changed: "🔧 Changes",
    Fixed: "🐛 Bug fixes",
    Security: "🔒 Security",
    Removed: "🗑️ Removed",
    "Known limitations": "⚠️ Known limitations",
  };

  const url = $derived(`https://github.com/hex-dump/perihub/releases/tag/v${notes.version}`);
  let dialog: HTMLDivElement | undefined = $state();
  let closeButton: HTMLButtonElement | undefined = $state();
  let error = $state<string | null>(null);

  onMount(() => {
    const opener = document.activeElement;
    closeButton?.focus();
    return () => {
      if (opener instanceof HTMLElement) opener.focus();
    };
  });

  function openNotes(e: Event) {
    e.preventDefault();
    error = null;
    api.openReleaseNotes(notes.version).catch((err) => (error = errorText(err)));
  }

  function keydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      onclose();
    } else if (e.key === "Tab" && dialog) {
      // Keep Tab inside the dialog while it is open.
      const items = [...dialog.querySelectorAll<HTMLElement>("button, a[href], [tabindex='0']")];
      const first = items[0];
      const last = items[items.length - 1];
      const active = document.activeElement;
      if (!items.length || !dialog.contains(active)) {
        e.preventDefault();
        first?.focus();
      } else if (e.shiftKey && active === first) {
        e.preventDefault();
        last?.focus();
      } else if (!e.shiftKey && active === last) {
        e.preventDefault();
        first?.focus();
      }
    }
  }
</script>

<svelte:window onkeydown={keydown} />

<!-- The backdrop closes it with a click; the keyboard has Esc and the Close button. -->
<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={(e) => e.target === e.currentTarget && onclose()}>
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="whats-new-title" bind:this={dialog}>
    <div class="row head">
      <h2 id="whats-new-title" class="grow">What's new in PeriHub {notes.version}</h2>
      <button bind:this={closeButton} onclick={onclose}>Close</button>
    </div>

    <!-- Focusable so the keyboard can scroll it. -->
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div class="body col" role="region" aria-label="Release notes" tabindex="0">
      <p class="muted small">Released {notes.date}</p>
      {#if notes.summary}<p>{notes.summary}</p>{/if}
      {#each notes.sections as section (section.name)}
        {#if section.titles.length}
          <section class="col">
            <h3>{HEADINGS[section.name] ?? section.name}</h3>
            <ul>
              {#each section.titles as title, i (i)}<li>{title}</li>{/each}
            </ul>
          </section>
        {/if}
      {/each}
    </div>

    <div class="row foot">
      <a href={url} onclick={openNotes}>Full release notes</a>
      {#if error}<span class="small" role="alert">{error}</span>{/if}
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 50;
    display: grid;
    place-items: center;
    padding: 24px;
    background: var(--scrim);
    backdrop-filter: blur(3px);
  }

  .dialog {
    display: flex;
    flex-direction: column;
    gap: 12px;
    width: min(620px, 100%);
    max-height: 100%;
    min-height: 0;
    padding: 24px;
    border-radius: 14px;
    background: var(--panel);
    border: 1px solid var(--line-strong);
    box-shadow: var(--shadow-modal);
  }

  .head,
  .foot {
    flex: none;
  }

  .foot {
    gap: 12px;
  }

  .body {
    flex: 1 1 auto;
    min-height: 0;
    overflow-y: auto;
    gap: 16px;
    padding-right: 6px;
    border-radius: var(--radius-sm);
  }

  .body:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  section {
    gap: 6px;
  }

  ul {
    margin: 0;
    padding-left: 20px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
</style>
