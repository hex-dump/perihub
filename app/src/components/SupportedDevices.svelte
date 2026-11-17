<script lang="ts">
  // Which brands and models PeriHub knows, and how far: counts per level, a brand opens to its models.
  import { brands as all, byLevel, type DeviceBrand } from "../lib/devices";
  import { levelStatus } from "../lib/status";
  import Status from "./Status.svelte";

  interface Props {
    brands?: DeviceBrand[];
  }

  let { brands = all }: Props = $props();
  const total = $derived(brands.reduce((n, b) => n + b.models.length, 0));
</script>

<section class="col" aria-labelledby="supported-devices">
  <h3 id="supported-devices">Supported devices</h3>
  <p class="muted small">
    {total} models from {brands.length} brands. Supported: tested on real hardware. Experimental: you can change
    settings at your own risk. Read-only: PeriHub reads the device and changes nothing.
  </p>
  <div class="brands">
    {#each brands as brand (brand.brand)}
      <details>
        <summary>
          <span class="name">{brand.brand}</span>
          <span class="counts">
            {#each byLevel(brand.models) as l (l.level)}
              <Status kind={levelStatus(l.level)} label={`${l.models.length} ${l.label.toLowerCase()}`} chip />
            {/each}
          </span>
        </summary>
        <div class="models small">
          {#each byLevel(brand.models) as l (l.level)}
            <p>
              <b>{l.label}</b> <span class="muted">({l.note})</span>:
              {l.models.map((m) => m.name).join(", ")}
            </p>
          {/each}
        </div>
      </details>
    {/each}
  </div>
</section>

<style>
  .brands {
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    background: var(--field);
  }

  details + details {
    border-top: 1px solid var(--line);
  }

  summary {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px 12px;
    padding: 8px 12px;
    cursor: pointer;
    font-size: 14px;
  }

  summary:hover {
    background: var(--panel-2);
  }

  summary:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }

  .name {
    min-width: 110px;
    font-weight: 600;
  }

  .counts {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .models {
    padding: 4px 12px 10px;
    line-height: 1.5;
  }

  .models p {
    margin: 6px 0 0;
  }
</style>
