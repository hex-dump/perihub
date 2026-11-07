<script lang="ts">
  // Backups PeriHub saved before writing to a device, newest first, each one with a Restore button.
  import { onMount } from "svelte";
  import { errorText, type BackupEntry } from "../lib/api";
  import { describeBackup } from "../lib/backups";
  import ConfirmButton from "./ConfirmButton.svelte";

  interface Props {
    /** The device, e.g. "G502 HERO". */
    device: string;
    load: () => Promise<BackupEntry[]>;
    /** Restores a backup; throws on error. The dialog is closed by the caller afterwards. */
    restore: (id: string) => Promise<void>;
    onclose: () => void;
  }

  let { device, load, restore, onclose }: Props = $props();

  const SHOWN = 10;

  let backups = $state<BackupEntry[] | null>(null);
  let showAll = $state(false);
  let restoring = $state<string | null>(null);
  let error = $state<string | null>(null);

  const visible = $derived(backups && !showAll ? backups.slice(0, SHOWN) : backups);

  onMount(async () => {
    try {
      backups = await load();
    } catch (e) {
      error = errorText(e);
      backups = [];
    }
  });

  async function run(id: string) {
    restoring = id;
    error = null;
    try {
      await restore(id);
    } catch (e) {
      error = errorText(e);
    } finally {
      restoring = null;
    }
  }

  function when(created: number): string {
    // English like the rest of the app.
    return new Date(created * 1000).toLocaleString("en-GB", { dateStyle: "medium", timeStyle: "short" });
  }

  function keydown(e: KeyboardEvent) {
    if (e.key === "Escape" && !restoring) onclose();
  }
</script>

<svelte:window onkeydown={keydown} />

<div class="backdrop">
  <div class="dialog col" role="dialog" aria-modal="true" aria-labelledby="backups-title">
    <h2 id="backups-title">Backups of the {device}</h2>
    <p class="muted small">
      Each backup is the device's memory just before PeriHub wrote to it. To undo your last save, restore the newest
      one. The current state is backed up first, so a restore can be undone too.
    </p>

    {#if error}<div class="notice error">{error}</div>{/if}

    {#if backups === null}
      <p class="muted">Loading…</p>
    {:else if backups.length === 0}
      <p class="muted">No backups yet: PeriHub saves one the first time it writes to the {device}.</p>
    {:else}
      <ul class="list">
        {#each visible ?? [] as backup (backup.id)}
          <li class="row">
            <div class="grow col entry">
              <span>{describeBackup(backup.label)}</span>
              <span class="muted small">{when(backup.created)}</span>
            </div>
            <ConfirmButton disabled={!!restoring} onconfirm={() => run(backup.id)} confirmLabel="Restore this backup?">
              {restoring === backup.id ? "Restoring…" : "Restore"}
            </ConfirmButton>
          </li>
        {/each}
      </ul>
      {#if !showAll && backups.length > SHOWN}
        <button class="link" onclick={() => (showAll = true)}>Show {backups.length - SHOWN} older backups</button>
      {/if}
    {/if}

    <div class="row end">
      <button disabled={!!restoring} onclick={onclose}>Close</button>
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
    background: rgb(0 0 0 / 0.6);
    backdrop-filter: blur(3px);
  }

  .dialog {
    width: min(620px, 100%);
    max-height: 100%;
    min-height: 0;
    gap: 12px;
    padding: 24px;
    border-radius: 14px;
    background: var(--panel);
    border: 1px solid var(--line-strong);
    box-shadow: 0 24px 64px rgba(0, 0, 0, 0.5);
  }

  h2 {
    margin: 0;
    font-size: 1.15rem;
  }

  p {
    margin: 0;
  }

  .list {
    flex: 0 1 auto;
    min-height: 0;
    overflow-y: auto;
    list-style: none;
    margin: 0;
    padding: 0;
    border: 1px solid var(--line);
    border-radius: 10px;
  }

  .list li {
    gap: 12px;
    padding: 10px 14px;
    border-bottom: 1px solid var(--line);
  }

  .list li:last-child {
    border-bottom: none;
  }

  .entry {
    gap: 2px;
  }

  .end {
    justify-content: flex-end;
  }

  .link {
    align-self: flex-start;
    background: none;
    border: none;
    padding: 0;
    color: var(--accent-2, var(--accent));
    cursor: pointer;
  }
</style>
