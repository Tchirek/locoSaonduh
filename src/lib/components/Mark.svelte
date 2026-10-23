<script lang="ts">
  import { onDestroy } from "svelte";

  let { size = 32, writhing = false }: { size?: number; writhing?: boolean } = $props();

  const REST =
    "M220 540 H268 C320 540 330 420 395 420 C460 420 452 636 525 636 C598 636 610 442 672 442 C724 442 742 512 804 540";
  const SEGMENTS = [
    [268, 540, 320, 540, 330, 420, 395, 420],
    [395, 420, 460, 420, 452, 636, 525, 636],
    [525, 636, 598, 636, 610, 442, 672, 442],
    [672, 442, 724, 442, 742, 512, 804, 540],
  ];
  const STEPS = 16;
  const START = 268;
  const END = 804;
  const RAMP = 110;
  const WAVENUMBER = (2 * Math.PI) / 277;
  const AMPLITUDE = 105;
  const SPEED = 4.2;
  const FADE_S = 0.7;
  const DOT = 58;
  const HOLE = 22;
  const POSE = WAVENUMBER * 395 + Math.PI / 2;

  const rest = [[220, 540]].concat(
    SEGMENTS.flatMap(([x0, y0, x1, y1, x2, y2, x3, y3], index) =>
      Array.from({ length: STEPS + (index === 0 ? 1 : 0) }, (_, step) => {
        const t = (step + (index === 0 ? 0 : 1)) / STEPS;
        const u = 1 - t;
        const a = u * u * u;
        const b = 3 * u * u * t;
        const c = 3 * u * t * t;
        const d = t * t * t;
        return [a * x0 + b * x1 + c * x2 + d * x3, a * y0 + b * y1 + c * y2 + d * y3];
      }),
    ),
  );

  let path = $state(REST);
  let dot = $state(DOT);
  let hole = $state(HOLE);

  let goal = 0;
  let envelope = 0;
  let phase = POSE;
  let frame = 0;
  let last = 0;
  type Settle = { from: number; span: number; envelope: number; elapsed: number; duration: number };
  let settle: Settle | null = null;

  function smooth(value: number) {
    const clamped = Math.min(1, Math.max(0, value));
    return clamped * clamped * (3 - 2 * clamped);
  }

  function reach(x: number) {
    return smooth((x - START) / RAMP) * smooth((END - x) / RAMP);
  }

  // 蛇の道 尾から頭へ 波送る
  function offset(x: number, weight: number) {
    const primary = Math.sin(WAVENUMBER * x - phase);
    const writhe = 0.26 * weight * weight * Math.sin(2 * WAVENUMBER * x - 1.6 * phase + 1);
    return AMPLITUDE * reach(x) * (primary + writhe);
  }

  function draw(weight: number) {
    path = rest
      .map(([x, y], index) => {
        const blended = y + (540 + offset(x, weight) - y) * weight;
        return `${index === 0 ? "M" : "L"}${x.toFixed(1)} ${blended.toFixed(1)}`;
      })
      .join(" ");
    // 波頭 届くたびごと 灯は息す
    const arrival = Math.sin(WAVENUMBER * 760 - phase);
    const breath = 1 + 0.1 * weight * (0.5 + 0.5 * arrival);
    dot = DOT * breath;
    hole = HOLE * breath;
  }

  function restPose() {
    path = REST;
    dot = DOT;
    hole = HOLE;
    phase = POSE;
    envelope = 0;
    settle = null;
    frame = 0;
    last = 0;
  }

  // 這ひ終へて 元の姿に 身を伏せる
  function beginSettle(): Settle {
    const turn = 2 * Math.PI;
    let span = (((POSE - phase) % turn) + turn) % turn;
    if (span < Math.PI / 4) span += turn;
    return { from: phase, span, envelope, elapsed: 0, duration: (Math.PI / 2) * (span / SPEED) };
  }

  function tick(now: number) {
    const elapsed = last ? Math.min(0.05, (now - last) / 1000) : 0;
    last = now;
    if (goal === 1) {
      settle = null;
      phase += elapsed * SPEED;
      envelope = Math.min(1, envelope + elapsed / FADE_S);
    } else {
      const motion = (settle ??= beginSettle());
      motion.elapsed += elapsed;
      const progress = Math.min(1, motion.elapsed / motion.duration);
      phase = motion.from + motion.span * Math.sin((progress * Math.PI) / 2);
      envelope = motion.envelope * (1 - smooth(progress));
      if (progress >= 1) {
        restPose();
        return;
      }
    }
    draw(smooth(envelope));
    frame = requestAnimationFrame(tick);
  }

  // ゆるやかに 目覚めまどろむ 穴惑ひ
  $effect(() => {
    const still = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    goal = writhing && !still ? 1 : 0;
    if (goal === 1 && frame === 0) {
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
  <path
    d={path}
    fill="none"
    stroke="var(--mark-ink, #f2f0e9)"
    stroke-width="64"
    stroke-linecap="round"
    stroke-linejoin="round"
  />
  <circle cx="812" cy="540" r={dot} fill="var(--accent, #e54820)" />
  <circle cx="812" cy="540" r={hole} fill="var(--mark-bg, #191b18)" />
</svg>

<style>
  .mark {
    display: block;
    flex: none;
  }
</style>
