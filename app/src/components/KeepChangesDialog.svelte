<script lang="ts">
  // After a save to an untested mouse: keep the changes, or the backup goes back by itself, like a
  // screen resolution change. The countdown needs no click, in case the mouse stopped clicking.
  import { onDestroy, onMount } from "svelte";

  interface Props {
    model: string;
    /** Seconds before the changes are undone. */
    seconds?: number;
    /** What happens without an answer, instead of putting the backup back. */
    undoText?: string;
    /** What to check, instead of the buttons that were changed. */
    checkText?: string;
    onkeep: () => void;
    onundo: () => void;
  }

  let {
    model,
    seconds = 15,
    undoText = "puts back the backup",
    checkText = "move it, click, try the buttons you changed",
    onkeep,
    onundo,
  }: Props = $props();

  let left = $state(0);
  let done = false;
  let timer: ReturnType<typeof setInterval> | undefined;
  let undoButton: HTMLButtonElement | undefined = $state();

  function finish(keep: boolean) {
    if (done) return;
    done = true;
    clearInterval(timer);
    if (keep) onkeep();
    else onundo();
  }

  onMount(() => {
    left = seconds;
    // The keyboard works even if the mouse does not. Enter and Escape both undo: keeping
    // needs a deliberate choice.
    undoButton?.focus();
    timer = setInterval(() => {
      left -= 1;
      if (left <= 0) finish(false);
    }, 1000);
  });

  onDestroy(() => clearInterval(timer));

  function keydown(e: KeyboardEvent) {
    if (e.key === "Escape") finish(false);
  }
</script>

<svelte:window onkeydown={keydown} />

<div class="backdrop">
  <div class="dialog col" role="alertdialog" aria-modal="true" aria-labelledby="keep-title" aria-describedby="keep-text">
    <h2 id="keep-title">Keep these changes?</h2>
    <p id="keep-text">
      Check that the {model} works as expected: {checkText}. Without an answer, PeriHub
      {undoText} in <b>{left}</b> seconds.
    </p>
    <div class="row end">
      <button bind:this={undoButton} onclick={() => finish(false)}>Undo now</button>
      <button class="primary" onclick={() => finish(true)}>Keep changes</button>
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 60;
    display: grid;
    place-items: center;
    padding: 24px;
    background: rgb(0 0 0 / 0.6);
    backdrop-filter: blur(3px);
  }

  .dialog {
    width: min(520px, 100%);
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

  .end {
    justify-content: flex-end;
    gap: 8px;
  }
</style>
