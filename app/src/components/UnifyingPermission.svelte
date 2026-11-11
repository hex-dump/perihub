<script lang="ts">
  // Linux: permission to open a Unifying receiver. In DJ mode the typing of the keyboards paired with
  // it comes through the same interface as HID++, so the permission opens that typing to every program
  // of the user: it comes with a full warning and can be removed. Shows nothing on Windows.
  import { onMount } from "svelte";
  import { api, errorText, type ReceiverReport, type UnifyingAccess } from "../lib/api";

  interface Props {
    receiver: ReceiverReport;
  }

  let { receiver }: Props = $props();
  let access = $state<UnifyingAccess | null>(null);
  let understood = $state(false);
  let busy = $state(false);
  let error = $state<string | null>(null);
  /** Just granted: PeriHub opens the receiver at its next look, a few seconds later. */
  let granted = $state(false);
  /** Just removed: the access already given to the receiver ends when it is plugged in again. */
  let revoked = $state(false);

  async function check() {
    try {
      access = await api.unifyingAccess();
    } catch (e) {
      error = errorText(e);
    }
  }

  async function change(grant: boolean) {
    busy = true;
    error = null;
    try {
      await (grant ? api.grantUnifyingAccess() : api.revokeUnifyingAccess());
      understood = false;
      granted = grant;
      revoked = !grant;
      await check();
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = false;
    }
  }

  onMount(() => {
    if (!navigator.userAgent.includes("Windows")) check();
  });
</script>

{#if access?.supported && receiver.denied && granted}
  <div class="notice info">
    Permission given. PeriHub opens the {receiver.name} within a few seconds; if it does not, unplug it and plug it in
    again.
  </div>
{:else if access?.supported && receiver.denied}
  <div class="notice warn col">
    <strong>PeriHub cannot open your {receiver.name}.</strong>
    <span>
      On Linux, what you type on the keyboards paired with it, and what the paired mice do, comes through the same
      channel PeriHub needs to talk to the devices. That is why PeriHub's own udev rule leaves it closed. To open it,
      know what it means:
    </span>
    <ul>
      <li>
        Linux cannot give this permission to PeriHub alone. <b>Every program that runs under your user account</b>
        will be able to read <b>every key you press on the keyboards paired with this receiver</b>, in any app, even
        while PeriHub is closed, and also on a keyboard you pair with it later.
      </li>
      <li>
        That includes <b>passwords, private messages and anything else you type</b>. A malicious or compromised
        program (a downloaded script, a game mod, a browser extension with access to your system) could use it as a
        keylogger without you noticing.
      </li>
      <li>
        PeriHub itself only reads the answers of the devices it asks: everything else that arrives on that channel is
        dropped at once, and never stored, logged or sent. The permission, though, stays until you remove it here.
      </li>
      <li>If Solaar is installed, its own rule already opens the receiver: then this is not needed.</li>
    </ul>
    <span>Allow it only if you trust all the software you run. Your password will be asked to install the rule.</span>
    <label class="row">
      <input type="checkbox" bind:checked={understood} />
      I understand that every program running as my user will be able to read what I type on keyboards paired with
      this receiver.
    </label>
    <div class="row">
      <button class="danger" disabled={!understood || busy} onclick={() => change(true)}>
        {busy ? "Waiting for the password…" : "Allow PeriHub to open the receiver"}
      </button>
    </div>
  </div>
{:else if access?.supported && revoked}
  <div class="notice info">
    Permission removed. To finish, unplug the {receiver.name} and plug it in again (or restart the computer).
  </div>
{:else if access?.supported && access.rule_installed}
  <div class="row small muted">
    <span class="grow">
      Programs running as your user can read what is typed on the keyboards paired with your {receiver.name}, because
      you allowed it in PeriHub.
    </span>
    <button class="danger" disabled={busy} onclick={() => change(false)}>Remove permission</button>
  </div>
{/if}
{#if error}
  <div class="notice error col">
    <span>{error}</span>
    {#if access}
      <span class="small">You can also run this in a terminal:</span>
      <code>{access.rule_installed ? access.manual_remove : access.manual_install}</code>
    {/if}
  </div>
{/if}

<style>
  .notice.warn {
    gap: 8px;
  }

  .notice ul {
    margin: 0;
    padding-left: 18px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  code {
    user-select: all;
    word-break: break-all;
  }
</style>
