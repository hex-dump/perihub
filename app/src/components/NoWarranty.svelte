<script lang="ts">
  // The disclaimer every experimental feature shows before it can be turned on, with the box the
  // user has to tick. Same words everywhere, so nobody can miss what they accept.
  interface Props {
    /** The device, e.g. "Logitech G403 HERO". */
    device: string;
    accepted: boolean;
  }

  let { device, accepted = $bindable() }: Props = $props();
</script>

<div class="warranty" role="note">
  <p><b>No warranty. You use this at your own risk.</b></p>
  <p>
    This feature has never been tested on the {device}. PeriHub is free software provided "as is", without any
    warranty, as its license says. If you turn it on, you alone are responsible for what happens: the author of PeriHub
    accepts no responsibility for any damage, malfunction or loss, to the {device}, to other devices, to your data or
    to anything else. If you are not willing to take that risk, press Cancel.
  </p>
</div>
<label class="row check">
  <input type="checkbox" bind:checked={accepted} />
  I have read this. I use this experimental feature at my own risk, and the author of PeriHub is not responsible for
  any damage.
</label>

<style>
  .warranty {
    padding: 12px 14px;
    border-radius: 10px;
    border: 1px solid var(--warn);
    background: var(--warn-soft, rgba(255, 170, 0, 0.08));
  }

  .warranty p {
    margin: 0;
  }

  .warranty p + p {
    margin-top: 6px;
  }

  .check {
    gap: 10px;
    align-items: flex-start;
  }
</style>
