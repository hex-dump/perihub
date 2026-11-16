<script lang="ts">
  // Everyone PeriHub takes code, data or facts from, grouped, in a dialog that scrolls. Esc or Close
  // closes it and focus goes back to what opened it.
  import { onMount } from "svelte";
  import { NOT_AFFILIATED, NOTICES_FILE, OWN_LICENSE, creditGroups } from "../lib/credits";

  interface Props {
    onclose: () => void;
  }

  let { onclose }: Props = $props();

  let dialog: HTMLDivElement | undefined = $state();
  let closeButton: HTMLButtonElement | undefined = $state();

  onMount(() => {
    const opener = document.activeElement;
    closeButton?.focus();
    return () => {
      if (opener instanceof HTMLElement) opener.focus();
    };
  });

  function keydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      e.preventDefault();
      onclose();
    } else if (e.key === "Tab" && dialog) {
      // Keep Tab inside the dialog while it is open.
      const items = [...dialog.querySelectorAll<HTMLElement>("button, [tabindex='0']")];
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
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="credits-title" bind:this={dialog}>
    <div class="row head">
      <h2 id="credits-title" class="grow">Credits</h2>
      <button bind:this={closeButton} onclick={onclose}>Close</button>
    </div>

    <!-- Focusable so the keyboard can scroll it. -->
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div class="body col" role="region" aria-label="List of credits" tabindex="0">
      <p class="muted small">
        PeriHub builds on the work of these projects. {NOTICES_FILE}
      </p>

      {#each creditGroups as group (group.id)}
        <section class="col" aria-labelledby="credits-{group.id}">
          <h3 id="credits-{group.id}">{group.title}</h3>
          {#if group.intro}<p class="muted small">{group.intro}</p>{/if}
          <ul class="list">
            {#each group.credits as credit (credit.name)}
              <li class="col">
                <div class="row wrap name">
                  <b>{credit.name}</b>
                  <span class="chip">{credit.license}</span>
                </div>
                <code class="link small">{credit.link}</code>
                {#each credit.copyright ?? [] as line (line)}
                  <span class="muted small">{line}</span>
                {/each}
                {#each credit.notes as note (note)}
                  <p class="small">{note}</p>
                {/each}
              </li>
            {/each}
          </ul>
        </section>
      {/each}

      <p class="muted small">{OWN_LICENSE} {NOT_AFFILIATED}</p>
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
    width: min(680px, 100%);
    max-height: 100%;
    min-height: 0;
    padding: 24px;
    border-radius: 14px;
    background: var(--panel);
    border: 1px solid var(--line-strong);
    box-shadow: var(--shadow-modal);
  }

  .head {
    flex: none;
  }

  .body {
    flex: 1 1 auto;
    min-height: 0;
    overflow-y: auto;
    gap: 20px;
    padding-right: 6px;
    border-radius: var(--radius-sm);
  }

  .body:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  section {
    gap: 8px;
  }

  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    border: 1px solid var(--line);
    border-radius: 10px;
  }

  .list li {
    gap: 4px;
    padding: 12px 14px;
    border-bottom: 1px solid var(--line);
  }

  .list li:last-child {
    border-bottom: none;
  }

  .name {
    gap: 8px;
  }

  .chip {
    white-space: normal;
    font-weight: 500;
  }

  .link {
    align-self: flex-start;
    color: var(--accent-2);
    overflow-wrap: anywhere;
    user-select: all;
  }
</style>
