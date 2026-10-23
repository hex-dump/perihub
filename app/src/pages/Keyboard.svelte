<script lang="ts">
  import { untrack } from "svelte";
  import type { KeyboardInfo } from "../lib/api";
  import KeyboardLighting from "./KeyboardLighting.svelte";
  import KeyboardKeys from "./KeyboardKeys.svelte";

  interface Props {
    info: KeyboardInfo | null;
    error: string | null;
    /** A keyboard PeriHub recognizes but does not support yet, such as the Apex 3. */
    unsupported: string | null;
    initialTab?: string;
  }

  let { info, error, unsupported, initialTab }: Props = $props();
  let tab = $state<"lighting" | "keys">(untrack(() => initialTab) === "keys" ? "keys" : "lighting");
</script>

<header class="row">
  <div class="col tight grow">
    <h1>SteelSeries Apex 3 TKL</h1>
    <span class="muted small">
      {#if info}Firmware {info.firmware}{:else}Not connected: lighting settings will be applied when it is plugged in{/if}
    </span>
  </div>
</header>

{#if error && !info}
  <div class="notice error">{error}</div>
{/if}
{#if unsupported && !info}
  <div class="notice info">
    <b>{unsupported} connected.</b> PeriHub supports only the Apex 3 TKL for now, and it sends nothing to this keyboard
    until someone has tested it. If you can help, see issue #9 on PeriHub's GitHub page.
  </div>
{/if}

<div class="tabs">
  <button class:active={tab === "lighting"} onclick={() => (tab = "lighting")}>Lighting</button>
  <button class:active={tab === "keys"} onclick={() => (tab = "keys")}>Keys</button>
</div>

<section>
  {#if tab === "lighting"}
    <KeyboardLighting connected={!!info} />
  {:else if info}
    <KeyboardKeys />
  {:else}
    <div class="notice info">Connect the keyboard to edit its key mapping.</div>
  {/if}
</section>

<style>
  header {
    margin-bottom: 20px;
  }

  .tight {
    gap: 4px;
  }

  section {
    padding-top: 20px;
  }

  .notice {
    margin-bottom: 14px;
  }
</style>
