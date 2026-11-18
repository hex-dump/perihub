<script lang="ts">
  // Tests of the mouse in this window: polling rate, DPI and clicks. Read-only: nothing is sent to the
  // mouse, the page only looks at the events the window receives.
  import { onMount } from "svelte";
  import Term from "../components/Term.svelte";
  import {
    BUTTON_NAMES,
    CHATTER_MS,
    DOUBLE_MS,
    ClickTracker,
    PollingMeter,
    pollingSource,
    countsPerInch,
    refreshRate,
    type ClickKind,
    type DpiSource,
  } from "../lib/mouseTest";

  const REFRESH_EVERY_MS = 200;
  const LOG_LINES = 8;

  const source = pollingSource({
    raw: "onpointerrawupdate" in window,
    coalesced: typeof PointerEvent !== "undefined" && typeof PointerEvent.prototype.getCoalescedEvents === "function",
  });
  const meter = new PollingMeter({ source });
  let stats = $state(meter.stats());
  let refresh = $state<number | null>(null);
  let moving = $state(false);

  function onPointer(e: PointerEvent) {
    if (e.pointerType !== "mouse") return;
    moving = true;
    if (source === "coalesced") {
      const list = e.getCoalescedEvents();
      const events = list.length > 0 ? list : [e];
      meter.noteBurst(events.length);
      for (const c of events) meter.add(c.timeStamp);
    } else {
      meter.add(e.timeStamp);
    }
  }

  let pollArea = $state<HTMLElement>();
  $effect(() => {
    // pointerrawupdate is not in the DOM typings of every setup, so the listener is added by hand.
    const area = pollArea;
    if (!area) return;
    const type = source === "raw" ? "pointerrawupdate" : "pointermove";
    area.addEventListener(type, onPointer as EventListener);
    return () => area.removeEventListener(type, onPointer as EventListener);
  });

  function resetPolling() {
    meter.reset();
    stats = meter.stats();
    moving = false;
  }

  // --- DPI ---

  type DpiState = "idle" | "measuring" | "done";
  let dpiArea = $state<HTMLElement>();
  let distance = $state(10);
  let dpiState = $state<DpiState>("idle");
  let dpiSource = $state<DpiSource>("pointer");
  let dpiNote = $state<string | null>(null);
  let dx = $state(0);
  let dy = $state(0);
  const counts = $derived(Math.hypot(dx, dy));
  const cpi = $derived(countsPerInch(counts, distance));

  function onMove(e: MouseEvent) {
    dx += e.movementX;
    dy += e.movementY;
  }

  function onLockChange() {
    if (dpiState === "measuring" && document.pointerLockElement !== dpiArea) finish();
  }

  async function startDpi() {
    dx = 0;
    dy = 0;
    dpiNote = null;
    dpiSource = "pointer";
    dpiState = "measuring";
    const area = dpiArea;
    if (!area?.requestPointerLock) return;
    // Raw counts need a lock without the system's adjustments (the pointer speed and acceleration).
    try {
      await (area.requestPointerLock({ unadjustedMovement: true }) as unknown as Promise<void> | undefined);
      if (document.pointerLockElement === area) dpiSource = "raw";
    } catch {
      try {
        await (area.requestPointerLock() as unknown as Promise<void> | undefined);
      } catch {
        // No lock: the movement over the box is used.
      }
    }
  }

  function finish() {
    if (dpiState !== "measuring") return;
    dpiState = "done";
    if (document.pointerLockElement) document.exitPointerLock();
  }

  function onDpiKey(e: KeyboardEvent) {
    if (e.key === " " || e.key === "Enter") {
      e.preventDefault();
      if (dpiState === "measuring") finish();
      else startDpi();
    }
  }

  // --- Clicks ---

  const clicks = new ClickTracker();
  let clickRows = $state<{ button: number; presses: number; doubles: number; chatter: number; shortest: number | null }[]>([]);
  let log = $state<{ id: number; button: number; gap: number | null; kind: ClickKind }[]>([]);
  let logId = 0;

  function onClick(e: MouseEvent) {
    e.preventDefault();
    const kind = clicks.press(e.button, e.timeStamp);
    const s = clicks.get(e.button)!;
    log = [{ id: logId++, button: e.button, gap: s.last, kind }, ...log].slice(0, LOG_LINES);
    clickRows = clicks.buttons().map((button) => ({ button, ...clicks.get(button)! }));
  }

  /** Stops the extra buttons from going back or forward in the page, and the menu from opening. */
  const quiet = (e: Event) => e.preventDefault();

  function resetClicks() {
    clicks.reset();
    clickRows = [];
    log = [];
  }

  const buttonName = (b: number) => BUTTON_NAMES[b] ?? `Button ${b + 1}`;
  const hz = (n: number) => (n >= 100 ? Math.round(n) : n.toFixed(1)).toString();
  const worn = $derived(clickRows.filter((r) => r.chatter > 0));

  onMount(() => {
    const timer = setInterval(() => document.hidden || (stats = meter.stats()), REFRESH_EVERY_MS);
    // The screen refresh, to tell a mouse apart from a window that reports once for each frame.
    const gaps: number[] = [];
    let frame = 0;
    let before = 0;
    const tick = (t: number) => {
      if (before) gaps.push(t - before);
      if (gaps.length > 120) gaps.shift();
      before = t;
      if (gaps.length % 20 === 10) {
        refresh = refreshRate(gaps);
        meter.setRefresh(refresh);
      }
      frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    document.addEventListener("pointerlockchange", onLockChange);
    document.addEventListener("mousemove", onMove);
    return () => {
      clearInterval(timer);
      cancelAnimationFrame(frame);
      document.removeEventListener("pointerlockchange", onLockChange);
      document.removeEventListener("mousemove", onMove);
      if (document.pointerLockElement) document.exitPointerLock();
    };
  });
</script>

<header class="page-head">
  <h1>Mouse tester</h1>
</header>

<p class="lead">
  These tests only watch what this window receives from your mouse. Nothing is sent to the mouse, and it works with
  any mouse, including ones PeriHub does not support.
</p>

<div class="col gap">
  <section class="card col">
    <div class="row">
      <h2 class="grow"><Term term="polling">Polling rate</Term></h2>
      <button type="button" class="tiny" onclick={resetPolling}>Reset</button>
    </div>
    <p class="muted small">
      Move the mouse in circles over the box below, as fast as you can. A mouse reports its position many times a second;
      gaming mice do this 1000 times (1000 Hz) or more.
    </p>
    <div class="pad" role="application" aria-label="Move the mouse here" bind:this={pollArea}>
      {#if !moving}<span class="muted">Move the mouse here</span>{/if}
    </div>

    {#if source === "frames"}
      <div class="notice warn">
        <b>This window cannot measure the polling rate.</b> It reports the mouse once for each frame of the screen
        (about {refresh ? Math.round(refresh) : "60"} times a second), whatever the mouse does. PeriHub shows no number
        rather than a wrong one. On Windows the rate can be measured; on Linux use a tool such as
        <code>libinput debug-events</code> or a measuring website in a browser that supports it.
      </div>
    {:else if stats.verdict === "capped"}
      <div class="notice warn"><b>The number is not reliable.</b> {stats.reason} No polling rate is shown.</div>
    {:else if source === "coalesced"}
      <div class="notice info">
        This window does not report each movement as it happens, so the rate is worked out from the times of the
        events it hands over together. It is a good guide, and PeriHub shows nothing if it cannot trust it.
      </div>
    {/if}

    {#if source !== "frames" && stats.verdict !== "capped"}
      <div class="stats">
        <div class="stat"><span class="muted small">Now</span><b>{stats.verdict === "ok" ? hz(stats.live) : "–"}</b><span class="muted small">Hz</span></div>
        <div class="stat"><span class="muted small">Average</span><b>{stats.verdict === "ok" ? hz(stats.average) : "–"}</b><span class="muted small">Hz</span></div>
        <div class="stat"><span class="muted small">Lowest</span><b>{stats.verdict === "ok" && stats.min ? hz(stats.min) : "–"}</b><span class="muted small">Hz</span></div>
        <div class="stat"><span class="muted small">Highest</span><b>{stats.verdict === "ok" && stats.max ? hz(stats.max) : "–"}</b><span class="muted small">Hz</span></div>
      </div>
      {#if stats.verdict === "waiting"}
        <p class="muted small">{moving ? "Keep moving the mouse…" : "Waiting for movement."}</p>
      {/if}
    {/if}
  </section>

  <section class="card col">
    <h2>DPI check</h2>
    <p class="muted small">
      Put a ruler on the desk next to the mouse. Press Start, move the mouse straight along the ruler for the distance
      below, then press Stop (or Space). PeriHub divides the counts the mouse reported by the distance to get the DPI
      the mouse really uses.
    </p>
    <label class="row small">
      Distance
      <input type="number" min="1" max="100" step="0.5" bind:value={distance} disabled={dpiState === "measuring"} />
      cm
    </label>
    <div
      class="pad dpi"
      class:live={dpiState === "measuring"}
      role="button"
      tabindex="0"
      aria-label="DPI test area"
      bind:this={dpiArea}
      onclick={() => (dpiState === "measuring" ? finish() : startDpi())}
      onkeydown={onDpiKey}
    >
      {#if dpiState === "measuring"}
        <span><b>{Math.round(counts)}</b> counts · click or press Space to stop</span>
      {:else if dpiState === "done"}
        <span>Click or press Space to measure again</span>
      {:else}
        <span class="muted">Click here or press Space to start</span>
      {/if}
    </div>
    <div class="ruler" aria-hidden="true">
      {#each Array.from({ length: 11 }, (_, i) => i) as i (i)}<span class="tick" style:left="{i * 10}%"><i>{i * 10}%</i></span>{/each}
    </div>
    {#if dpiState === "done"}
      {#if cpi}
        <div class="stats">
          <div class="stat"><span class="muted small">Counts</span><b>{Math.round(counts)}</b></div>
          <div class="stat"><span class="muted small">Distance</span><b>{distance}</b><span class="muted small">cm</span></div>
          <div class="stat"><span class="muted small">Measured</span><b>{dpiSource === "raw" ? "" : "≈ "}{Math.round(cpi)}</b><span class="muted small">DPI</span></div>
        </div>
      {:else}
        <p class="muted small">No movement was measured. Try again.</p>
      {/if}
      {#if dpiSource === "raw"}
        <p class="muted small">
          Raw counts from the mouse, without the system's pointer speed or acceleration.
          {#if window.devicePixelRatio !== 1}
            Your screen scaling is {Math.round(window.devicePixelRatio * 100)}%, and some windows scale the movement by
            it: compare the result with the DPI you set.
          {/if}
        </p>
      {:else}
        <div class="notice warn">
          <b>This result is approximate.</b> This window cannot give the raw counts of the mouse, so the movement it
          sees has gone through the system's pointer speed and screen scaling. Turn off pointer acceleration (on
          Windows: Mouse settings → Additional → untick "Enhance pointer precision") and set the pointer speed to the
          middle, then try again.
        </div>
      {/if}
      <p class="muted small">Most mice are a few percent off their set DPI. A larger gap is worth a second try.</p>
    {/if}
    {#if dpiNote}<p class="muted small">{dpiNote}</p>{/if}
  </section>

  <section class="card col">
    <div class="row">
      <h2 class="grow">Click test</h2>
      <button type="button" class="tiny" onclick={resetClicks}>Reset</button>
    </div>
    <p class="muted small">
      Click each button once, slowly, a few times. Every click must count once. A press that comes less than {CHATTER_MS}
      ms after the one before is not a person: the switch is worn and registers double clicks you did not make.
      Presses within {DOUBLE_MS} ms are listed as double clicks.
    </p>
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div
      class="pad"
      role="application"
      aria-label="Click here"
      onmousedown={onClick}
      onmouseup={quiet}
      onauxclick={quiet}
      oncontextmenu={quiet}
    >
      <span class="muted">Click here with each button</span>
    </div>
    {#if worn.length > 0}
      <div class="notice error" role="alert">
        <b>Unwanted double clicks.</b>
        {worn.map((r) => `${buttonName(r.button)} button: ${r.chatter}`).join(", ")}. If this happens when you click
        once, the switch is failing, which is a hardware fault.
      </div>
    {:else if clickRows.length > 0}
      <div class="notice ok">No unwanted double clicks so far.</div>
    {/if}
    {#if clickRows.length > 0}
      <table>
        <thead>
          <tr><th>Button</th><th>Presses</th><th>Double clicks</th><th>Too fast ({CHATTER_MS} ms)</th><th>Shortest gap</th></tr>
        </thead>
        <tbody>
          {#each clickRows as r (r.button)}
            <tr>
              <td>{buttonName(r.button)}</td>
              <td>{r.presses}</td>
              <td>{r.doubles}</td>
              <td class:bad={r.chatter > 0}>{r.chatter}</td>
              <td>{r.shortest === null ? "–" : `${Math.round(r.shortest)} ms`}</td>
            </tr>
          {/each}
        </tbody>
      </table>
      <ul class="log muted small">
        {#each log as l (l.id)}
          <li class:bad={l.kind === "chatter"}>
            {buttonName(l.button)}:
            {l.gap === null || l.gap > DOUBLE_MS
              ? "single click"
              : `${Math.round(l.gap)} ms after the last press (${l.kind === "chatter" ? "too fast" : "double click"})`}
          </li>
        {/each}
      </ul>
    {/if}
  </section>
</div>

<style>
  .gap {
    gap: 18px;
    margin-top: 18px;
  }

  .pad {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 160px;
    border: 1px dashed var(--line-strong);
    border-radius: 8px;
    background: var(--panel-2);
    user-select: none;
    touch-action: none;
  }

  .pad.dpi {
    height: 90px;
    cursor: pointer;
  }

  .pad.live {
    border-style: solid;
    border-color: var(--accent);
  }

  .ruler {
    position: relative;
    height: 22px;
    margin: -6px 0 6px;
    border-top: 2px solid var(--line-strong);
  }

  .tick {
    position: absolute;
    top: 0;
    height: 8px;
    border-left: 2px solid var(--line-strong);
  }

  .tick i {
    position: absolute;
    top: 10px;
    left: -12px;
    font-size: 11px;
    font-style: normal;
    color: var(--faint);
  }

  .stats {
    display: flex;
    flex-wrap: wrap;
    gap: 28px;
  }

  .stat {
    display: flex;
    align-items: baseline;
    gap: 6px;
  }

  .stat span:first-child {
    min-width: 0;
    margin-right: 4px;
  }

  .stat b {
    font-size: 26px;
    font-variant-numeric: tabular-nums;
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 14px;
  }

  th,
  td {
    text-align: left;
    padding: 6px 8px;
    border-bottom: 1px solid var(--line);
  }

  .bad {
    color: var(--bad);
    font-weight: 600;
  }

  .log {
    margin: 0;
    padding-left: 18px;
  }
</style>
