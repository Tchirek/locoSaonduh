<script lang="ts">
  let {
    level = 0,
    tone = "ink",
    bars = 28,
  }: {
    level?: number;
    tone?: "ink" | "light" | "accent";
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
    height: 18px;
    align-items: center;
    gap: 3px;
  }

  span {
    width: 2px;
    height: 5px;
    flex: 1;
    background: currentColor;
    opacity: 0.15;
    transition: opacity 120ms linear;
  }

  span.on {
    opacity: 1;
  }

  .meter {
    color: var(--ink);
  }

  .light {
    color: var(--paper);
  }

  .accent {
    color: var(--accent);
  }
</style>
