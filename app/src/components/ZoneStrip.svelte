<script lang="ts">
  import { onMount } from "svelte";
  import type { KeyboardLighting } from "../lib/api";
  import { previewFrame } from "../lib/effects";

  interface Props {
    lighting: KeyboardLighting;
    brightness: number;
  }

  let { lighting, brightness }: Props = $props();
  let colors = $state<string[]>([]);

  /** Effects whose preview changes over time; the others are drawn once. */
  const ANIMATED = ["breathing", "color_cycle", "reactive", "wave", "hardware_rainbow"];
  /** 30 frames per second are plenty for a preview and half the work of a 60 Hz screen. */
  const FRAME_MS = 1000 / 30;

  const start = performance.now();
  let frame = 0;
  let last = -Infinity;
  const animated = () => ANIMATED.includes(lighting.type);

  /** Animated only while someone looks at it: the window is in front and visible. */
  const watched = () => !document.hidden && document.hasFocus();

  function draw(now: number) {
    if (now - last >= FRAME_MS || !animated()) {
      colors = previewFrame(lighting, now - start);
      last = now;
    }
    frame = animated() && watched() ? requestAnimationFrame(draw) : 0;
  }

  function restart() {
    if (!frame) frame = requestAnimationFrame(draw);
  }

  // Drawn again when the lighting changes; an animation resumes when the window is in front again.
  $effect(() => {
    JSON.stringify(lighting);
    restart();
  });

  onMount(() => {
    document.addEventListener("visibilitychange", restart);
    window.addEventListener("focus", restart);
    return () => {
      cancelAnimationFrame(frame);
      document.removeEventListener("visibilitychange", restart);
      window.removeEventListener("focus", restart);
    };
  });
</script>

<div class="strip" style:opacity={0.25 + 0.75 * (brightness / 16)}>
  {#each colors as color, i (i)}
    <div class="zone" style:background={color} style:box-shadow="0 0 18px {color}"></div>
  {/each}
</div>

<style>
  .strip {
    display: grid;
    grid-template-columns: repeat(8, 1fr);
    gap: 6px;
    padding: 14px;
    border-radius: var(--radius);
    background: var(--stage);
    border: 1px solid var(--line);
  }

  .zone {
    height: 34px;
    border-radius: 6px;
  }
</style>
