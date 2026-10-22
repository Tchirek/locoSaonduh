<script lang="ts">
  let { size = 32 }: { size?: number } = $props();
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
    d="M220 540 H268 C320 540 330 420 395 420 C460 420 452 636 525 636 C598 636 610 442 672 442 C724 442 742 512 804 540"
    fill="none"
    stroke="var(--mark-ink, #f2f0e9)"
    stroke-width="64"
    stroke-linecap="round"
    stroke-linejoin="round"
  />
  <circle cx="812" cy="540" r="58" fill="var(--accent, #e54820)" />
  <circle cx="812" cy="540" r="22" fill="var(--mark-bg, #191b18)" />
</svg>

<style>
  .mark {
    display: block;
    flex: none;
  }
</style>
