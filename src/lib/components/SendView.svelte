<script lang="ts">
  import { Check, Radio, RefreshCw, Square } from "@lucide/svelte";
  import type { Snapshot } from "../types";
  import DeviceIcon from "./DeviceIcon.svelte";
  import LevelMeter from "./LevelMeter.svelte";

  let {
    snapshot,
    starting = false,
    stopping = false,
    scanning = false,
    refreshing = false,
    onStart,
    onStop,
    onSourceChange,
    onScan,
    onRefresh,
  }: {
    snapshot: Snapshot;
    starting?: boolean;
    stopping?: boolean;
    scanning?: boolean;
    refreshing?: boolean;
    onStart: (sourceId: string) => void;
    onStop: () => void;
    onSourceChange: (sourceId: string) => void;
    onScan: () => void;
    onRefresh: () => void;
  } = $props();

  let selectedSource = $derived(
    snapshot.sources.find((source) => source.id === snapshot.selectedSource) ??
      snapshot.sources[0],
  );
  let broadcast = $derived(snapshot.broadcast);
  let listeners = $derived(broadcast?.listeners ?? []);
  let hasReports = $derived(listeners.some((listener) => listener.reported));
  let audioBusy = $derived(snapshot.listening.length > 0 || snapshot.pending.length > 0);
  let peers = $derived(snapshot.peers);

  function elapsed(startedAtMs: number): string {
    const seconds = Math.max(0, Math.floor((Date.now() - startedAtMs) / 1000));
    const h = Math.floor(seconds / 3600);
    const m = Math.floor((seconds % 3600) / 60);
    const s = seconds % 60;
    return h > 0
      ? `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`
      : `${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
  }
</script>

<div class="page">
  {#if broadcast}
    <section class="onair card" aria-label="广播状态">
      <div class="onair-top">
        <span class="live-dot pulse"></span>
        <span class="onair-status">广播中</span>
        <span class="onair-time mono">{elapsed(broadcast.startedAtMs)}</span>
      </div>
      <div class="onair-source">
        <DeviceIcon platform={broadcast.sourceKind} active />
        <h1>{broadcast.source}</h1>
      </div>
      <LevelMeter level={broadcast.level} tone="accent" bars={40} />
      <dl class="stats">
        <div>
          <dt>码率</dt>
          <dd class="mono">{broadcast.quality.bitrateKbps}<small>kbps</small></dd>
        </div>
        <div>
          <dt>音质</dt>
          <dd>{broadcast.quality.tierLabel}</dd>
        </div>
        <div>
          <dt>抖动</dt>
          <dd class="mono">
            {#if hasReports}{broadcast.quality.jitterMs.toFixed(1)}<small>ms</small>{:else}—{/if}
          </dd>
        </div>
        <div>
          <dt>丢包</dt>
          <dd class="mono">
            {#if hasReports}{broadcast.quality.lossPct.toFixed(2)}<small>%</small>{:else}—{/if}
          </dd>
        </div>
      </dl>
      <button
        type="button"
        class="button button--block stop"
        onclick={onStop}
        disabled={stopping}
      >
        <Square size={12} fill="currentColor" strokeWidth={0} />
        {stopping ? "正在停止" : "停止广播"}
      </button>
    </section>

    <section class="section">
      <div class="section-head">
        <h2>听众</h2>
        <span class="count">{listeners.length}</span>
      </div>
      {#if listeners.length > 0}
        <div class="list">
          {#each listeners as listener (listener.id)}
            <div class="card row">
              <DeviceIcon platform="desktop" />
              <div class="row-main">
                <span class="row-title">{listener.alias}</span>
                <span class="row-sub mono">{listener.address}</span>
              </div>
              <span class="row-stat mono">
                {listener.bufferMs > 0 ? `${listener.bufferMs} ms` : "测速中"}
              </span>
              <span class="live-dot" class:idle={!listener.reported}></span>
            </div>
          {/each}
        </div>
      {:else}
        <p class="empty">等待收听…</p>
      {/if}
    </section>
  {:else}
    <section class="section">
      <div class="section-head">
        <h2>音源</h2>
        <div class="tools">
          <button
            type="button"
            class="icon-button"
            aria-label="刷新设备"
            title="刷新设备"
            onclick={onRefresh}
            disabled={refreshing}
          >
            <span class="spin" class:on={refreshing}><RefreshCw size={15} strokeWidth={1.8} /></span>
          </button>
        </div>
      </div>
      {#if snapshot.sources.length > 0}
        <div class="list" role="radiogroup" aria-label="音源">
          {#each snapshot.sources as source (source.id)}
            {@const selected = source.id === selectedSource?.id}
            <button
              type="button"
              class="card row source"
              class:selected
              role="radio"
              aria-checked={selected}
              onclick={() => !selected && onSourceChange(source.id)}
            >
              <DeviceIcon platform={source.kind} active={selected} />
              <span class="row-main">
                <span class="row-title">{source.name}</span>
                <span class="row-sub">{source.detail}</span>
              </span>
              {#if selected}
                <span class="check"><Check size={18} strokeWidth={2.2} /></span>
              {/if}
            </button>
          {/each}
        </div>
      {:else}
        <p class="empty card">没有可用的音频设备</p>
      {/if}
    </section>

    <div class="start">
      <button
        type="button"
        class="button button--accent button--block"
        disabled={starting || !selectedSource || audioBusy}
        onclick={() => selectedSource && onStart(selectedSource.id)}
      >
        <Radio size={18} strokeWidth={1.9} />
        {starting ? "正在打开…" : "开始广播"}
      </button>
      {#if audioBusy}
        <p class="hint">收听中，不能同时广播</p>
      {/if}
    </div>

    <section class="section">
      <div class="section-head">
        <h2>附近设备</h2>
        <span class="count">{peers.length}</span>
        <div class="tools">
          <button
            type="button"
            class="icon-button"
            aria-label="扫描"
            title="扫描"
            onclick={onScan}
            disabled={scanning}
          >
            <span class="spin" class:on={scanning}><RefreshCw size={15} strokeWidth={1.8} /></span>
          </button>
        </div>
      </div>
      {#if peers.length > 0}
        <div class="peers">
          {#each peers as peer (peer.id)}
            <span class="peer chip" title={peer.address}>
              {peer.alias}
              {#if peer.isBroadcasting}<span class="live-dot"></span>{/if}
            </span>
          {/each}
        </div>
      {:else}
        <p class="hint">暂无其他设备</p>
      {/if}
    </section>
  {/if}
</div>

<style>
  .onair {
    display: flex;
    flex-direction: column;
    gap: 14px;
    margin-bottom: 24px;
    padding: 16px;
  }

  .onair-top {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .onair-status {
    color: var(--accent);
    font-size: 13px;
    font-weight: 650;
  }

  .onair-time {
    margin-left: auto;
    color: var(--ink-3);
    font-size: 12px;
  }

  /* ともしびの 息するごとし 夜長かな */
  .pulse {
    animation: pulse 1.6s ease-in-out infinite;
  }

  @keyframes pulse {
    50% {
      opacity: 0.3;
    }
  }

  .onair-source {
    display: flex;
    min-width: 0;
    align-items: center;
    gap: 12px;
  }

  .onair-source h1 {
    overflow: hidden;
    font-size: 20px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .stats {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 6px;
    margin: 0;
  }

  .stats div {
    padding: 8px 10px;
    border-radius: var(--radius-sm);
    background: var(--bg);
  }

  dt {
    color: var(--ink-3);
    font-size: 11px;
  }

  dd {
    margin: 3px 0 0;
    font-size: 15px;
    font-weight: 600;
  }

  dd small {
    margin-left: 2px;
    color: var(--ink-3);
    font-size: 11px;
    font-weight: 400;
  }

  .stop {
    min-height: 40px;
    background: var(--bg);
  }

  .stop:hover:not(:disabled) {
    color: var(--on-inverse);
    background: var(--inverse);
  }

  .row-stat {
    color: var(--ink-2);
    font-size: 12px;
  }

  .source {
    width: 100%;
    border: 1.5px solid transparent;
    text-align: left;
    transition:
      background-color 140ms ease,
      border-color 140ms ease;
  }

  .source:hover {
    background: var(--card-hover);
  }

  .source.selected {
    border-color: var(--accent);
  }

  .check {
    display: grid;
    color: var(--accent);
  }

  .start {
    margin: 16px 0 24px;
  }

  .hint {
    margin-top: 8px;
    color: var(--ink-3);
    font-size: 12px;
    text-align: center;
  }

  .peers {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .peer {
    gap: 6px;
    height: 28px;
    padding: 0 10px;
    border-radius: 14px;
    background: var(--card);
    font-size: 12px;
  }

  @media (max-width: 480px) {
    .stats {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }
</style>
