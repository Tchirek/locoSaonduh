<script lang="ts">
  import { Headphones, Radio, Settings } from "@lucide/svelte";
  import type { Tab } from "../types";
  import Mark from "./Mark.svelte";

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
  <div class="brand"><Mark size={30} /></div>
  {#each items as item (item.id)}
    <button
      type="button"
      class="item"
      class:active={tab === item.id}
      aria-current={tab === item.id ? "page" : undefined}
      onclick={() => onTab(item.id)}
    >
      <span class="pill">
        <item.icon size={20} strokeWidth={1.7} />
        {#if item.id === "receive" && listening > 0}
          <span class="badge mono">{listening}</span>
        {:else if item.id === "send" && live}
          <span class="badge badge--dot" aria-hidden="true"></span>
        {/if}
      </span>
      <span class="label">{item.label}</span>
    </button>
  {/each}
</nav>

<style>
  .rail {
    display: flex;
    width: 76px;
    flex: none;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding: 16px 0;
    background: var(--bg);
    border-right: 1px solid var(--line);
  }

  .brand {
    padding-bottom: 14px;
  }

  .item {
    display: flex;
    width: 64px;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    padding: 4px 0;
    color: var(--ink-2);
  }

  .pill {
    position: relative;
    display: grid;
    width: 52px;
    height: 30px;
    place-items: center;
    border-radius: 15px;
    transition: background-color 140ms ease;
  }

  .item:hover .pill {
    background: var(--card-hover);
  }

  .item.active {
    color: var(--ink);
  }

  .item.active .pill {
    color: var(--on-accent-soft);
    background: var(--accent-soft);
  }

  .label {
    font-size: 12px;
    font-weight: 500;
  }

  .item.active .label {
    font-weight: 650;
  }

  .badge {
    position: absolute;
    top: 1px;
    right: 9px;
    display: grid;
    min-width: 16px;
    height: 16px;
    place-items: center;
    padding: 0 4px;
    border-radius: 8px;
    color: var(--on-accent);
    background: var(--accent);
    font-size: 10px;
    font-weight: 600;
  }

  .badge--dot {
    top: 4px;
    right: 13px;
    min-width: 8px;
    width: 8px;
    height: 8px;
    padding: 0;
  }

  @media (max-width: 560px) {
    .rail {
      width: 100%;
      flex-direction: row;
      justify-content: space-around;
      padding: 6px 0 8px;
      border-top: 1px solid var(--line);
      border-right: 0;
    }

    .brand {
      display: none;
    }
  }
</style>
