<script lang="ts">
  // Keyboards with QMK firmware, found by their raw HID channel. Vial keyboards are read by themselves.
  // Any other keyboard with that channel is read only once the user turns it on here: its firmware may
  // answer that channel with code of its own. On Linux, an optional rule per keyboard opens only that
  // channel, after PeriHub checked that nothing typed arrives there.
  import { onMount } from "svelte";
  import { api, errorText, type QmkAccess, type QmkKeyboard } from "../lib/api";

  interface Props {
    keyboards: QmkKeyboard[];
    onopen: (key: string) => void;
  }

  let { keyboards, onopen }: Props = $props();
  let access = $state<QmkAccess | null>(null);
  let busy = $state<string | null>(null);
  let error = $state<{ key: string; text: string } | null>(null);
  /** The VIA keyboard whose warning is open, and whether its box is checked. */
  let asking = $state<string | null>(null);
  let understood = $state(false);
  /** Keyboards just allowed: PeriHub opens them at its next look, a few seconds later. */
  let granted = $state<string[]>([]);

  async function check() {
    try {
      access = await api.qmkAccess();
    } catch (e) {
      error = { key: "", text: errorText(e) };
    }
  }

  onMount(() => {
    check();
  });

  async function run(key: string, task: () => Promise<void>) {
    busy = key;
    error = null;
    try {
      await task();
      await check();
    } catch (e) {
      error = { key, text: errorText(e) };
    } finally {
      busy = null;
    }
  }

  const turnOn = (key: string) =>
    run(key, async () => {
      await api.qmkSetEnabled(key, true);
      asking = null;
      understood = false;
    });
  const turnOff = (key: string) => run(key, () => api.qmkSetEnabled(key, false));
  const grant = (key: string) =>
    run(key, async () => {
      await api.grantQmkAccess(key);
      granted = [...granted, key];
    });
  const revoke = (key: string) =>
    run(key, async () => {
      await api.revokeQmkAccess(key);
      granted = granted.filter((k) => k !== key);
    });
  const manualInstall = (key: string) => access?.manual_install.find(([k]) => k === key)?.[1] ?? null;
</script>

<header>
  <h1>QMK keyboards</h1>
  <span class="chip test">Experimental</span>
</header>

<div class="notice info">
  These keyboards have QMK firmware: PeriHub found them by the raw HID channel that VIA and Vial use. Nobody has tried
  PeriHub on one yet. PeriHub reads what the keyboard is and its lighting, and changes the lighting only if you choose
  to, with the same backup, read-back and undo as every other device. Keymaps and macros are left alone: use VIA or
  Vial for them. <b>Close VIA and Vial while PeriHub reads a keyboard</b>: two programs on the same channel get each
  other's answers.
</div>

{#each keyboards as k (k.key)}
  <section class="card col">
    <div class="row">
      <span class="grow col tight">
        <b>{k.brand} {k.name}</b>
        <span class="muted small">{k.key} · {k.vial ? "Vial firmware" : "QMK firmware, maybe VIA"}</span>
      </span>
      {#if k.open}
        <button class="primary" onclick={() => onopen(k.key)}>Open its page</button>
      {:else if k.enabled && !k.denied}
        <span class="muted small">Opening…</span>
      {/if}
    </div>

    {#if !k.vial && !k.enabled}
      {#if asking === k.key}
        <div class="notice warn col">
          <strong>Read the {k.name} as a VIA keyboard?</strong>
          <span>
            PeriHub will send it VIA's requests: protocol version, layers, macros and lighting. A keyboard whose firmware
            does not speak VIA may give the same bytes another meaning. Turn it on only if this keyboard works with VIA
            (usevia.app) or came with VIA support.
          </span>
          <label class="row">
            <input type="checkbox" bind:checked={understood} />
            This keyboard works with VIA.
          </label>
          <div class="row">
            <button onclick={() => ((asking = null), (understood = false))}>Cancel</button>
            <button class="primary" disabled={!understood || busy === k.key} onclick={() => turnOn(k.key)}>
              Read it
            </button>
          </div>
        </div>
      {:else}
        <div class="row">
          <span class="grow small">
            Not read: its firmware does not say it is VIA, so PeriHub waits for you to turn it on.
          </span>
          <button onclick={() => ((asking = k.key), (understood = false))}>Read it as a VIA keyboard…</button>
        </div>
      {/if}
    {:else if !k.vial}
      <div class="row small">
        <span class="grow muted">You turned it on as a VIA keyboard.</span>
        <button disabled={busy === k.key} onclick={() => turnOff(k.key)}>Stop reading it</button>
      </div>
    {/if}

    {#if k.enabled && k.denied && access?.supported}
      {#if granted.includes(k.key)}
        <div class="notice info">
          Permission given. PeriHub opens the {k.name} within a few seconds; if it does not, unplug it and plug it in
          again.
        </div>
      {:else}
        <div class="notice warn col">
          <strong>Linux does not let PeriHub open this keyboard yet.</strong>
          <span>
            PeriHub can add a rule that opens only this keyboard's raw HID channel, after checking that it carries
            nothing else: what you type arrives on another channel, which stays closed. Your password will be asked.
          </span>
          <div class="row">
            <button class="primary" disabled={busy === k.key} onclick={() => grant(k.key)}>
              {busy === k.key ? "Waiting for the password…" : "Allow PeriHub to open it"}
            </button>
          </div>
        </div>
      {/if}
    {:else if access?.supported && access.allowed.includes(k.key)}
      <div class="row small muted">
        <span class="grow">PeriHub's rule opens this keyboard's raw HID channel.</span>
        <button disabled={busy === k.key} onclick={() => revoke(k.key)}>Remove permission</button>
      </div>
    {/if}

    {#if error && error.key === k.key}
      <div class="notice error col">
        <span>{error.text}</span>
        {#if access && (manualInstall(k.key) || access.allowed.includes(k.key))}
          <span class="small">You can also run this in a terminal:</span>
          <code>{access.allowed.includes(k.key) ? access.manual_remove : manualInstall(k.key)}</code>
        {/if}
      </div>
    {/if}
  </section>
{:else}
  <div class="notice info">No keyboard with QMK firmware is connected.</div>
{/each}

{#if error && error.key === ""}<div class="notice error">{error.text}</div>{/if}

<style>
  header {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-bottom: 16px;
  }

  h1 {
    margin: 0;
  }

  section {
    gap: 10px;
    margin-bottom: 12px;
  }

  .notice.warn {
    gap: 8px;
  }

  code {
    user-select: all;
    white-space: pre-wrap;
    word-break: break-all;
  }
</style>
