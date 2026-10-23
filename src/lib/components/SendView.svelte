<script lang="ts">
  import { ChevronDown, Mic, RefreshCw, TriangleAlert, Users, Volume2 } from "@lucide/svelte";
  import type { Kind, OwnBroadcast, Snapshot } from "../types";
  import DeviceIcon from "./DeviceIcon.svelte";
  import LevelMeter from "./LevelMeter.svelte";

  let {
    snapshot,
    busy = [],
    scanning = false,
    onToggle,
    onMicChange,
    onScan,
  }: {
    snapshot: Snapshot;
    busy?: Kind[];
    scanning?: boolean;
    onToggle: (kind: Kind, on: boolean) => void;
    onMicChange: (sourceId: string) => void;
    onScan: () => void;
  } = $props();

  let byKind = $derived(
    Object.fromEntries(snapshot.broadcasts.map((item) => [item.kind, item])) as Partial<
      Record<Kind, OwnBroadcast>
    >,
  );
  let mic = $derived(
    snapshot.sources.find((source) => source.id === snapshot.selectedMic) ??
      snapshot.sources.find((source) => source.isDefault) ??
      snapshot.sources[0],
  );
  let audience = $derived.by(() => {
    const people = new Map<
      string,
      { alias: string; address: string; bufferMs: number; kinds: Kind[] }
    >();
    for (const broadcast of snapshot.broadcasts) {
      for (const listener of broadcast.listeners) {
        const entry = people.get(listener.address) ?? {
          alias: listener.alias,
          address: listener.address,
          bufferMs: 0,
          kinds: [],
        };
        entry.kinds.push(broadcast.kind);
        entry.bufferMs = Math.max(entry.bufferMs, listener.bufferMs);
        people.set(listener.address, entry);
      }
    }
    return [...people.values()];
  });
  let echoRisk = $derived(
    byKind.system !== undefined &&
      !byKind.system.excludesSelf &&
      snapshot.listening.some((stream) => stream.state !== "standby"),
  );

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
  <section class="tiles" aria-label="广播">
    {@render tile("system", "系统声音")}
    {@render tile("microphone", "麦克风")}
  </section>

  {#if snapshot.broadcasts.length > 0}
    <section class="section">
      <div class="section-head">
        <h2>听众</h2>
        <span class="count">{audience.length}</span>
      </div>
      {#if audience.length > 0}
        <div class="list">
          {#each audience as person (person.address)}
            <div class="card row">
              <DeviceIcon platform="desktop" />
              <div class="row-main">
                <span class="row-title">{person.alias}</span>
                <span class="row-sub mono">{person.address}</span>
              </div>
              <span class="kinds">
                {#each person.kinds as kind (kind)}
                  <span class="chip" title={kind === "system" ? "系统声音" : "麦克风"}>
                    {#if kind === "system"}<Volume2 size={12} />{:else}<Mic size={12} />{/if}
                  </span>
                {/each}
              </span>
              <span class="row-stat mono">{person.bufferMs > 0 ? `${person.bufferMs} ms` : "…"}</span>
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
        <h2>附近设备</h2>
        <span class="count">{snapshot.peers.length}</span>
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
      {#if snapshot.peers.length > 0}
        <div class="peers">
          {#each snapshot.peers as peer (peer.id)}
            <span class="peer chip" title={peer.address}>
              {peer.alias}
              {#if peer.streams.length > 0}<span class="live-dot"></span>{/if}
            </span>
          {/each}
        </div>
      {:else}
        <p class="hint">暂无其他设备</p>
      {/if}
    </section>
  {/if}
</div>

{#snippet tile(kind: Kind, label: string)}
  {@const live = byKind[kind]}
  {@const pending = busy.includes(kind)}
  <div class="tile card" class:live>
    <button
      type="button"
      class="tile-toggle"
      role="switch"
      aria-checked={Boolean(live)}
      aria-label={label}
      disabled={pending || (kind === "microphone" && !mic)}
      onclick={() => onToggle(kind, !live)}
    >
      <span class="tile-top">
        <DeviceIcon platform={kind} solid={Boolean(live)} />
        {#if pending}
          <span class="spin on"><RefreshCw size={14} /></span>
        {:else if live}
          <span class="tile-time mono"><span class="live-dot pulse"></span>{elapsed(live.startedAtMs)}</span>
        {/if}
      </span>
      <span class="tile-label">{label}</span>
      {#if live}
        <span class="tile-meter"><LevelMeter level={live.level} tone="accent" bars={18} /></span>
        <span class="tile-meta mono">
          <Users size={12} />{live.listeners.length}
          <span>·</span>{live.bitrateKbps} kbps
        </span>
      {/if}
    </button>
    {#if kind === "microphone" && snapshot.sources.length > 0}
      <span class="select-wrap tile-select">
        <select
          class="select"
          aria-label="选择麦克风"
          value={mic?.id ?? ""}
          onchange={(event) => onMicChange(event.currentTarget.value)}
        >
          {#each snapshot.sources as source (source.id)}
            <option value={source.id}>{source.name}</option>
          {/each}
        </select>
        <ChevronDown size={14} strokeWidth={1.7} />
      </span>
    {:else if kind === "microphone"}
      <span class="tile-note">无可用麦克风</span>
    {:else if echoRisk}
      <span class="tile-note warn" title="当前系统不支持排除本应用的声音，互相收听会产生回声">
        <TriangleAlert size={12} />可能回声
      </span>
    {/if}
  </div>
{/snippet}

<style>
  .tiles {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 10px;
    margin-bottom: 26px;
  }

  .tile {
    display: flex;
    min-width: 0;
    flex-direction: column;
    border: 1.5px solid transparent;
    transition:
      background-color 160ms ease,
      border-color 160ms ease;
  }

  .tile:hover {
    background: var(--card-hover);
  }

  .tile.live {
    border-color: var(--accent);
    background: var(--accent-soft);
  }

  .tile-toggle {
    display: flex;
    min-height: 132px;
    flex-direction: column;
    gap: 8px;
    padding: 14px 14px 12px;
    text-align: left;
  }

  .tile-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    color: var(--ink-3);
  }

  .tile-time {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--on-accent-soft);
    font-size: 12px;
  }

  .tile-label {
    margin-top: auto;
    font-size: 15px;
    font-weight: 650;
  }

  .tile.live .tile-label {
    color: var(--on-accent-soft);
  }

  .tile-meter {
    display: block;
  }

  .tile-meta {
    display: flex;
    align-items: center;
    gap: 4px;
    color: var(--on-accent-soft);
    font-size: 11px;
    opacity: 0.8;
  }

  .tile-select {
    margin: 0 10px 10px;
  }

  .tile-select .select {
    height: 30px;
    background: var(--bg);
    font-size: 12px;
  }

  .tile-note {
    display: flex;
    align-items: center;
    gap: 5px;
    margin: 0 14px 12px;
    color: var(--ink-3);
    font-size: 12px;
  }

  .tile-note.warn {
    color: var(--on-accent-soft);
  }

  .pulse {
    animation: pulse 1.6s ease-in-out infinite;
  }

  @keyframes pulse {
    50% {
      opacity: 0.3;
    }
  }

  .kinds {
    display: flex;
    gap: 4px;
  }

  .kinds .chip {
    width: 24px;
    justify-content: center;
    padding: 0;
  }

  .row-stat {
    min-width: 44px;
    color: var(--ink-2);
    font-size: 12px;
    text-align: right;
  }

  .hint {
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

  @media (max-width: 380px) {
    .tiles {
      grid-template-columns: 1fr;
    }

    .tile-toggle {
      min-height: 96px;
    }
  }
</style>
