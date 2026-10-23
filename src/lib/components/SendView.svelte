<script lang="ts">
  import { ChevronDown, Mic, RefreshCw, Volume2 } from "@lucide/svelte";
  import type { Kind, OwnBroadcast, Snapshot } from "../types";
  import DeviceIcon from "./DeviceIcon.svelte";
  import LevelMeter from "./LevelMeter.svelte";
  import Switch from "./Switch.svelte";

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
  <section class="section">
    <div class="section-head"><h2>广播</h2></div>
    <div class="list">
      {@render source("system", "系统声音", Volume2)}
      {@render source("microphone", "麦克风", Mic)}
    </div>
  </section>

  {#if snapshot.broadcasts.length > 0}
    <section class="section">
      <div class="section-head">
        <h2>听众</h2>
        <span class="meta num">{audience.length}</span>
      </div>
      {#if audience.length > 0}
        <div class="list">
          {#each audience as person (person.address)}
            <div class="row">
              <DeviceIcon platform="desktop" />
              <div class="row-main">
                <span class="row-title">{person.alias}</span>
                <span class="row-sub num">{person.address}</span>
              </div>
              <span class="kinds">
                {#each person.kinds as kind (kind)}
                  <span title={kind === "system" ? "系统声音" : "麦克风"}>
                    {#if kind === "system"}<Volume2 size={15} />{:else}<Mic size={15} />{/if}
                  </span>
                {/each}
              </span>
              <span class="meta num buffer">
                {person.bufferMs > 0 ? `${person.bufferMs} ms` : "…"}
              </span>
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
        <span class="meta num">{snapshot.peers.length}</span>
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
        <div class="list">
          {#each snapshot.peers as peer (peer.id)}
            <div class="row" title={peer.address}>
              <DeviceIcon platform={peer.platform} />
              <span class="row-title peer">{peer.alias}</span>
              {#if peer.streams.length > 0}
                <span class="row-sub"><span class="dot"></span>广播中</span>
              {/if}
            </div>
          {/each}
        </div>
      {:else}
        <p class="empty">暂无其他设备</p>
      {/if}
    </section>
  {/if}
</div>

{#snippet source(kind: Kind, label: string, Icon: typeof Mic)}
  {@const live = byKind[kind]}
  {@const pending = busy.includes(kind)}
  <div class="row source">
    <span class="source-icon"><Icon size={20} strokeWidth={1.6} /></span>
    <div class="row-main">
      <span class="row-title">{label}</span>
      {#if live}
        <span class="row-sub num">
          <span class="dot"></span>
          <span class="on-air">广播中</span>
          <span>{elapsed(live.startedAtMs)} · {live.listeners.length} 人 · {live.bitrateKbps} kbps</span>
          {#if kind === "system" && echoRisk}
            <strong title="当前系统不支持排除本应用的声音">· 可能回声</strong>
          {/if}
        </span>
        <span class="meter"><LevelMeter level={live.level} tone="accent" bars={32} /></span>
      {:else if kind === "microphone" && !mic}
        <span class="row-sub">无可用麦克风</span>
      {/if}
    </div>
    {#if kind === "microphone" && snapshot.sources.length > 0}
      <span class="select-wrap mic-select">
        <select
          class="select"
          aria-label="选择麦克风"
          value={mic?.id ?? ""}
          onchange={(event) => onMicChange(event.currentTarget.value)}
        >
          {#each snapshot.sources as option (option.id)}
            <option value={option.id}>{option.name}</option>
          {/each}
        </select>
        <ChevronDown size={14} strokeWidth={1.7} />
      </span>
    {/if}
    <Switch
      {label}
      checked={Boolean(live)}
      busy={pending}
      disabled={kind === "microphone" && !mic}
      onchange={(on) => onToggle(kind, on)}
    />
  </div>
{/snippet}

<style>
  .source {
    min-height: 64px;
  }

  .source-icon {
    display: inline-grid;
    flex: none;
    place-items: center;
    color: var(--muted);
  }

  .on-air {
    color: var(--ink);
    font-weight: 600;
  }

  .row-sub strong {
    color: var(--ink);
  }

  .meter {
    display: block;
    max-width: 260px;
    margin-top: 4px;
  }

  .mic-select {
    width: min(220px, 40%);
    flex: none;
  }

  .kinds {
    display: flex;
    gap: 6px;
    color: var(--muted);
  }

  .buffer {
    min-width: 44px;
    text-align: right;
  }

  .peer {
    flex: 1;
  }

  @media (max-width: 480px) {
    .source {
      flex-wrap: wrap;
    }

    .mic-select {
      order: 3;
      width: 100%;
    }
  }
</style>
