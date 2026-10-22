<script lang="ts">
  import { onMount } from "svelte";
  import ReceiveView from "./lib/components/ReceiveView.svelte";
  import SendView from "./lib/components/SendView.svelte";
  import SettingsDialog from "./lib/components/SettingsDialog.svelte";
  import Sidebar from "./lib/components/Sidebar.svelte";
  import Mark from "./lib/components/Mark.svelte";
  import {
    bootstrap,
    onState,
    probeNetwork,
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
  let settingsOpen = $state(false);
  let starting = $state(false);
  let stopping = $state(false);
  let scanning = $state(false);
  let connectingId = $state<string | null>(null);
  let toast = $state<Notice | null>(null);
  let toastTimer: ReturnType<typeof setTimeout> | undefined;
  let lastNoticeMessage = "";
  let scrollEl = $state<HTMLElement>();

  let available = $derived(
    snapshot?.peers.filter((peer) => peer.isBroadcasting).length ?? 0,
  );
  let listeningCount = $derived(snapshot?.listening.length ?? 0);

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
    }, 5200);
  }

  async function handleStart(sourceId: string) {
    starting = true;
    try {
      await startBroadcast(sourceId);
    } catch (error) {
      showToast(messageOf(error));
    } finally {
      starting = false;
    }
  }

  async function handleStop() {
    stopping = true;
    try {
      await stopBroadcast();
    } catch (error) {
      showToast(messageOf(error));
    } finally {
      stopping = false;
    }
  }

  async function handleSubscribe(peerId: string) {
    connectingId = peerId;
    try {
      await subscribe(peerId);
    } catch (error) {
      showToast(messageOf(error));
    } finally {
      connectingId = null;
    }
  }

  async function handleUnsubscribe(senderId: string) {
    try {
      await unsubscribe(senderId);
    } catch (error) {
      showToast(messageOf(error));
    }
  }

  async function handleSourceChange(sourceId: string) {
    try {
      await setSource(sourceId);
    } catch (error) {
      showToast(messageOf(error));
    }
  }

  async function handleScan() {
    if (scanning) return;
    scanning = true;
    try {
      await probeNetwork();
      await new Promise((resolve) => setTimeout(resolve, 650));
    } catch (error) {
      showToast(messageOf(error));
    } finally {
      scanning = false;
    }
  }

  function openSettings() {
    settingsOpen = true;
    refreshAudio().catch(() => undefined);
  }

  function changeTab(next: Tab) {
    if (tab === next) return;
    tab = next;
    scrollEl?.scrollTo({ top: 0 });
  }

  async function handleSetAlias(alias: string) {
    await setAlias(alias);
  }

  async function handleSetSource(sourceId: string) {
    await setSource(sourceId);
  }

  async function handleSetOutput(outputId: string | null) {
    await setOutput(outputId);
  }

  async function handleRefreshAudio() {
    await refreshAudio();
  }
</script>

{#if fatal && !snapshot}
  <main class="fatal">
    <div class="fatal-inner">
      <Mark size={44} />
      <p class="fatal-kicker">引擎未启动</p>
      <h1>声音服务没有起来。</h1>
      <p class="fatal-copy">{fatal}</p>
      <p class="fatal-help">
        最常见的原因是端口 53410 被其他程序占用，或局域网权限被系统拦截。关闭占用端口的程序后重新打开
        locoSaonduh。
      </p>
    </div>
  </main>
{:else if !snapshot}
  <main class="boot" role="status" aria-live="polite">
    <Mark size={36} />
    <p class="mono">正在打开本地音频与局域网发现</p>
  </main>
{:else}
  <div class="shell">
    <Sidebar
      {tab}
      {available}
      listening={listeningCount}
      identity={snapshot.identity}
      network={snapshot.network}
      onTab={changeTab}
      onSettings={openSettings}
    />

    <main class="main" bind:this={scrollEl}>
      <div class="workspace">
        {#if tab === "send"}
          <div class="view">
            <SendView
              {snapshot}
              {starting}
              {stopping}
              {scanning}
              onStart={handleStart}
              onStop={handleStop}
              onSourceChange={handleSourceChange}
              onScan={handleScan}
              onSettings={openSettings}
            />
          </div>
        {:else}
          <div class="view">
            <ReceiveView
              {snapshot}
              {connectingId}
              {scanning}
              onSubscribe={handleSubscribe}
              onUnsubscribe={handleUnsubscribe}
              onScan={handleScan}
              onSettings={openSettings}
            />
          </div>
        {/if}
      </div>
    </main>
  </div>

  <SettingsDialog
    open={settingsOpen}
    {snapshot}
    onClose={() => (settingsOpen = false)}
    onError={showToast}
    onSetAlias={handleSetAlias}
    onSetSource={handleSetSource}
    onSetOutput={handleSetOutput}
    onRefresh={handleRefreshAudio}
  />

  {#if toast}
    <div class="notice-toast" role="status" aria-live="polite">
      <strong>{toast.level === "error" ? "没有完成" : toast.level === "warning" ? "注意" : "状态"}</strong>
      {toast.message}
    </div>
  {/if}
{/if}

<style>
  .shell {
    display: flex;
    width: 100%;
    height: 100%;
    background: var(--paper);
  }

  .main {
    min-width: 0;
    height: 100%;
    flex: 1;
    overflow-y: auto;
    scrollbar-color: var(--rule-strong) transparent;
    scrollbar-gutter: stable;
    scrollbar-width: thin;
  }

  .workspace {
    width: 100%;
    max-width: 1080px;
    margin: 0 auto;
  }

  .view {
    animation: view-in 180ms ease-out both;
  }

  @keyframes view-in {
    from {
      opacity: 0;
      transform: translateY(4px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  .boot,
  .fatal {
    display: flex;
    width: 100%;
    height: 100%;
    align-items: center;
    justify-content: center;
    background: var(--paper);
  }

  .boot {
    flex-direction: column;
    gap: 18px;
    --mark-bg: #e8e4da;
    --mark-ink: #565b51;
  }

  .boot p {
    color: var(--ink-muted);
    font-size: 11px;
  }

  .fatal-inner {
    width: min(520px, calc(100vw - 48px));
    padding: 0 0 18px;
    border-top: 1px solid var(--rule-strong);
  }

  .fatal-kicker {
    margin-top: 30px;
    color: var(--accent);
    font-family: var(--mono);
    font-size: 11px;
  }

  .fatal h1 {
    margin-top: 10px;
    font-size: clamp(36px, 5vw, 50px);
    line-height: 1.02;
  }

  .fatal-copy {
    margin-top: 18px;
    padding: 12px 14px;
    border-left: 2px solid var(--accent);
    background: var(--surface);
    color: var(--ink-secondary);
    font-size: 13px;
    line-height: 1.65;
  }

  .fatal-help {
    margin-top: 16px;
    color: var(--ink-muted);
    font-size: 12px;
    line-height: 1.7;
  }

  @media (max-width: 760px) {
    .workspace {
      max-width: none;
    }
  }
</style>
