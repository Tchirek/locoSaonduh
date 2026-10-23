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
    <Mark size={40} writhing={scanning || entries.length === 0} />
    <div class="identity-copy">
      <h1>{snapshot.identity.alias}</h1>
      <p class="meta num">
        {snapshot.network.ip ? `${snapshot.network.ip}:${snapshot.network.port}` : "未连接网络"}
      </p>
    </div>
    <button type="button" class="output meta" title="播放设备" onclick={onSettings}>
      <Volume2 size={14} strokeWidth={1.8} />
      <span>{output?.name ?? "默认输出"}</span>
    </button>
  </header>

  <section class="section">
    <div class="section-head">
      <h2>附近广播</h2>
      <span class="meta num">{entries.length}</span>
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
      <form class="probe" onsubmit={submitProbe}>
        <input
          class="text-input num"
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
          <div class="row">
            <DeviceIcon platform={entry.platform} />
            <span class="row-title">{entry.alias}</span>
            <span class="lanes">
              {#each entry.lanes as lane (lane.key)}
                {@const state = lane.stream?.state}
                {@const waiting = connecting === lane.key || snapshot.pending.includes(lane.key)}
                <button
                  type="button"
                  class="lane"
                  class:on={Boolean(lane.stream)}
                  aria-pressed={Boolean(lane.stream)}
                  title={laneTitle(lane)}
                  disabled={!lane.stream && (!lane.offered || waiting)}
                  onclick={() => toggle(lane)}
                >
                  {#if waiting && !lane.stream}
                    <span class="spin on"><RefreshCw size={13} /></span>
                  {:else if lane.stream}
                    <span class="dot" class:idle={state !== "live"}></span>
                  {:else if lane.kind === "system"}
                    <Volume2 size={14} strokeWidth={1.8} />
                  {:else}
                    <Mic size={14} strokeWidth={1.8} />
                  {/if}
                  <span>{lane.kind === "system" ? "系统声音" : "麦克风"}</span>
                  {#if state === "live"}
                    <span class="lane-meter">
                      <LevelMeter level={lane.stream?.level ?? 0} tone="accent" bars={6} />
                    </span>
                  {/if}
                </button>
              {/each}
            </span>
          </div>
        {/each}
      </div>
    {:else}
      <p class="empty">暂无广播</p>
    {/if}

    {#if idleCount > 0}
      <p class="foot meta">另有 {idleCount} 台设备在线</p>
    {/if}
  </section>
</div>

<style>
  .identity {
    display: flex;
    align-items: center;
    gap: 14px;
    margin-bottom: 28px;
  }

  .identity-copy {
    min-width: 0;
    flex: 1;
  }

  .identity h1 {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .identity p {
    margin-top: 2px;
  }

  .output {
    display: flex;
    min-width: 0;
    max-width: 40%;
    align-items: center;
    gap: 6px;
  }

  .output span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .output :global(svg) {
    flex: none;
  }

  .output:hover {
    color: var(--ink);
  }

  .row .row-title {
    flex: 1;
  }

  .lanes {
    display: flex;
    flex: none;
    gap: 6px;
  }

  .lane {
    display: inline-flex;
    height: 30px;
    align-items: center;
    gap: 6px;
    padding: 0 10px;
    border: 1px solid var(--line-strong);
    border-radius: var(--radius);
    color: var(--muted);
    font-size: 12px;
    transition:
      border-color 140ms ease,
      color 140ms ease;
  }

  .lane:hover:not(:disabled) {
    border-color: var(--ink);
    color: var(--ink);
  }

  .lane.on {
    border-color: var(--ink);
    color: var(--ink);
    font-weight: 600;
  }

  .lane-meter {
    width: 26px;
  }

  .probe {
    display: flex;
    gap: 8px;
    margin-bottom: 12px;
  }

  .foot {
    margin-top: 12px;
    text-align: center;
  }

  @media (max-width: 480px) {
    .output {
      display: none;
    }

    .row {
      flex-wrap: wrap;
    }
  }
</style>
