<script lang="ts">
  import { AudioLines, ChevronDown, Radio, RefreshCw, Square } from "@lucide/svelte";
  import type { Peer, Snapshot } from "../types";
  import DeviceIcon from "./DeviceIcon.svelte";
  import LevelMeter from "./LevelMeter.svelte";
  import Mark from "./Mark.svelte";

  let {
    snapshot,
    starting = false,
    stopping = false,
    scanning = false,
    onStart,
    onStop,
    onSourceChange,
    onScan,
    onSettings,
  }: {
    snapshot: Snapshot;
    starting?: boolean;
    stopping?: boolean;
    scanning?: boolean;
    onStart: (sourceId: string) => void;
    onStop: () => void;
    onSourceChange: (sourceId: string) => void;
    onScan: () => void;
    onSettings: () => void;
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

  function peerLabel(peer: Peer): string {
    if (peer.isListening) return "正在收听";
    if (peer.isBroadcasting) return "也在广播";
    return "在线";
  }

  function fixed(value: number, digits = 1): string {
    return value.toFixed(digits);
  }
</script>

<div class="page">
  {#if broadcast}
    <header class="page-head live-head">
      <div>
        <p class="mode-label">发送</p>
        <h1>正在广播</h1>
        <p class="lede">
          {#if listeners.length > 0}
            同一局域网内的设备可以在接收页听到这段声音，已有 {listeners.length} 台设备正在收听。
          {:else}
            同一局域网内的设备可以在接收页听到这段声音。第一位听众还没有出现。
          {/if}
        </p>
      </div>
      <div class="head-actions">
        <button
          type="button"
          class="button stop-button"
          onclick={onStop}
          disabled={stopping}
        >
          <Square size={13} fill="currentColor" strokeWidth={0} />
          {stopping ? "正在停止" : "停止广播"}
        </button>
      </div>
    </header>

    <section class="on-air" aria-label="广播状态">
      <div class="onair-top">
        <span class="onair-status">
          <span class="live-dot" aria-hidden="true"></span>
          正在广播
        </span>
        <span class="onair-time mono">{elapsed(broadcast.startedAtMs)}</span>
      </div>

      <div class="onair-main">
        <div class="onair-copy">
          <h2>{broadcast.source}</h2>
          <p class="onair-meta">
            Opus · 48 kHz · 立体声
            <span aria-hidden="true">/</span>
            自适应码率
          </p>
        </div>
        <div class="onair-meter">
          <LevelMeter level={broadcast.level} tone="light" bars={44} />
        </div>
      </div>

      <dl class="onair-stats">
        <div>
          <dt>当前码率</dt>
          <dd class="mono">{broadcast.quality.bitrateKbps}<small> kbps</small></dd>
        </div>
        <div>
          <dt>音质档</dt>
          <dd>{broadcast.quality.tierLabel}</dd>
        </div>
        <div>
          <dt>网络抖动</dt>
          <dd class="mono">
            {#if hasReports}
              {fixed(broadcast.quality.jitterMs)}<small> ms</small>
            {:else}
              —
            {/if}
          </dd>
        </div>
        <div>
          <dt>丢包</dt>
          <dd class="mono">
            {#if hasReports}
              {fixed(broadcast.quality.lossPct, 2)}<small> %</small>
            {:else}
              —
            {/if}
          </dd>
        </div>
        <div>
          <dt>听众</dt>
          <dd class="mono">{listeners.length}<small> 台</small></dd>
        </div>
      </dl>

      <p class="policy-note">
        <span>自动权衡</span>
        链路干净时提高码率；抖动或丢包出现时，优先保持连续与低延迟。
      </p>
    </section>

    <section class="listeners">
      <div class="section-head">
        <h2>正在收听</h2>
        <span class="meta">{listeners.length === 0 ? "等待听众" : `${listeners.length} 台已连接`}</span>
      </div>
      {#if listeners.length === 0}
        <div class="waiting-block">
          <div class="waiting-mark"><Mark size={34} /></div>
          <div>
            <p class="waiting-title">在另一台设备打开「接收」。</p>
            <p class="waiting-copy">
              这台设备会自动出现在对方的附近广播里，点一下「收听」即可。
            </p>
          </div>
        </div>
      {:else}
        <div class="listener-table">
          {#each listeners as listener (listener.id)}
            <div class="listener-row">
              <DeviceIcon platform="desktop" size={17} />
              <div class="listener-name">
                <span>{listener.alias}</span>
                <small class="mono">{listener.address}</small>
              </div>
              <div class="listener-stat">
                <span>缓冲</span>
                <b class="mono">{listener.bufferMs > 0 ? `${listener.bufferMs} ms` : "测速中"}</b>
              </div>
              <div class="listener-stat listener-stat--jitter">
                <span>抖动</span>
                <b class="mono">{fixed(listener.jitterMs)} ms</b>
              </div>
              <div class="listener-stat listener-stat--loss">
                <span>丢包</span>
                <b class="mono">{fixed(listener.lossPct, 2)}%</b>
              </div>
              <div class="listener-state">
                <span class="live-dot" aria-hidden="true"></span>
                {listener.reported ? "稳定" : "连接中"}
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </section>

    <section class="nearby-live">
      <div class="section-head">
        <h2>同一网络</h2>
        <button
          type="button"
          class="button button--quiet button--small"
          onclick={onScan}
          disabled={scanning}
        >
          <span class="spin-wrap" class:spin={scanning}>
            <RefreshCw size={13} />
          </span>
          {scanning ? "扫描中" : "重新扫描"}
        </button>
      </div>
      <div class="compact-peers">
        {#each peers as peer (peer.id)}
          <span class="compact-peer">
            <DeviceIcon platform={peer.platform} size={14} />
            {peer.alias}
            <em>{peerLabel(peer)}</em>
          </span>
        {:else}
          <span class="quiet">没有发现其他设备。</span>
        {/each}
      </div>
    </section>
  {:else}
    <header class="page-head">
      <div>
        <p class="mode-label">发送</p>
        <h1>广播这一台的声音</h1>
        <p class="lede">
          采集系统声音或麦克风，用 Opus 编成低延迟音频，经局域网直接送往正在收听的设备。
        </p>
      </div>
      <div class="head-actions">
        <button
          type="button"
          class="button button--quiet button--small"
          onclick={onScan}
          disabled={scanning}
        >
          <span class="spin-wrap" class:spin={scanning}>
            <RefreshCw size={13} />
          </span>
          {scanning ? "扫描中" : "扫描附近"}
        </button>
      </div>
    </header>

    <div class="setup-grid">
      <section class="setup" aria-labelledby="source-title">
        <div class="section-head">
          <h2 id="source-title">广播设置</h2>
          <span class="meta">自动模式</span>
        </div>

        <label class="field">
          <span class="field-label">音源</span>
          <span class="select-wrap">
            <select
              value={snapshot.selectedSource ?? ""}
              onchange={(event) =>
                onSourceChange(event.currentTarget.value)}
              disabled={snapshot.sources.length === 0}
            >
              {#each snapshot.sources as source (source.id)}
                <option value={source.id}>
                  {source.kind === "system" ? "系统 · " : "麦克风 · "}{source.name}
                </option>
              {/each}
            </select>
            <ChevronDown size={15} strokeWidth={1.6} aria-hidden="true" />
          </span>
        </label>
        <p class="source-detail">
          {selectedSource?.detail ?? "没有可用的音频设备"}
        </p>

        <div class="auto-block">
          <div class="auto-title">
            <AudioLines size={16} strokeWidth={1.6} />
            <span>自动权衡</span>
          </div>
          <p>
            不设固定旋钮。发送端跟随链路状态，在清晰度、连续性与延迟之间取当前最优；抖动出现时先保连续，链路干净后再把码率升回来。
          </p>
          <dl class="range-list">
            <div><dt>码率</dt><dd class="mono">64–160 kbps</dd></div>
            <div><dt>接收缓冲</dt><dd class="mono">30–120 ms</dd></div>
            <div><dt>帧长</dt><dd class="mono">10 ms</dd></div>
          </dl>
        </div>

        {#if audioBusy}
          <p class="blocked-note">
            这台设备正在收听。断开所有收听流后，才能开始广播。
          </p>
        {/if}

        <button
          type="button"
          class="button button--accent start-button"
          disabled={starting || !selectedSource || audioBusy}
          onclick={() => selectedSource && onStart(selectedSource.id)}
        >
          <Radio size={17} strokeWidth={1.8} />
          {starting ? "正在打开音频设备" : "开始广播"}
        </button>
        <button type="button" class="change-output" onclick={onSettings}>
          更换音源与设备
        </button>
        <p class="pairing-note">无需配对。开始后，对方在接收页点「收听」即可加入。</p>
      </section>

      <section class="nearby" aria-labelledby="nearby-title">
        <div class="section-head">
          <h2 id="nearby-title">附近设备</h2>
          <span class="meta">{peers.length} 台设备</span>
        </div>

        {#if peers.length > 0}
          <div class="peer-list">
            {#each peers as peer (peer.id)}
              <div class="peer-row" class:has-broadcast={peer.isBroadcasting}>
                <DeviceIcon platform={peer.platform} size={20} />
                <div class="peer-copy">
                  <div class="peer-name">
                    {peer.alias}
                    {#if peer.isBroadcasting}
                      <span class="inline-status">广播中</span>
                    {/if}
                  </div>
                  <p>
                    {peer.isBroadcasting
                      ? `${peer.broadcast?.source ?? "声音"} · ${peer.broadcast?.listeners ?? 0} 人收听`
                      : `${peer.address} · 可收听`}
                  </p>
                </div>
              </div>
            {/each}
          </div>
        {:else}
          <div class="empty-peers">
            <Mark size={33} />
            <p class="empty-title">这张网里还没有别的设备。</p>
            <p>在另一台设备打开 locoSaonduh，通常两秒内就会出现在这里。</p>
          </div>
        {/if}

        <div class="nearby-foot">
          <span class="live-dot" aria-hidden="true"></span>
          <span>
            UDP 组播 + 广播 · 端口 {snapshot.network.port}
          </span>
        </div>
      </section>
    </div>
  {/if}
</div>

<style>
  .page {
    padding: 38px 44px 58px;
  }

  .page-head {
    align-items: flex-start;
  }

  .head-actions {
    padding-top: 5px;
  }

  .live-head .lede {
    max-width: 560px;
  }

  .stop-button {
    color: var(--ink);
    background: transparent;
    border-color: var(--rule-strong);
  }

  .stop-button:hover:not(:disabled) {
    background: var(--ink);
    border-color: var(--ink);
    color: var(--paper);
  }

  .on-air {
    margin-top: 24px;
    padding: 25px 28px 22px;
    color: #eeeade;
    background: var(--ink);
    border-radius: 3px;
  }

  .onair-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding-bottom: 18px;
    border-bottom: 1px solid #3a3d35;
  }

  .onair-status {
    display: flex;
    align-items: center;
    gap: 9px;
    color: #f2efe4;
    font-size: 13px;
    font-weight: 600;
    letter-spacing: 0.01em;
  }

  .live-dot {
    display: inline-block;
    width: 6px;
    height: 6px;
    flex: none;
    border-radius: 50%;
    background: var(--accent);
  }

  .onair-time {
    color: #8d9185;
    font-size: 13px;
  }

  .onair-main {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(200px, 0.72fr);
    gap: 36px;
    align-items: end;
    padding: 24px 0 20px;
  }

  .onair-copy h2 {
    overflow: hidden;
    color: #f6f2e8;
    font-size: clamp(30px, 3.5vw, 45px);
    font-weight: 620;
    line-height: 1.06;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .onair-meta {
    display: flex;
    flex-wrap: wrap;
    gap: 7px;
    margin-top: 9px;
    color: #969a8e;
    font-family: var(--mono);
    font-size: 11px;
  }

  .onair-meter {
    padding-bottom: 10px;
  }

  .onair-stats {
    display: grid;
    grid-template-columns: repeat(5, minmax(0, 1fr));
    gap: 0;
    margin: 0;
    border-top: 1px solid #3a3d35;
    border-bottom: 1px solid #3a3d35;
  }

  .onair-stats > div {
    padding: 16px 20px 17px 0;
  }

  .onair-stats > div + div {
    padding-left: 20px;
    border-left: 1px solid #3a3d35;
  }

  dt {
    color: #8c9084;
    font-size: 11px;
    font-weight: 500;
  }

  dd {
    margin: 7px 0 0;
    color: #f5f1e6;
    font-family: var(--display);
    font-size: 17px;
    font-weight: 620;
    letter-spacing: -0.02em;
  }

  dd small {
    margin-left: 3px;
    color: #b0b4a7;
    font-family: var(--mono);
    font-size: 11px;
    font-weight: 400;
    letter-spacing: 0;
  }

  .policy-note {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    padding-top: 17px;
    color: #94988c;
    font-size: 12px;
    line-height: 1.6;
  }

  .policy-note span {
    color: #c9c4b6;
    font-weight: 600;
  }

  .listeners {
    margin-top: 36px;
  }

  .waiting-block {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 22px 0 24px;
    border-top: 1px solid var(--rule);
    border-bottom: 1px solid var(--rule);
  }

  .waiting-mark {
    --mark-bg: #e6e2d7;
    --mark-ink: #6f7367;
  }

  .waiting-title {
    font-size: 14px;
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  .waiting-copy {
    margin-top: 4px;
    color: var(--ink-secondary);
    font-size: 13px;
    line-height: 1.6;
  }

  .listener-table {
    border-top: 1px solid var(--rule);
  }

  .listener-row {
    display: grid;
    grid-template-columns: 26px minmax(150px, 1fr) repeat(3, minmax(74px, auto)) minmax(68px, auto);
    gap: 14px;
    align-items: center;
    min-height: 57px;
    border-bottom: 1px solid var(--rule);
  }

  .listener-row :global(.device-icon) {
    width: 24px;
    height: 24px;
  }

  .listener-name {
    display: flex;
    min-width: 0;
    align-items: baseline;
    gap: 9px;
  }

  .listener-name span {
    overflow: hidden;
    font-size: 13px;
    font-weight: 590;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .listener-name small {
    overflow: hidden;
    color: var(--ink-muted);
    font-size: 10px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .listener-stat {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .listener-stat span {
    color: var(--ink-muted);
    font-size: 10px;
  }

  .listener-stat b {
    font-size: 12px;
    font-weight: 500;
  }

  .listener-state {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 6px;
    color: var(--ink-secondary);
    font-size: 11px;
  }

  .nearby-live {
    margin-top: 34px;
  }

  .compact-peers {
    display: flex;
    flex-wrap: wrap;
    gap: 7px 22px;
    padding: 15px 0;
    border-top: 1px solid var(--rule);
    border-bottom: 1px solid var(--rule);
  }

  .compact-peer {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    color: var(--ink-secondary);
    font-size: 12px;
  }

  .compact-peer em {
    color: var(--ink-muted);
    font-style: normal;
  }

  .compact-peer :global(.device-icon) {
    width: 19px;
    height: 19px;
  }

  .quiet {
    color: var(--ink-muted);
    font-size: 12px;
  }

  .setup-grid {
    display: grid;
    grid-template-columns: minmax(0, 1.05fr) minmax(320px, 0.95fr);
    gap: 0;
    margin-top: 30px;
  }

  .setup {
    padding: 26px 42px 0 0;
    border-right: 1px solid var(--rule);
  }

  .nearby {
    padding: 26px 0 0 36px;
  }

  .field {
    display: block;
  }

  .field-label {
    display: block;
    margin-bottom: 8px;
    color: var(--ink-secondary);
    font-size: 12px;
  }

  .select-wrap {
    position: relative;
    display: flex;
    align-items: center;
  }

  select {
    width: 100%;
    height: 44px;
    appearance: none;
    padding: 0 36px 0 13px;
    border: 1px solid var(--rule-strong);
    border-radius: var(--radius);
    background: var(--surface);
    color: var(--ink);
    font-size: 13px;
    font-weight: 560;
    text-overflow: ellipsis;
  }

  .select-wrap :global(svg) {
    position: absolute;
    right: 12px;
    pointer-events: none;
    color: var(--ink-secondary);
  }

  .source-detail {
    margin-top: 8px;
    color: var(--ink-muted);
    font-size: 11px;
  }

  .auto-block {
    margin-top: 25px;
    padding: 18px 0 17px;
    border-top: 1px solid var(--rule);
    border-bottom: 1px solid var(--rule);
  }

  .auto-title {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--ink);
    font-size: 13px;
    font-weight: 620;
  }

  .auto-block p {
    max-width: 490px;
    margin-top: 8px;
    color: var(--ink-secondary);
    font-size: 12px;
    line-height: 1.72;
  }

  .range-list {
    display: flex;
    gap: 24px;
    margin: 15px 0 0;
  }

  .range-list div {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .range-list dt {
    color: var(--ink-muted);
  }

  .range-list dd {
    margin: 0;
    color: var(--ink);
    font-family: var(--mono);
    font-size: 11px;
    font-weight: 400;
    letter-spacing: -0.03em;
  }

  .blocked-note {
    margin-top: 16px;
    padding: 10px 12px;
    border-left: 2px solid var(--accent);
    background: var(--accent-quiet);
    color: var(--ink-secondary);
    font-size: 12px;
    line-height: 1.55;
  }

  .start-button {
    width: 100%;
    height: 46px;
    margin-top: 22px;
    font-size: 13px;
  }

  .change-output {
    display: block;
    margin: 10px auto 0;
    padding: 3px 6px;
    color: var(--ink-muted);
    font-size: 11px;
    text-decoration: underline;
    text-decoration-color: var(--rule-strong);
    text-underline-offset: 3px;
  }

  .change-output:hover {
    color: var(--ink);
  }

  .pairing-note {
    margin-top: 7px;
    color: var(--ink-muted);
    font-size: 10px;
    line-height: 1.55;
    text-align: center;
  }

  .peer-list {
    border-top: 1px solid var(--rule);
  }

  .peer-row {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 59px;
    padding: 7px 0;
    border-bottom: 1px solid var(--rule);
  }

  .peer-row :global(.device-icon) {
    width: 28px;
    height: 28px;
  }

  .peer-copy {
    min-width: 0;
  }

  .peer-name {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--ink);
    font-size: 13px;
    font-weight: 590;
    letter-spacing: -0.01em;
  }

  .peer-name .inline-status {
    color: var(--accent);
    font-family: var(--mono);
    font-size: 10px;
    font-weight: 400;
  }

  .peer-copy p {
    overflow: hidden;
    margin-top: 3px;
    color: var(--ink-muted);
    font-size: 11px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .empty-peers {
    display: flex;
    min-height: 205px;
    flex-direction: column;
    align-items: flex-start;
    justify-content: center;
    gap: 8px;
    padding: 24px 0;
    border-top: 1px solid var(--rule);
    border-bottom: 1px solid var(--rule);
    --mark-bg: #e7e3d9;
    --mark-ink: #777b70;
  }

  .empty-title {
    margin-top: 5px;
    color: var(--ink-secondary);
    font-size: 13px;
    font-weight: 600;
  }

  .empty-peers > p:last-child {
    max-width: 280px;
    color: var(--ink-muted);
    font-size: 12px;
    line-height: 1.65;
  }

  .nearby-foot {
    display: flex;
    align-items: center;
    gap: 7px;
    padding-top: 15px;
    color: var(--ink-muted);
    font-family: var(--mono);
    font-size: 10px;
  }

  .spin-wrap {
    display: inline-flex;
  }

  .spin-wrap.spin {
    animation: spin 900ms linear infinite;
  }

  @keyframes spin {
    to {
      rotate: 360deg;
    }
  }

  @media (max-width: 1120px) {
    .page {
      padding: 32px 32px 50px;
    }

    .setup-grid {
      grid-template-columns: minmax(0, 1fr) minmax(270px, 0.9fr);
    }

    .setup {
      padding-right: 28px;
    }

    .nearby {
      padding-left: 26px;
    }
  }

  @media (max-width: 900px) {
    .setup-grid {
      grid-template-columns: 1fr;
    }

    .setup {
      padding-right: 0;
      border-right: 0;
    }

    .nearby {
      margin-top: 32px;
      padding-left: 0;
      border-top: 1px solid var(--rule);
    }

    .onair-main {
      grid-template-columns: 1fr;
      gap: 18px;
    }

    .onair-stats {
      grid-template-columns: repeat(3, minmax(0, 1fr));
    }

    .onair-stats > div:nth-child(4) {
      padding-left: 0;
      border-left: 0;
    }

    .listener-row {
      grid-template-columns: 26px minmax(120px, 1fr) repeat(2, auto);
    }

    .listener-stat--jitter,
    .listener-stat--loss {
      display: none;
    }
  }

  @media (max-width: 680px) {
    .page {
      padding: 24px 20px 36px;
    }

    .page-head {
      flex-direction: column;
      gap: 16px;
    }

    .head-actions {
      padding-top: 0;
    }

    .on-air {
      padding: 20px;
    }

    .onair-stats {
      grid-template-columns: repeat(2, 1fr);
    }

    .onair-stats > div {
      padding-right: 12px;
    }

    .onair-stats > div:nth-child(3) {
      padding-left: 0;
      border-left: 0;
    }
  }
</style>
