<script lang="ts">
  // Before the first save to a mouse whose memory format nobody has tested yet: what could go
  // wrong, what PeriHub does about it, and the "No warranty" box to tick. Asked once per mouse.
  import NoWarranty from "./NoWarranty.svelte";
  interface Props {
    /** The mouse, e.g. "G403 HERO". */
    model: string;
    profileFormat: number;
    sectorSize: number;
    /** The button that goes on with the write, e.g. "Save to mouse". */
    action: string;
    /** Through a LIGHTSPEED receiver, on a tested memory format: the radio is what is untested. */
    receiver?: boolean;
    onconfirm: () => void;
    onclose: () => void;
  }

  let { model, profileFormat, sectorSize, action, receiver = false, onconfirm, onclose }: Props = $props();

  let understood = $state(false);

  function keydown(e: KeyboardEvent) {
    if (e.key === "Escape") onclose();
  }
</script>

<svelte:window onkeydown={keydown} />

<div class="backdrop">
  <div class="dialog col" role="dialog" aria-modal="true" aria-labelledby="experimental-title">
    <h2 id="experimental-title">Save to the {model}?</h2>
    {#if receiver}
      <p>
        PeriHub has saved to the {model}'s memory format with the USB cable, but never through a LIGHTSPEED receiver.
        Logitech G HUB saves this way too, and the commands are the same as with the cable, so saving should work, but
        PeriHub has not proven it yet.
      </p>
    {:else}
      <p>
        The {model} keeps its profiles in a memory format PeriHub has never saved to on a real mouse (profile format
        {profileFormat}, {sectorSize}-byte sectors). PeriHub writes it the same way as on the mice it was tested on, so
        saving should work, but it has not been proven.
      </p>
    {/if}
    <ul>
      <li>
        <b>What could go wrong:</b> {#if receiver}a save that stops halfway, if the mouse turns off or loses its receiver
          while saving.{:else}a profile saved wrong, buttons or lighting that do not behave as set.{/if} In the worst
        case the mouse keeps a broken profile until it is fixed with a backup or with Logitech G HUB.
      </li>
      <li>
        <b>What PeriHub does to protect the mouse:</b> it accepts only the DPI and polling values the mouse itself
        reports, saves a backup before every write and reads every write back. After each save it asks whether to keep
        the changes, and puts the backup back by itself after 15 seconds without an answer. If PeriHub is closed or the
        mouse unplugged before that, it does so the next time the mouse is connected{#if receiver}.
          Through the receiver it does not save when the battery is below 15%.{:else} with its cable.{/if}
      </li>
      <li>
        {#if receiver}
          <b>What stays off:</b> enabling or disabling profiles. Use the USB cable for that.
        {:else}
          <b>What stays off:</b> enabling or disabling profiles, and saving through a LIGHTSPEED receiver. Use the USB
          cable.
        {/if}
      </li>
    </ul>
    <p class="muted small">
      PeriHub asks this only before your first save to the {model}{#if receiver} through its receiver{/if}. Whether it
      works or not, please send a mouse test report{#if !receiver}: one report makes this memory format tested for every
        mouse that uses it{/if}.
    </p>
    <NoWarranty device={`Logitech ${model}`} bind:accepted={understood} />
    <div class="row end">
      <button onclick={onclose}>Cancel</button>
      <button class="primary" disabled={!understood} onclick={onconfirm}>{action}</button>
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
    overflow-y: auto;
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

  p,
  ul {
    margin: 0;
  }

  ul {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding-left: 20px;
  }


  .end {
    justify-content: flex-end;
    gap: 8px;
  }
</style>
