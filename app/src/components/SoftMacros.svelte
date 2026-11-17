<script lang="ts">
  // Macros run by PeriHub: the list, the Stop button and, on Linux, the permission to simulate input.
  import { api, errorText, type SoftMacrosInfo } from "../lib/api";
  import { keyLabel } from "../lib/keys";
  import { softSummary } from "../lib/macros";
  import ConfirmButton from "./ConfirmButton.svelte";
  import Status from "./Status.svelte";
  import KeyAccessPermission from "./KeyAccessPermission.svelte";

  interface Props {
    info: SoftMacrosInfo;
    onchange: () => void;
  }

  let { info, onchange }: Props = $props();
  let understood = $state(false);
  let busy = $state(false);
  let error = $state<string | null>(null);

  async function run(task: () => Promise<unknown>) {
    busy = true;
    error = null;
    try {
      await task();
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = false;
      onchange();
    }
  }

  async function changeAccess(grant: boolean) {
    await run(async () => {
      await (grant ? api.grantSoftAccess() : api.revokeSoftAccess());
      understood = false;
      // udev applies the new permission a moment later.
      await new Promise((r) => setTimeout(r, 1500));
    });
  }
</script>

<div class="card col">
  <h2>Macros run by PeriHub</h2>
  <p class="muted small">
    They can repeat until you press the button again, type text and open websites or programs. They work while PeriHub
    is running, also in the tray. Mouse buttons and keyboard keys start one by sending a key of its own: F19, F24, then
    F13–F18 (up to 8 macros).
  </p>

  {#if error}<div class="notice error">{error}</div>{/if}

  {#if !info.status.available}
    <div class="notice error">PeriHub cannot simulate a keyboard and a mouse yet, so these macros cannot run.</div>
    {#if info.rule_supported}
      <div class="warning col">
        <b>Before you allow it, know what it means:</b>
        <ul>
          <li>PeriHub installs a rule that gives your user access to <code>/dev/uinput</code>, which creates virtual keyboards and mice.</li>
          <li>
            <b>Every program that runs under your user account gets the same access</b>, not only PeriHub: any of them
            could type and click anything as if it were you, for example commands in a terminal or “Allow” in a dialog.
          </li>
          <li>Without the rule a program needs administrator rights for that. Many gaming systems already have it, for game controllers.</li>
          <li>You can remove it here at any time; it ends when the PC restarts or the device is set up again.</li>
        </ul>
        <label class="row small"><input type="checkbox" bind:checked={understood} /> I understand</label>
        <div class="row">
          <button class="danger" disabled={!understood || busy} onclick={() => changeAccess(true)}>
            Allow PeriHub to simulate a keyboard and a mouse
          </button>
        </div>
      </div>
    {/if}
  {:else if info.rule_supported && !info.rule_installed}
    <p class="muted small">
      This PC already lets programs of your user simulate a keyboard and a mouse (another program, such as Steam, set it up).
    </p>
  {/if}
  {#if info.rule_installed}
    <div class="row small">
      <span class="muted">PeriHub's rule for /dev/uinput is installed.</span>
      <button class="ghost" disabled={busy} onclick={() => changeAccess(false)}>Remove permission</button>
    </div>
  {/if}

  {#if info.status.keyboard_error}
    <div class="notice error">
      A key of the Apex 3 TKL starts a macro, but PeriHub cannot read the keyboard's keys: {info.status.keyboard_error}
    </div>
    <KeyAccessPermission
      title="Starting macros from the keyboard needs permission to read what you type on it."
      ongranted={onchange}
    />
  {:else if info.status.keyboard_read}
    <p class="muted small">
      PeriHub reads the keys of the Apex 3 TKL, because one of them starts a macro. It acts only on the macro keys and
      drops every other key as it arrives.
    </p>
  {/if}

  {#if info.macros.length}
    <div class="list">
      {#each info.macros as m (m.key)}
        <div class="row item">
          <span class="chip">{keyLabel(m.key)}</span>
          <div class="col grow tight">
            <strong>{m.name}</strong>
            <span class="small muted">{softSummary(m)}</span>
          </div>
          {#if info.status.running.includes(m.key)}<Status kind="ok" label="Running" chip />{/if}
          <ConfirmButton class="ghost" disabled={busy} onconfirm={() => run(() => api.deleteSoftMacro(m.key))} confirmLabel="Delete?">
            Delete
          </ConfirmButton>
        </div>
      {/each}
    </div>
    <div class="row">
      <button disabled={!info.status.running.length} onclick={() => run(api.stopSoftMacros)}>Stop running macros</button>
      <span class="muted small">Buttons and keys whose macro is deleted keep sending its key until you change them.</span>
    </div>
  {:else}
    <p class="muted small">None yet: choose “Macro run by PeriHub” for a mouse button or a keyboard key.</p>
  {/if}
</div>

<style>
  .tight {
    gap: 2px;
  }

  .warning {
    gap: 8px;
    padding: 12px 14px;
    border: 1px solid rgb(var(--bad-rgb) / 0.3);
    border-left: 3px solid var(--bad);
    border-radius: var(--radius-sm);
    background: var(--bad-soft);
  }

  .warning ul {
    margin: 0;
    padding-left: 18px;
    line-height: 1.6;
  }

  .list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .item {
    gap: 12px;
  }</style>
