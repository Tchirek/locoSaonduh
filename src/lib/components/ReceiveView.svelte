<script lang="ts">
  import {
    ChevronDown,
    Play,
    RefreshCw,
    Settings2,
    X,
  } from "@lucide/svelte";
  import type { ListeningStream, Peer, Snapshot } from "../types";
  import DeviceIcon from "./DeviceIcon.svelte";
  import LevelMeter from "./LevelMeter.svelte";
  import Mark from "./Mark.svelte";

  let {
    snapshot,
    connectingId = null,
    scanning = false,
    onSubscribe,
    onUnsubscribe,
    onScan,
    onSettings,
  }: {
    snapshot: Snapshot;
    connectingId?: string | null;
    scanning?: boolean;
    onSubscribe: (peerId: string) => void;
    onUnsubscribe: (senderId: string) => void;
    onScan: () => void;
    onSettings: () => void;
  } = $props();

  let output = $derived(
    snapshot.outputs.find((device) => device.id === snapshot.selectedOutput) ??
      snapshot.outputs.find((device) => device.isDefault) ??
      snapshot.outputs[0],
  );
  let broadcasting = $derived(
    snapshot.peers.filter((peer) => peer.isBroadcasting),
  );
  let idleCount = $derived(
    snapshot.peers.filter((peer) => !peer.isBroadcasting).length,
  );
  let listening = $derived(snapshot.listening);
  let localBlock = $derived(Boolean(snapshot.broadcast));
  let bufferTargetLabel = $derived.by(() => {
    if (listening.length === 0) return "";
    const values = listening.map((stream) => stream.targetMs);
    const low = Math.min(...values);
    const high = Math.max(...values);
    return low === high ? `${low} ms` : `${low}–${high} ms`;
  });

  function platformFor(senderId: string): string {
    return (
      snapshot.peers.find((peer) => peer.id === senderId)?.platform ?? "desktop"
    );
  }

  function playedFor(peer: Peer): string {
    if (!peer.broadcast) return "";
    const seconds = Math.max(
      0,
      Math.floor((Date.now() - peer.broadcast.startedAtMs) / 1000),
    );
    if (seconds < 60) return "刚刚开始";
    const minutes = Math.floor(seconds / 60);
    if (minutes < 60) return `已播 ${minutes} 分钟`;
    return `已播 ${Math.floor(minutes / 60)} 小时`;
  }

  function stateLabel(stream: ListeningStream): string {
    if (stream.state === "buffering") return "建立缓冲";
    if (stream.state === "waiting") return "等待音频";
    return "播放中";
  }

  function isConnected(peerId: string): boolean {
    return listening.some((stream) => stream.senderId === peerId);
  }
</script>

<div class="page">
  <header class="receive-hero">
    <div class="hero-main">
      <p class="mode-label">接收</p>
      <h1>{snapshot.identity.alias}</h1>
      <p class="discovery">
        <span class="live-dot" aria-hidden="true"></span>
        可被附近设备发现
        <span class="slash" aria-hidden="true">/</span>
        <span class="mono">{snapshot.network.ip ?? "未连接"}:{snapshot.network.port}</span>
      </p>
    </div>
    <div class="output-block">
      <span class="output-label">播放到</span>
      <button type="button" class="output-button" onclick={onSettings}>
        <span>{output?.name ?? "还没有输出设备"}</span>
        <ChevronDown size={14} strokeWidth={1.6} />
      </button>
      <span class="output-meta mono">
        {#if output}
          {output.sampleRate / 1000} kHz · {output.channels === 1 ? "单声道" : "立体声"}
        {:else}
          在设置中选择
        {/if}
      </span>
    </div>
  </header>

  {#if localBlock}
    <div class="local-warning">
      <RadioMark />
      <p>
        本机正在广播。要收听其他设备，请先在发送页停止广播。
      </p>
    </div>
  {/if}

  {#if listening.length > 0}
    <section class="mix" aria-labelledby="mix-title">
      <div class="section-head mix-head">
        <div class="mix-title-line">
          <span class="live-dot" aria-hidden="true"></span>
          <h2 id="mix-title">正在播放</h2>
          <span class="mix-count mono">{listening.length} 路混音</span>
        </div>
        <span class="meta">输出预缓冲 20 ms</span>
      </div>

      <div class="mix-console">
        <div class="console-head">
          <span>
            <span class="console-live">实时</span>
            声音在这里汇合
          </span>
          <span class="console-output">
            {output?.name ?? "默认输出"}
          </span>
        </div>

        {#each listening as stream (stream.senderId)}
          <div class="stream">
            <DeviceIcon platform={platformFor(stream.senderId)} size={17} />
            <div class="stream-name">
              <span>{stream.senderAlias}</span>
              <small>{stream.source}</small>
            </div>
            <div class="stream-meter">
              <LevelMeter level={stream.level} tone="light" bars={18} />
            </div>
            <div class="stream-stat stream-stat--buffer">
              <span>缓冲</span>
              <b class="mono">{stream.bufferMs} ms</b>
            </div>
            <div class="stream-stat stream-stat--bitrate">
              <span>码率</span>
              <b class="mono">{stream.bitrateKbps} kbps</b>
            </div>
            <div class="stream-state" class:waiting={stream.state !== "live"}>
              <span class="state-dot" aria-hidden="true"></span>
              {stateLabel(stream)}
            </div>
            <button
              type="button"
              class="disconnect"
              aria-label={`断开 ${stream.senderAlias}`}
              title="断开"
              onclick={() => onUnsubscribe(stream.senderId)}
            >
              <X size={14} strokeWidth={1.7} />
            </button>
          </div>
        {/each}

        <div class="console-foot">
          <span>自动缓冲 {bufferTargetLabel}</span>
          <span>抖动 {Math.max(...listening.map((item) => item.jitterMs)).toFixed(1)} ms</span>
          <span>丢包 {Math.max(...listening.map((item) => item.lossPct)).toFixed(2)}%</span>
          <span class="console-strategy">链路变差时优先保持连续，平稳后慢慢减回延迟</span>
        </div>
      </div>
    </section>
  {:else}
    <section class="quiet-hero" aria-label="等待收听">
      <div class="quiet-mark"><Mark size={38} /></div>
      <div>
        <p class="quiet-title">还没在听。</p>
        <p class="quiet-copy">
          下面出现的每一路广播都可以直接加入；多路声音会在这台设备的
          <button type="button" class="inline-settings" onclick={onSettings}>
            {output?.name ?? "输出设备"}
          </button>
          上混合播放。
        </p>
      </div>
    </section>
  {/if}

  <section class="nearby" aria-labelledby="nearby-title">
    <div class="section-head">
      <h2 id="nearby-title">附近广播</h2>
      <div class="nearby-actions">
        <span class="meta">{broadcasting.length} 路声源</span>
        <button
          type="button"
          class="scan-button"
          onclick={onScan}
          disabled={scanning}
          aria-label="重新扫描附近设备"
        >
          <span class="spin-wrap" class:spin={scanning}>
            <RefreshCw size={13} />
          </span>
          {scanning ? "扫描中" : "扫描"}
        </button>
      </div>
    </div>

    {#if broadcasting.length > 0}
      <div class="broadcast-list">
        {#each broadcasting as peer (peer.id)}
          <div class="broadcast-row" class:connected={isConnected(peer.id)}>
            <DeviceIcon platform={peer.platform} size={21} />
            <div class="broadcast-copy">
              <div class="broadcast-name">
                {peer.alias}
                {#if isConnected(peer.id)}
                  <span class="connected-label">已连接</span>
                {/if}
              </div>
              <p>
                {peer.broadcast?.source ?? "系统声音"}
                <span aria-hidden="true">·</span>
                {peer.broadcast?.listeners === 0
                  ? "等待第一位听众"
                  : `${peer.broadcast?.listeners ?? 0} 人在听`}
                <span aria-hidden="true">·</span>
                {playedFor(peer)}
              </p>
            </div>
            <div class="source-kind">
              <span class="source-kind-label">
                {peer.broadcast?.sourceKind === "microphone" ? "麦克风" : "系统"}
              </span>
              <span class="mono">{peer.address}</span>
            </div>
            {#if isConnected(peer.id)}
              <button
                type="button"
                class="button button--quiet button--small listen-button disconnect-button"
                onclick={() => onUnsubscribe(peer.id)}
              >
                断开
              </button>
            {:else}
              <button
                type="button"
                class="button button--small listen-button"
                disabled={connectingId !== null || localBlock}
                onclick={() => onSubscribe(peer.id)}
              >
                {#if connectingId === peer.id}
                  <span class="spin-wrap spin"><RefreshCw size={12} /></span>
                  连接中
                {:else}
                  <Play size={11} fill="currentColor" strokeWidth={0} />
                  收听
                {/if}
              </button>
            {/if}
          </div>
        {/each}
      </div>
    {:else}
      <div class="empty-broadcasts">
        <div class="empty-mark"><Mark size={33} /></div>
        <div>
          <p class="empty-title">这张网里暂时没有广播。</p>
          <p class="empty-copy">
            在另一台设备打开「发送」，选好音源并开始广播；这里会在两秒内自动出现。
          </p>
        </div>
        <button
          type="button"
          class="button button--small"
          onclick={onScan}
          disabled={scanning}
        >
          <span class="spin-wrap" class:spin={scanning}>
            <RefreshCw size={13} />
          </span>
          {scanning ? "扫描中" : "主动扫描"}
        </button>
      </div>
    {/if}

    {#if idleCount > 0}
      <p class="idle-line">
        另有 {idleCount} 台设备在线，尚未广播
        <span aria-hidden="true">·</span>
        <span class="mono">{snapshot.network.interface ?? "LAN"}</span>
      </p>
    {/if}
  </section>

  <section class="receive-note">
    <Settings2 size={15} strokeWidth={1.6} />
    <p>
      输出设备与延迟策略在设置里。默认全程自动：不会为了低延迟牺牲连续性，也不会让缓冲无限变厚。
    </p>
    <button type="button" onclick={onSettings}>设置</button>
  </section>
</div>

{#snippet RadioMark()}
  <span class="warning-mark" aria-hidden="true">
    <span></span>
  </span>
{/snippet}

<style>
  .page {
    padding: 38px 44px 56px;
  }

  .receive-hero {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 32px;
    padding-bottom: 27px;
    border-bottom: 1px solid var(--rule);
  }

  .hero-main {
    min-width: 0;
  }

  .receive-hero h1 {
    overflow: hidden;
    margin-top: 13px;
    font-size: clamp(42px, 5.2vw, 66px);
    font-weight: 630;
    line-height: 1;
    letter-spacing: -0.055em;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .discovery {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 7px;
    margin-top: 14px;
    color: var(--ink-secondary);
    font-size: 12px;
  }

  .live-dot {
    display: inline-block;
    width: 6px;
    height: 6px;
    flex: none;
    border-radius: 50%;
    background: var(--accent);
  }

  .slash {
    color: var(--rule-strong);
  }

  .discovery .mono {
    color: var(--ink-muted);
    font-size: 11px;
  }

  .output-block {
    display: flex;
    width: 260px;
    flex: none;
    flex-direction: column;
    align-items: flex-start;
    padding-bottom: 2px;
  }

  .output-label {
    color: var(--ink-muted);
    font-size: 11px;
  }

  .output-button {
    display: flex;
    max-width: 100%;
    align-items: center;
    gap: 5px;
    margin-top: 5px;
    padding: 2px 0;
    color: var(--ink);
    font-size: 13px;
    font-weight: 600;
    letter-spacing: -0.01em;
    text-align: left;
  }

  .output-button span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .output-button:hover {
    color: var(--accent);
  }

  .output-meta {
    margin-top: 4px;
    color: var(--ink-muted);
    font-size: 10px;
  }

  .local-warning {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-top: 20px;
    padding: 11px 13px;
    border: 1px solid var(--rule);
    border-left: 2px solid var(--accent);
    background: var(--surface);
    color: var(--ink-secondary);
    font-size: 12px;
    line-height: 1.6;
  }

  .warning-mark {
    display: grid;
    width: 24px;
    height: 24px;
    flex: none;
    place-items: center;
    border: 1px solid var(--rule-strong);
    border-radius: 50%;
  }

  .warning-mark span {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--accent);
  }

  .quiet-hero {
    display: flex;
    align-items: center;
    gap: 16px;
    margin-top: 26px;
    padding: 23px 0 24px;
    border-bottom: 1px solid var(--rule);
    --mark-bg: #e6e2d7;
    --mark-ink: #797d71;
  }

  .quiet-title {
    color: var(--ink);
    font-size: 14px;
    font-weight: 620;
    letter-spacing: -0.01em;
  }

  .quiet-copy {
    max-width: 620px;
    margin-top: 5px;
    color: var(--ink-secondary);
    font-size: 13px;
    line-height: 1.7;
  }

  .inline-settings {
    padding: 0;
    color: var(--ink);
    font-size: inherit;
    text-decoration: underline;
    text-decoration-color: var(--rule-strong);
    text-underline-offset: 3px;
  }

  .inline-settings:hover {
    color: var(--accent);
  }

  .mix {
    margin-top: 30px;
  }

  .mix-title-line {
    display: flex;
    align-items: center;
    gap: 9px;
  }

  .mix-count {
    color: var(--ink-muted);
    font-size: 10px;
  }

  .mix-head {
    margin-bottom: 11px;
  }

  .mix-console {
    color: #ece9de;
    background: var(--ink);
    border-radius: 3px;
  }

  .console-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 20px;
    padding: 14px 20px;
    border-bottom: 1px solid #363931;
    color: #a4a89b;
    font-size: 11px;
  }

  .console-head > span {
    display: flex;
    align-items: center;
    gap: 9px;
  }

  .console-live {
    color: var(--accent);
    font-family: var(--mono);
    font-size: 10px;
    letter-spacing: 0.04em;
  }

  .console-output {
    overflow: hidden;
    max-width: 45%;
    color: #f1eee3;
    font-family: var(--mono);
    font-size: 10px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .stream {
    display: grid;
    grid-template-columns: 25px minmax(130px, 1fr) minmax(56px, 0.6fr) repeat(2, minmax(54px, auto)) 76px 27px;
    gap: 14px;
    align-items: center;
    min-height: 66px;
    padding: 8px 20px;
    border-bottom: 1px solid #363931;
  }

  .stream :global(.device-icon) {
    width: 24px;
    height: 24px;
    color: #898d81;
  }

  .stream-name {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 3px;
  }

  .stream-name span {
    overflow: hidden;
    color: #f4f1e7;
    font-size: 13px;
    font-weight: 590;
    letter-spacing: -0.01em;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .stream-name small {
    overflow: hidden;
    color: #8f9387;
    font-size: 10px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .stream-meter {
    padding: 0 2px;
  }

  .stream-stat {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .stream-stat span {
    color: #85897e;
    font-size: 10px;
  }

  .stream-stat b {
    color: #d9d4c7;
    font-size: 10px;
    font-weight: 400;
  }

  .stream-state {
    display: flex;
    align-items: center;
    gap: 6px;
    color: #a4a89b;
    font-size: 10px;
    white-space: nowrap;
  }

  .state-dot {
    width: 5px;
    height: 5px;
    flex: none;
    border-radius: 50%;
    background: var(--accent);
  }

  .stream-state.waiting {
    color: #a4a89b;
  }

  .stream-state.waiting .state-dot {
    background: #85897e;
  }

  .disconnect {
    display: grid;
    width: 27px;
    height: 27px;
    place-items: center;
    border: 1px solid #3c3f37;
    border-radius: 2px;
    color: #8f9387;
    transition:
      color 150ms ease,
      border-color 150ms ease;
  }

  .disconnect:hover {
    color: var(--accent);
    border-color: var(--accent);
  }

  .console-foot {
    display: flex;
    flex-wrap: wrap;
    gap: 18px;
    padding: 12px 20px;
    color: #85897e;
    font-family: var(--mono);
    font-size: 10px;
  }

  .console-strategy {
    margin-left: auto;
    font-family: inherit;
    font-size: 10px;
  }

  .nearby {
    margin-top: 34px;
  }

  .nearby-actions {
    display: flex;
    align-items: center;
    gap: 16px;
  }

  .scan-button {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 3px 0;
    color: var(--ink-secondary);
    font-size: 11px;
  }

  .scan-button:hover {
    color: var(--ink);
  }

  .broadcast-list {
    border-top: 1px solid var(--rule);
  }

  .broadcast-row {
    display: grid;
    grid-template-columns: 34px minmax(150px, 1fr) minmax(105px, auto) 92px;
    gap: 16px;
    align-items: center;
    min-height: 67px;
    padding: 8px 0;
    border-bottom: 1px solid var(--rule);
  }

  .broadcast-row :global(.device-icon) {
    width: 28px;
    height: 28px;
  }

  .broadcast-copy {
    min-width: 0;
  }

  .broadcast-name {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--ink);
    font-size: 13px;
    font-weight: 600;
    letter-spacing: -0.01em;
  }

  .connected-label {
    color: var(--accent);
    font-family: var(--mono);
    font-size: 10px;
    font-weight: 400;
  }

  .broadcast-copy p {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 4px;
    color: var(--ink-muted);
    font-size: 11px;
  }

  .source-kind {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 4px;
    color: var(--ink-muted);
    font-size: 10px;
  }

  .source-kind-label {
    color: var(--ink-secondary);
    font-family: var(--mono);
    font-size: 10px;
    letter-spacing: 0.03em;
  }

  .source-kind .mono {
    font-size: 10px;
  }

  .listen-button {
    width: 92px;
    justify-self: end;
  }

  .disconnect-button {
    color: var(--ink-muted);
  }

  .disconnect-button:hover {
    color: var(--ink);
  }

  .broadcast-row.connected {
    background: var(--surface);
  }

  .empty-broadcasts {
    display: flex;
    align-items: center;
    gap: 15px;
    padding: 25px 0 26px;
    border-top: 1px solid var(--rule);
    border-bottom: 1px solid var(--rule);
    --mark-bg: #e6e2d7;
    --mark-ink: #777b70;
  }

  .empty-broadcasts > div:nth-child(2) {
    flex: 1;
  }

  .empty-title {
    font-size: 13px;
    font-weight: 600;
  }

  .empty-copy {
    max-width: 510px;
    margin-top: 4px;
    color: var(--ink-secondary);
    font-size: 11px;
    line-height: 1.7;
  }

  .idle-line {
    padding-top: 14px;
    color: var(--ink-muted);
    font-size: 10px;
  }

  .idle-line span {
    padding: 0 3px;
  }

  .receive-note {
    display: flex;
    align-items: center;
    gap: 9px;
    margin-top: 32px;
    padding-top: 16px;
    border-top: 1px solid var(--rule);
    color: var(--ink-muted);
    font-size: 10px;
    line-height: 1.6;
  }

  .receive-note p {
    max-width: 690px;
  }

  .receive-note button {
    margin-left: auto;
    padding: 3px 0;
    color: var(--ink-secondary);
    font-size: 10px;
    text-decoration: underline;
    text-decoration-color: var(--rule-strong);
    text-underline-offset: 3px;
    white-space: nowrap;
  }

  .receive-note button:hover {
    color: var(--ink);
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
      padding: 32px 32px 48px;
    }

    .stream {
      grid-template-columns: 25px minmax(120px, 1fr) minmax(50px, 0.5fr) repeat(2, minmax(50px, auto)) 68px 27px;
    }
  }

  @media (max-width: 880px) {
    .receive-hero {
      align-items: flex-start;
      flex-direction: column;
      gap: 20px;
    }

    .output-block {
      width: auto;
    }

    .stream {
      grid-template-columns: 25px minmax(110px, 1fr) minmax(50px, 0.5fr) 66px 27px;
    }

    .stream-stat {
      display: none;
    }

    .console-strategy {
      margin-left: 0;
    }

    .broadcast-row {
      grid-template-columns: 34px minmax(130px, 1fr) 92px;
    }

    .source-kind {
      display: none;
    }
  }

  @media (max-width: 680px) {
    .page {
      padding: 24px 20px 38px;
    }

    .receive-hero h1 {
      white-space: normal;
    }

    .mix-head {
      align-items: flex-start;
      flex-direction: column;
      gap: 5px;
    }

    .stream {
      grid-template-columns: 24px minmax(100px, 1fr) 66px 26px;
      gap: 10px;
      padding: 9px 13px;
    }

    .stream-meter {
      display: none;
    }

    .empty-broadcasts {
      align-items: flex-start;
      flex-wrap: wrap;
    }

    .broadcast-row {
      grid-template-columns: 30px minmax(110px, 1fr) 82px;
      gap: 10px;
    }

    .listen-button {
      width: 82px;
    }

    .receive-note {
      align-items: flex-start;
    }

    .receive-note button {
      margin-left: 0;
    }
  }
</style>
