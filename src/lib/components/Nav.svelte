<script lang="ts">
  import { Headphones, Radio, Settings } from "@lucide/svelte";
  import type { Tab } from "../types";

  let {
    tab,
    onTab,
    listening = 0,
    live = false,
  }: {
    tab: Tab;
    onTab: (tab: Tab) => void;
    listening?: number;
    live?: boolean;
  } = $props();

  const items = [
    { id: "receive", label: "接收", icon: Headphones },
    { id: "send", label: "发送", icon: Radio },
    { id: "settings", label: "设置", icon: Settings },
  ] as const;
</script>

<nav class="rail" aria-label="主要模式">
  {#each items as item (item.id)}
    <button
      type="button"
      class="item"
      class:active={tab === item.id}
      aria-current={tab === item.id ? "page" : undefined}
      onclick={() => onTab(item.id)}
    >
      <span class="pill"><item.icon size={20} strokeWidth={1.7} /></span>
      <span class="label">
        {item.label}
        {#if item.id === "receive" && listening > 0}
          <span class="count num">{listening}</span>
        {:else if item.id === "send" && live}
          <span class="dot" aria-label="广播中"></span>
        {/if}
      </span>
    </button>
  {/each}
</nav>

<style>
  .rail {
    display: flex;
    width: 72px;
    flex: none;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    padding: 16px 0;
    border-right: 1px solid var(--line);
  }

  .item {
    display: flex;
    width: 64px;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    padding: 4px 0;
    border-radius: var(--radius);
    color: var(--muted);
    transition: color 140ms ease;
  }

  .pill {
    display: grid;
    width: 52px;
    height: 30px;
    place-items: center;
    border-radius: 15px;
    transition: background-color 140ms ease;
  }

  .item:hover,
  .item.active {
    color: var(--ink);
  }

  .item:hover .pill {
    background: var(--hover);
  }

  .item.active .pill {
    background: var(--line-strong);
  }

  .label {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 12px;
  }

  .item.active .label {
    font-weight: 650;
  }

  .count {
    color: var(--accent);
    font-weight: 650;
  }

  @media (max-width: 560px) {
    .rail {
      width: 100%;
      flex-direction: row;
      justify-content: space-around;
      padding: 6px 8px 8px;
      border-top: 1px solid var(--line);
      border-right: 0;
    }
  }
</style>
