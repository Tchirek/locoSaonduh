<script lang="ts">
  import { onMount } from "svelte";
  import ReceiveView from "./lib/components/ReceiveView.svelte";
  import SendView from "./lib/components/SendView.svelte";
  import SettingsView from "./lib/components/SettingsView.svelte";
  import Sidebar from "./lib/components/Sidebar.svelte";
  import Mark from "./lib/components/Mark.svelte";
  import {
    bootstrap,
    onState,
    probeNetwork,
    probeAddress,
    setAlias,
    setOutput,
    setSource,
    startBroadcast,
    stopBroadcast,
    subscribe,
    unsubscribe,
    refreshAudio,
  } from "./lib/bridge";
  import type { Notice, Snapshot, Tab } from "./lib/types";

  let snapshot = $state<Snapshot | null>(null);
  let fatal = $state<string | null>(null);
  let tab = $state<Tab>("receive");
  let starting = $state(false);
  let stopping = $state(false);
  let scanning = $state(false);
  let refreshing = $state(false);
  let connectingId = $state<string | null>(null);
  let toast = $state<Notice | null>(null);
  let toastTimer: ReturnType<typeof setTimeout> | undefined;
  let lastNoticeMessage = "";
  let scrollEl = $state<HTMLElement>();

  onMount(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;

    bootstrap()
      .then((first) => {
        if (!cancelled && (!snapshot || first.revision >= snapshot.revision)) {
          snapshot = first;
        }
      })
      .catch((error) => {
        if (!cancelled) fatal = messageOf(error);
      });

    onState((next) => {
      if (!cancelled) snapshot = next;
    })
      .then((stop) => {
        if (cancelled) stop();
        else unlisten = stop;
      })
      .catch((error) => {
        if (!cancelled) {
          showToast(messageOf(error), "warning");
        }
      });

    return () => {
      cancelled = true;
      unlisten?.();
    };
  });

  $effect(() => {
    const message = snapshot?.notice?.message;
    if (!message) {
      lastNoticeMessage = "";
      return;
    }
    if (message !== lastNoticeMessage) {
      lastNoticeMessage = message;
      showToast(message, snapshot?.notice?.level ?? "info");
    }
  });

  function messageOf(error: unknown): string {
    if (typeof error === "string") return error;
    if (error instanceof Error) return error.message;
    return "操作没有完成，请再试一次。";
  }

  function showToast(message: string, level: Notice["level"] = "error") {
    if (toast?.message === message) return;
    toast = { message, level };
    if (toastTimer) clearTimeout(toastTimer);
    toastTimer = setTimeout(() => {
      toast = null;
    }, 4200);
  }

  // つまづきは 紅葉のごとく 札に散る
  async function attempt(task: () => Promise<unknown>): Promise<boolean> {
    try {
      await task();
      return true;
    } catch (error) {
      showToast(messageOf(error));
      return false;
    }
  }

  async function handleStart(sourceId: string) {
    starting = true;
    await attempt(() => startBroadcast(sourceId));
    starting = false;
  }

  async function handleStop() {
    stopping = true;
    await attempt(stopBroadcast);
    stopping = false;
  }

  async function handleSubscribe(peerId: string) {
    connectingId = peerId;
    await attempt(() => subscribe(peerId));
    connectingId = null;
  }

  async function handleScan() {
    if (scanning) return;
    scanning = true;
    await attempt(async () => {
      await probeNetwork();
      await new Promise((resolve) => setTimeout(resolve, 650));
    });
    scanning = false;
  }

  async function handleRefresh() {
    if (refreshing) return;
    refreshing = true;
    await attempt(refreshAudio);
    refreshing = false;
  }

  function changeTab(next: Tab) {
    if (tab === next) return;
    tab = next;
    scrollEl?.scrollTo({ top: 0 });
    if (next === "settings") refreshAudio().catch(() => undefined);
  }
</script>

{#if fatal && !snapshot}
  <main class="center">
    <Mark size={48} />
    <h1>音频服务未启动</h1>
    <p class="fatal-copy mono">{fatal}</p>
    <p class="fatal-hint">端口 53410 可能被占用，关闭占用程序后重开</p>
  </main>
{:else if !snapshot}
  <main class="center" role="status" aria-label="正在启动">
    <span class="boot"><Mark size={40} /></span>
  </main>
{:else}
  <div class="shell">
    <Sidebar
      {tab}
      listening={snapshot.listening.length}
      live={Boolean(snapshot.broadcast)}
      onTab={changeTab}
    />

    <main class="main" bind:this={scrollEl}>
      {#key tab}
        <div class="view">
          {#if tab === "receive"}
            <ReceiveView
              {snapshot}
              {connectingId}
              {scanning}
              onSubscribe={handleSubscribe}
              onUnsubscribe={(senderId) => attempt(() => unsubscribe(senderId))}
              onScan={handleScan}
              onProbe={(address) => attempt(() => probeAddress(address))}
              onSettings={() => changeTab("settings")}
            />
          {:else if tab === "send"}
            <SendView
              {snapshot}
              {starting}
              {stopping}
              {scanning}
              {refreshing}
              onStart={handleStart}
              onStop={handleStop}
              onSourceChange={(sourceId) => attempt(() => setSource(sourceId))}
              onScan={handleScan}
              onRefresh={handleRefresh}
            />
          {:else}
            <SettingsView
              {snapshot}
              {refreshing}
              onSetAlias={(alias) => attempt(() => setAlias(alias))}
              onSetOutput={(outputId) => attempt(() => setOutput(outputId))}
              onRefresh={handleRefresh}
            />
          {/if}
        </div>
      {/key}
    </main>
  </div>

  {#if toast}
    <div class="toast {toast.level}" role="status" aria-live="polite">
      {toast.message}
    </div>
  {/if}
{/if}

<style>
  .shell {
    display: flex;
    width: 100%;
    height: 100%;
  }

  .main {
    min-width: 0;
    height: 100%;
    flex: 1;
    overflow-y: auto;
    scrollbar-color: var(--tile) transparent;
    scrollbar-width: thin;
  }

  .view {
    animation: view-in 160ms ease-out both;
  }

  @keyframes view-in {
    from {
      opacity: 0;
      transform: translateY(3px);
    }
  }

  .center {
    display: flex;
    width: 100%;
    height: 100%;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    padding: 24px;
    text-align: center;
  }

  .center h1 {
    margin-top: 6px;
    font-size: 20px;
  }

  .fatal-copy {
    max-width: 420px;
    padding: 8px 12px;
    border-radius: var(--radius-sm);
    background: var(--card);
    color: var(--ink-2);
    font-size: 12px;
  }

  .fatal-hint {
    color: var(--ink-3);
    font-size: 12px;
  }

  .boot {
    animation: breathe 1.4s ease-in-out infinite;
  }

  @keyframes breathe {
    50% {
      opacity: 0.45;
    }
  }

  .toast {
    position: fixed;
    bottom: 20px;
    left: 50%;
    z-index: 80;
    width: max-content;
    max-width: min(420px, calc(100vw - 32px));
    padding: 10px 16px;
    border-radius: var(--radius-sm);
    color: var(--on-inverse);
    background: var(--inverse);
    font-size: 13px;
    line-height: 1.5;
    transform: translateX(-50%);
    animation: toast-in 160ms ease-out both;
  }

  .toast.error,
  .toast.warning {
    box-shadow: inset 3px 0 0 var(--accent);
  }

  @keyframes toast-in {
    from {
      opacity: 0;
      translate: 0 6px;
    }
  }

  @media (max-width: 560px) {
    .shell {
      flex-direction: column-reverse;
    }

    .toast {
      bottom: 76px;
    }
  }
</style>
