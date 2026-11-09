<script lang="ts">
  // Asks before turning on experimental editing for an untested mouse model.
  import NoWarranty from "./NoWarranty.svelte";
  interface Props {
    /** The mouse, e.g. "G403 HERO". */
    model: string;
    /** The G300 has a protocol of its own: different risks and protections. */
    g300?: boolean;
    onconfirm: () => void;
    onclose: () => void;
  }

  let { model, g300 = false, onconfirm, onclose }: Props = $props();

  let understood = $state(false);

  function keydown(e: KeyboardEvent) {
    if (e.key === "Escape") onclose();
  }
</script>

<svelte:window onkeydown={keydown} />

<div class="backdrop">
  <div class="dialog col" role="dialog" aria-modal="true" aria-labelledby="experimental-title">
    <h2 id="experimental-title">Edit the {model} anyway?</h2>
    {#if g300}
      <p>
        PeriHub reads the G300 correctly, but has never saved to one. It saves a profile the way libratbag (Piper) does,
        and only with the values Logitech Gaming Software offers; whether the G300 keeps it after being unplugged has
        not been proven.
      </p>
      <ul>
        <li>
          <b>What could go wrong:</b> a profile that is not kept, or buttons and DPI that do not behave as set, until
          they are saved again or fixed with Logitech Gaming Software.
        </li>
        <li>
          <b>What PeriHub does to protect the mouse:</b> it backs up the three profiles before every save, writes only
          what you changed, and reads it back. The left and right buttons cannot be changed. After each save it asks
          whether to keep the changes, and puts the backup back by itself after 15 seconds without an answer. If PeriHub
          is closed or the mouse unplugged before that, it does so the next time the mouse is connected.
        </li>
        <li>
          <b>To turn it off:</b> here on the G300 page, or <b>Turn off experimental editing</b> in the tray menu.
        </li>
      </ul>
      <p class="muted small">If it works, please send a test report: it is how the G300 becomes fully supported.</p>
    {:else}
    <p>
      Nobody has tested PeriHub on the {model} yet. Its memory looks like the G502 HERO's, the mouse PeriHub was
      tested on, so saving should work, but it has not been proven.
    </p>
    <ul>
      <li>
        <b>What could go wrong:</b> a profile saved wrong, buttons or lighting that do not behave as set. In the worst
        case the mouse keeps a broken profile until it is fixed with a backup or with Logitech G HUB.
      </li>
      <li>
        <b>What PeriHub does to protect the mouse:</b> it accepts only the DPI and polling values the mouse itself
        reports, saves a backup before every write and reads every write back. After each save it asks whether to keep
        the changes, and puts the backup back by itself after 15 seconds without an answer. If PeriHub is closed or the
        mouse unplugged before that, it does so the next time the mouse is connected.
      </li>
      <li>
        <b>What stays off:</b> enabling or disabling profiles, and editing through a LIGHTSPEED receiver. Use the USB
        cable.
      </li>
      <li>
        <b>To turn it off:</b> here on the mouse page, or <b>Turn off experimental editing</b> in the tray menu.
      </li>
    </ul>
    <p class="muted small">
      If it works, please send a mouse test report: it is how the {model} becomes fully supported.
    </p>
    {/if}
    <NoWarranty device={`Logitech ${model}`} bind:accepted={understood} />
    <div class="row end">
      <button onclick={onclose}>Cancel</button>
      <button class="primary" disabled={!understood} onclick={onconfirm}>Turn on experimental editing</button>
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
