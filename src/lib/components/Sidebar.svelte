<script lang="ts">
  import { Headphones, Radio, Settings } from "@lucide/svelte";
  import type { Identity, Network, Tab } from "../types";
  import Mark from "./Mark.svelte";

  let {
    tab,
    onTab,
    identity,
    network,
    available = 0,
    listening = 0,
    onSettings,
  }: {
    tab: Tab;
    onTab: (tab: Tab) => void;
    identity: Identity;
    network: Network;
    available?: number;
    listening?: number;
    onSettings: () => void;
  } = $props();

  let online = $derived(Boolean(network.ip));
</script>

<aside class="rail">
  <div class="brand">
    <Mark size={31} />
    <div class="brand-type">
      <span class="wordmark">locoSaonduh</span>
      <span class="sub">局域网音频</span>
    </div>
  </div>

  <nav aria-label="主要模式">
    <button
      type="button"
      class:active={tab === "send"}
      aria-current={tab === "send" ? "page" : undefined}
      onclick={() => onTab("send")}
    >
      <Radio size={19} strokeWidth={1.65} />
      <span class="label">发送</span>
    </button>
    <button
      type="button"
      class:active={tab === "receive"}
      aria-current={tab === "receive" ? "page" : undefined}
      onclick={() => onTab("receive")}
    >
      <Headphones size={19} strokeWidth={1.65} />
      <span class="label">接收</span>
      {#if listening > 0}
        <span class="rail-meta">{listening}</span>
      {:else if available > 0}
        <span class="rail-meta">{available}</span>
      {/if}
    </button>
  </nav>

  <div class="rail-foot">
    <div class="network" title={network.interface ?? "未连接网络"}>
      <span class:offline={!online} class="dot" aria-hidden="true"></span>
      <span class="network-label">{online ? "局域网在线" : "等待网络"}</span>
    </div>
    <div class="address mono">
      {network.ip ?? "—"}<span>:{network.port}</span>
    </div>
    <div class="identity">
      <span class="alias" title={identity.alias}>{identity.alias}</span>
      <button
        type="button"
        class="settings-button"
        aria-label="设置"
        title="设置"
        onclick={onSettings}
      >
        <Settings size={17} strokeWidth={1.7} />
      </button>
    </div>
  </div>
</aside>

<style>
  .rail {
    display: flex;
    width: var(--rail);
    min-width: var(--rail);
    height: 100%;
    flex-direction: column;
    padding: 26px 18px 18px;
    color: #e8e4d9;
    background: var(--ink);
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 11px;
    padding: 0 7px 34px;
    --mark-bg: #f2f0e9;
    --mark-ink: #191b18;
  }

  .brand-type {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 2px;
  }

  .wordmark {
    overflow: hidden;
    color: #f4f1e8;
    font-family: var(--display);
    font-size: 18px;
    font-weight: 690;
    letter-spacing: -0.05em;
    text-overflow: clip;
    white-space: nowrap;
  }

  .sub {
    color: #8f9387;
    font-size: 11px;
    font-weight: 500;
    letter-spacing: 0.08em;
  }

  nav {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  nav button {
    display: flex;
    width: 100%;
    min-height: 43px;
    align-items: center;
    gap: 12px;
    padding: 0 11px;
    border-left: 2px solid transparent;
    border-radius: 2px;
    color: #a8ac9f;
    text-align: left;
    transition:
      color 150ms ease,
      background-color 150ms ease;
  }

  nav button:hover {
    color: #f2f0e9;
    background: #232620;
  }

  nav button.active {
    color: #f8f5ec;
    background: #242721;
    border-left-color: var(--accent);
  }

  .label {
    flex: 1;
    font-size: 14px;
    font-weight: 560;
    letter-spacing: -0.01em;
  }

  .rail-meta {
    color: #8d9185;
    font-family: var(--mono);
    font-size: 11px;
  }

  nav button.active .rail-meta {
    color: var(--accent);
  }

  .rail-foot {
    margin-top: auto;
    padding-top: 22px;
    border-top: 1px solid #32352e;
  }

  .network {
    display: flex;
    align-items: center;
    gap: 8px;
    color: #afb3a6;
    font-size: 12px;
  }

  .dot {
    display: block;
    width: 6px;
    height: 6px;
    flex: none;
    border-radius: 50%;
    background: var(--accent);
  }

  .dot.offline {
    background: #72766c;
  }

  .address {
    margin-top: 7px;
    padding-left: 14px;
    overflow: hidden;
    color: #7e8277;
    font-size: 11px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .address span {
    color: #5c6056;
  }

  .identity {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 18px;
    padding-top: 14px;
    border-top: 1px solid #32352e;
  }

  .alias {
    overflow: hidden;
    flex: 1;
    color: #e8e4d9;
    font-size: 13px;
    font-weight: 560;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .settings-button {
    display: grid;
    width: 30px;
    height: 30px;
    flex: none;
    place-items: center;
    border: 1px solid transparent;
    border-radius: 2px;
    color: #898d81;
    transition:
      color 150ms ease,
      background-color 150ms ease,
      border-color 150ms ease;
  }

  .settings-button:hover {
    color: #f2f0e9;
    background: #282b25;
    border-color: #3a3d35;
  }

  @media (max-width: 950px) {
    .rail {
      width: 76px;
      min-width: 76px;
      align-items: center;
      padding: 24px 12px 18px;
    }

    .brand {
      padding: 0 0 30px;
    }

    .brand-type,
    .label,
    .rail-meta,
    .network-label,
    .address,
    .alias {
      display: none;
    }

    nav {
      width: 100%;
    }

    nav button {
      justify-content: center;
      padding: 0;
    }

    .network {
      justify-content: center;
    }

    .identity {
      justify-content: center;
    }
  }
</style>
