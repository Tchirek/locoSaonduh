<script lang="ts">
  let {
    level = 0,
    tone = "ink",
    bars = 28,
  }: {
    level?: number;
    tone?: "ink" | "accent";
    bars?: number;
  } = $props();

  let segments = $derived(Array.from({ length: bars }));
  let count = $derived(Math.round(Math.min(1, Math.max(0, level)) * bars));
</script>

<div
  class="meter {tone}"
  role="meter"
  aria-label="音频电平"
  aria-valuemin="0"
  aria-valuemax="100"
  aria-valuenow={Math.round(Math.min(1, Math.max(0, level)) * 100)}
>
  {#each segments as _, index}
    <span class:on={index < count}></span>
  {/each}
</div>

<style>
  .meter {
    display: flex;
    width: 100%;
    height: 16px;
    align-items: center;
    gap: 2px;
    color: var(--ink);
  }

  span {
    height: 6px;
    flex: 1;
    border-radius: 1px;
    background: currentColor;
    opacity: 0.14;
    transition: opacity 120ms linear;
  }

  span.on {
    opacity: 1;
  }

  .accent {
    color: var(--accent);
  }
</style>
