<script lang="ts">
  // Before the first save to the memory of a device of another brand: what could go wrong, what
  // PeriHub does about it, and the "No warranty" box to tick. Asked once per device.
  import NoWarranty from "./NoWarranty.svelte";

  interface Props {
    /** e.g. "ASUS ROG Keris". */
    device: string;
    /** Where PeriHub knows the protocol from. */
    source: string;
    onconfirm: () => void;
    onclose: () => void;
  }

  let { device, source, onconfirm, onclose }: Props = $props();

  let understood = $state(false);

  function keydown(e: KeyboardEvent) {
    if (e.key === "Escape") onclose();
  }
</script>

<svelte:window onkeydown={keydown} />

<div class="backdrop">
  <div class="dialog col" role="dialog" aria-modal="true" aria-labelledby="save-title">
    <h2 id="save-title">Save to the {device}'s memory?</h2>
    <p>
      PeriHub knows how to talk to the {device} from published documentation ({source}), not from a test on a real
      device. Saving to its memory should work the way that documentation describes, but nobody has proven it with
      PeriHub yet.
    </p>
    <ul>
      <li>
        <b>What could go wrong:</b> settings saved wrong, or a device that does not behave as set until it is fixed with
        PeriHub's backup or the manufacturer's app.
      </li>
      <li>
        <b>What PeriHub does to protect the device:</b> it sends only the commands in its list for this device, never
        anything that touches firmware, and only values the device accepts. It saves a backup of the settings first and
        reads them back after saving. Then it asks whether to keep the changes, and puts the backup back by itself after
        15 seconds without an answer. If PeriHub is closed or the device unplugged before that, it does so the next time
        the device is connected.
      </li>
    </ul>
    <p class="muted small">
      PeriHub asks this only before your first save to the {device}. Whether it works or not, please send a test report:
      it makes the device tested for everyone who has one.
    </p>
    <NoWarranty {device} bind:accepted={understood} />
    <div class="row end">
      <button onclick={onclose}>Cancel</button>
      <button class="primary" disabled={!understood} onclick={onconfirm}>Save to memory</button>
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
