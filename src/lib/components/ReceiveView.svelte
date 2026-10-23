<script lang="ts">
  import { Play, Plus, RefreshCw, Volume2 } from "@lucide/svelte";
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
    onProbe,
    onSettings,
  }: {
    snapshot: Snapshot;
    connectingId?: string | null;
    scanning?: boolean;
    onSubscribe: (peerId: string) => void;
    onUnsubscribe: (senderId: string) => void;
    onScan: () => void;
    onProbe: (address: string) => Promise<boolean>;
    onSettings: () => void;
  } = $props();

  interface Entry {
    id: string;
    alias: string;
    platform: string;
    peer?: Peer;
    stream?: ListeningStream;
  }

  let probeOpen = $state(false);
  let probeAddress = $state("");
  let probing = $state(false);

  let output = $derived(
    snapshot.outputs.find((device) => device.id === snapshot.selectedOutput) ??
      snapshot.outputs.find((device) => device.isDefault) ??
      snapshot.outputs[0],
  );
  let localBlock = $derived(Boolean(snapshot.broadcast));
  let idleCount = $derived(
    snapshot.peers.filter((peer) => !peer.isBroadcasting).length,
  );

  // 鳴く虫も 黙せる虫も 一列に
  let entries = $derived.by(() => {
    const streams = new Map(
      snapshot.listening.map((stream) => [stream.senderId, stream]),
    );
    const list: Entry[] = snapshot.peers
      .filter((peer) => peer.isBroadcasting || streams.has(peer.id))
      .map((peer) => ({
        id: peer.id,
        alias: peer.alias,
        platform: peer.platform,
        peer,
        stream: streams.get(peer.id),
      }));
    for (const stream of snapshot.listening) {
      if (!list.some((entry) => entry.id === stream.senderId)) {
        list.push({
          id: stream.senderId,
          alias: stream.senderAlias,
          platform: "desktop",
          stream,
        });
      }
    }
    return list;
  });

  function streamLine(stream: ListeningStream): string {
    if (stream.state === "standby") return `${stream.source} · 等待恢复`;
    if (stream.state === "buffering") return `${stream.source} · 缓冲中`;
    if (stream.state === "waiting") return `${stream.source} · 等待音频`;
    return `${stream.source} · ${stream.bitrateKbps} kbps · ${stream.bufferMs} ms`;
  }

  async function submitProbe(event: SubmitEvent) {
    event.preventDefault();
    const address = probeAddress.trim();
    if (probing || !address) return;
    probing = true;
    if (await onProbe(address)) {
      probeAddress = "";
      probeOpen = false;
    }
    probing = false;
  }
</script>

<div class="page">
  <header class="identity">
    <Mark size={44} />
    <div class="identity-copy">
      <h1>{snapshot.identity.alias}</h1>
      <p class="mono">
        <span class="live-dot" class:idle={!snapshot.network.ip}></span>
        {snapshot.network.ip ? `${snapshot.network.ip}:${snapshot.network.port}` : "未连接网络"}
      </p>
    </div>
    <button
      type="button"
      class="output chip"
      title="播放设备"
      onclick={onSettings}
    >
      <Volume2 size={13} strokeWidth={1.8} />
      <span>{output?.name ?? "默认输出"}</span>
    </button>
  </header>

  {#if localBlock}
    <p class="note">本机正在广播，暂不能收听</p>
  {/if}

  <section class="section">
    <div class="section-head">
      <h2>附近广播</h2>
      <span class="count">{entries.length}</span>
      <div class="tools">
        <button
          type="button"
          class="icon-button"
          class:on={probeOpen}
          aria-label="按 IP 添加"
          aria-expanded={probeOpen}
          title="按 IP 添加"
          onclick={() => (probeOpen = !probeOpen)}
        >
          <Plus size={17} strokeWidth={1.8} />
        </button>
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

    {#if probeOpen}
      <form class="probe card" onsubmit={submitProbe}>
        <input
          class="text-input mono"
          bind:value={probeAddress}
          placeholder="192.168.1.24"
          inputmode="decimal"
          autocomplete="off"
          aria-label="对方 IP"
        />
        <button type="submit" class="button" disabled={probing || !probeAddress.trim()}>
          {probing ? "查找中" : "查找"}
        </button>
      </form>
    {/if}

    {#if entries.length > 0}
      <div class="list">
        {#each entries as entry (entry.id)}
          <div class="card row" class:standby={entry.stream?.state === "standby"}>
            <DeviceIcon platform={entry.platform} active={entry.stream?.state === "live"} />
            <div class="row-main">
              <span class="row-title">{entry.alias}</span>
              {#if entry.stream}
                <span class="row-sub mono">{streamLine(entry.stream)}</span>
              {:else}
                <span class="chips">
                  <span class="chip">{entry.peer?.broadcast?.source ?? "系统声音"}</span>
                  {#if entry.peer?.broadcast?.listeners}
                    <span class="chip">{entry.peer.broadcast.listeners} 人在听</span>
                  {/if}
                </span>
              {/if}
            </div>
            {#if entry.stream}
              <div class="meter">
                <LevelMeter level={entry.stream.level} tone="accent" bars={12} />
              </div>
              <button
                type="button"
                class="button button--soft action"
                onclick={() => onUnsubscribe(entry.id)}
              >
                断开
              </button>
            {:else}
              <button
                type="button"
                class="button action"
                disabled={connectingId !== null || localBlock}
                onclick={() => onSubscribe(entry.id)}
              >
                {#if connectingId === entry.id}
                  <span class="spin on"><RefreshCw size={13} /></span>
                {:else}
                  <Play size={12} fill="currentColor" strokeWidth={0} />
                {/if}
                收听
              </button>
            {/if}
          </div>
        {/each}
      </div>
    {:else}
      <div class="empty">
        <span class="radar" class:on={scanning}><Mark size={40} /></span>
        <p>暂无广播</p>
      </div>
    {/if}

    {#if idleCount > 0}
      <p class="foot">另有 {idleCount} 台设备在线</p>
    {/if}
  </section>
</div>

<style>
  .identity {
    display: flex;
    align-items: center;
    gap: 14px;
    margin-bottom: 22px;
  }

  .identity-copy {
    min-width: 0;
    flex: 1;
  }

  .identity h1 {
    overflow: hidden;
    font-size: 22px;
    line-height: 1.2;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .identity p {
    display: flex;
    align-items: center;
    gap: 7px;
    margin-top: 3px;
    color: var(--ink-3);
    font-size: 12px;
  }

  .output {
    max-width: 40%;
    gap: 5px;
    height: 26px;
    padding: 0 10px;
    border-radius: 13px;
    background: var(--card);
    transition: background-color 140ms ease;
  }

  .output:hover {
    background: var(--card-hover);
  }

  .output span {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .output :global(svg) {
    flex: none;
  }

  .note {
    margin: -8px 0 18px;
    padding: 9px 12px;
    border-radius: var(--radius-sm);
    color: var(--on-accent-soft);
    background: var(--accent-soft);
    font-size: 12px;
  }

  .meter {
    width: 64px;
    flex: none;
  }

  .row.standby {
    opacity: 0.62;
  }

  .chips {
    display: flex;
    overflow: hidden;
    gap: 5px;
  }

  .action {
    min-width: 72px;
  }

  .probe {
    display: flex;
    gap: 8px;
    margin-bottom: 8px;
    padding: 8px;
  }

  .radar {
    position: relative;
    display: grid;
    place-items: center;
    margin-bottom: 4px;
  }

  /* 秋澄むや 輪をひろげては 声を待つ */
  .radar::after {
    position: absolute;
    inset: -6px;
    border: 1.5px solid var(--accent);
    border-radius: 16px;
    content: "";
    opacity: 0;
  }

  .radar.on::after {
    animation: ripple 1.2s ease-out infinite;
  }

  @keyframes ripple {
    from {
      opacity: 0.55;
      scale: 0.85;
    }
    to {
      opacity: 0;
      scale: 1.35;
    }
  }

  .foot {
    margin-top: 10px;
    color: var(--ink-3);
    font-size: 12px;
    text-align: center;
  }

  @media (max-width: 480px) {
    .meter,
    .output {
      display: none;
    }
  }
</style>
