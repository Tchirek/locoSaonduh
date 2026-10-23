<script lang="ts">
  import { onDestroy } from "svelte";

  let { size = 32, writhing = false }: { size?: number; writhing?: boolean } = $props();

  const WAVE =
    "M220 540 H268 C320 540 330 420 395 420 C460 420 452 636 525 636 C598 636 610 442 672 442 C724 442 742 512 804 540";
  const LENGTH = 1000;
  const HEAD = 230;
  const TAIL = 170;
  const TRAVEL_S = 1.35;
  const CYCLE_S = 1.9;
  const FILL_S = 0.5;
  const DIM_S = 0.35;
  const DIM = 0.26;
  const DOT = 58;
  const HOLE = 22;
  const REACH = LENGTH + HEAD + TAIL;
  const ARRIVE_S = (TRAVEL_S * Math.acos(1 - (2 * LENGTH * 0.95) / REACH)) / Math.PI;

  type Dash = { array: string; offset: number; shown: boolean };
  const HIDDEN: Dash = { array: `0 ${LENGTH * 2}`, offset: 0, shown: false };

  let track = $state(1);
  let bright = $state<Dash>(HIDDEN);
  let faint = $state<Dash>(HIDDEN);
  let dot = $state(DOT);
  let hole = $state(HOLE);

  let wanted = false;
  let cycling = false;
  let clock = 0;
  let dim = 0;
  let frame = 0;
  let last = 0;

  function span(from: number, to: number): Dash {
    const start = Math.max(0, from);
    const end = Math.min(LENGTH, to);
    if (end - start < 0.5) return HIDDEN;
    return { array: `${end - start} ${LENGTH * 2}`, offset: -start, shown: true };
  }

  function ease(value: number) {
    return (1 - Math.cos(Math.PI * Math.min(1, Math.max(0, value)))) / 2;
  }

  // 北斗めく 折れ道の川 明り行く
  function draw() {
    const head = cycling ? ease(clock / TRAVEL_S) * REACH : 0;
    bright = span(head - HEAD, head);
    faint = span(head - HEAD - TAIL, head - HEAD);
    // 辿り着き 朱き輪ひとつ 満ちにけり
    const since = clock - ARRIVE_S;
    const fill = cycling && since > 0 && since < FILL_S ? Math.sin((Math.PI * since) / FILL_S) : 0;
    hole = HOLE * (1 - fill);
    dot = DOT * (1 + 0.06 * fill);
    track = 1 - (1 - DIM) * ease(dim);
  }

  function settle() {
    track = 1;
    bright = HIDDEN;
    faint = HIDDEN;
    dot = DOT;
    hole = HOLE;
    clock = 0;
    dim = 0;
    frame = 0;
    last = 0;
  }

  function tick(now: number) {
    const elapsed = last ? Math.min(0.05, (now - last) / 1000) : 0;
    last = now;
    if (wanted && !cycling) {
      cycling = true;
      clock = 0;
    }
    if (cycling) {
      clock += elapsed;
      if (clock >= CYCLE_S) {
        clock -= CYCLE_S;
        cycling = wanted;
      }
    }
    // 流れ終へ 墨色戻る 秋の川
    const step = elapsed / DIM_S;
    dim = cycling ? Math.min(1, dim + step) : Math.max(0, dim - step);
    if (!cycling && dim === 0) {
      settle();
      return;
    }
    draw();
    frame = requestAnimationFrame(tick);
  }

  $effect(() => {
    const still = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    wanted = writhing && !still;
    if (wanted && frame === 0) {
      frame = requestAnimationFrame(tick);
    }
  });

  onDestroy(() => cancelAnimationFrame(frame));
</script>

<svg
  class="mark"
  width={size}
  height={size}
  viewBox="0 0 1024 1024"
  role="img"
  aria-label="locoSaonduh"
>
  <rect width="1024" height="1024" rx="232" fill="var(--mark-bg, #191b18)" />
  <g
    fill="none"
    stroke="var(--mark-ink, #f2f0e9)"
    stroke-width="64"
    stroke-linecap="round"
    stroke-linejoin="round"
  >
    <path d={WAVE} stroke-opacity={track} />
    {#if faint.shown}
      <path
        d={WAVE}
        pathLength={LENGTH}
        stroke-dasharray={faint.array}
        stroke-dashoffset={faint.offset}
        stroke-opacity="0.55"
      />
    {/if}
    {#if bright.shown}
      <path
        d={WAVE}
        pathLength={LENGTH}
        stroke-dasharray={bright.array}
        stroke-dashoffset={bright.offset}
      />
    {/if}
  </g>
  <circle cx="812" cy="540" r={dot} fill="var(--accent, #e54820)" />
  <circle cx="812" cy="540" r={hole} fill="var(--mark-bg, #191b18)" />
</svg>

<style>
  .mark {
    display: block;
    flex: none;
  }
</style>
