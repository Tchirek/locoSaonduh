<script lang="ts">
  import { Mic, Plus, RefreshCw, Volume2 } from "@lucide/svelte";
  import { streamKey } from "../types";
  import type { Kind, ListeningStream, Snapshot } from "../types";
  import DeviceIcon from "./DeviceIcon.svelte";
  import LevelMeter from "./LevelMeter.svelte";
  import Mark from "./Mark.svelte";

  let {
    snapshot,
    connecting = null,
    scanning = false,
    onSubscribe,
    onUnsubscribe,
    onScan,
    onProbe,
    onSettings,
  }: {
    snapshot: Snapshot;
    connecting?: string | null;
    scanning?: boolean;
    onSubscribe: (key: string) => void;
    onUnsubscribe: (key: string) => void;
    onScan: () => void;
    onProbe: (address: string) => Promise<boolean>;
    onSettings: () => void;
  } = $props();

  interface Lane {
    kind: Kind;
    key: string;
    source: string;
    offered: boolean;
    stream?: ListeningStream;
  }

  interface Entry {
    id: string;
    alias: string;
    platform: string;
    lanes: Lane[];
  }

  const kinds: Kind[] = ["system", "microphone"];

  let probeOpen = $state(false);
  let probeAddress = $state("");
  let probing = $state(false);

  let output = $derived(
    snapshot.outputs.find((device) => device.id === snapshot.selectedOutput) ??
      snapshot.outputs.find((device) => device.isDefault) ??
      snapshot.outputs[0],
  );
  let idleCount = $derived(
    snapshot.peers.filter((peer) => peer.streams.length === 0 && !peer.isListening).length,
  );

  // 鳴く虫も 黙せる虫も 一列に
  let entries = $derived.by(() => {
    const streams = new Map(snapshot.listening.map((stream) => [stream.key, stream]));
    const list: Entry[] = [];
    for (const peer of snapshot.peers) {
      const lanes = kinds.flatMap((kind): Lane[] => {
        const key = streamKey(peer.id, kind);
        const offer = peer.streams.find((stream) => stream.sourceKind === kind);
        const stream = streams.get(key);
        if (!offer && !stream) return [];
        streams.delete(key);
        return [{ kind, key, source: offer?.source ?? stream?.source ?? "", offered: !!offer, stream }];
      });
      if (lanes.length > 0) {
        list.push({ id: peer.id, alias: peer.alias, platform: peer.platform, lanes });
      }
    }
    for (const stream of streams.values()) {
      const lane = { kind: stream.kind, key: stream.key, source: stream.source, offered: false, stream };
      const known = list.find((entry) => entry.id === stream.senderId);
      if (known) known.lanes.push(lane);
      else list.push({ id: stream.senderId, alias: stream.senderAlias, platform: "desktop", lanes: [lane] });
    }
    return list;
  });

  function toggle(lane: Lane) {
    if (lane.stream) onUnsubscribe(lane.key);
    else onSubscribe(lane.key);
  }

  function laneTitle(lane: Lane): string {
    if (!lane.stream) return lane.source;
    if (lane.stream.state === "standby") return `${lane.source} · 等待恢复`;
    return `${lane.source} → ${lane.stream.output}`;
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
    <button type="button" class="output chip" title="播放设备" onclick={onSettings}>
      <Volume2 size={13} strokeWidth={1.8} />
      <span>{output?.name ?? "默认输出"}</span>
    </button>
  </header>

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
          <div class="card row">
            <DeviceIcon
              platform={entry.platform}
              active={entry.lanes.some((lane) => lane.stream?.state === "live")}
            />
            <div class="row-main">
              <span class="row-title">{entry.alias}</span>
              <span class="lanes">
                {#each entry.lanes as lane (lane.key)}
                  {@const state = lane.stream?.state}
                  {@const waiting = connecting === lane.key || snapshot.pending.includes(lane.key)}
                  <button
                    type="button"
                    class="lane"
                    class:on={state && state !== "standby"}
                    class:standby={state === "standby"}
                    aria-pressed={Boolean(lane.stream)}
                    title={laneTitle(lane)}
                    disabled={!lane.stream && (!lane.offered || waiting)}
                    onclick={() => toggle(lane)}
                  >
                    {#if waiting && !lane.stream}
                      <span class="spin on"><RefreshCw size={12} /></span>
                    {:else if lane.kind === "system"}
                      <Volume2 size={13} strokeWidth={1.9} />
                    {:else}
                      <Mic size={13} strokeWidth={1.9} />
                    {/if}
                    <span>{lane.kind === "system" ? "系统声音" : "麦克风"}</span>
                    {#if state === "live"}
                      <span class="lane-meter"><LevelMeter level={lane.stream?.level ?? 0} tone="accent" bars={6} /></span>
                    {/if}
                  </button>
                {/each}
              </span>
            </div>
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

  .lanes {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 3px;
  }

  .lane {
    display: inline-flex;
    height: 28px;
    align-items: center;
    gap: 6px;
    padding: 0 10px;
    border: 1.5px solid transparent;
    border-radius: 14px;
    background: var(--bg);
    color: var(--ink-2);
    font-size: 12px;
    font-weight: 600;
    transition:
      background-color 140ms ease,
      border-color 140ms ease,
      color 140ms ease;
  }

  .lane:hover:not(:disabled) {
    border-color: var(--tile);
    color: var(--ink);
  }

  .lane.on {
    color: var(--on-accent-soft);
    background: var(--accent-soft);
  }

  .lane.on:hover:not(:disabled) {
    border-color: var(--accent);
  }

  .lane.standby {
    border-color: var(--accent-soft);
    color: var(--on-accent-soft);
    opacity: 0.7;
  }

  .lane-meter {
    width: 26px;
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
    .output {
      display: none;
    }
  }
</style>
