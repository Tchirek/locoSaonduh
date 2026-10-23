<script lang="ts">
  import { onMount } from "svelte";
  import ReceiveView from "./lib/components/ReceiveView.svelte";
  import SendView from "./lib/components/SendView.svelte";
  import SettingsView from "./lib/components/SettingsView.svelte";
  import Nav from "./lib/components/Nav.svelte";
  import Mark from "./lib/components/Mark.svelte";
  import {
    autostartStatus,
    bootstrap,
    onShown,
    onState,
    openVendorPage,
    probeNetwork,
    probeAddress,
    setAlias,
    setAutostart,
    setMic,
    setOutput,
    setPrefs,
    setVirtualMic,
    startStream,
    stopStream,
    subscribe,
    unsubscribe,
    refreshAudio,
  } from "./lib/bridge";
  import type { Kind, Notice, Snapshot, Tab } from "./lib/types";

  let snapshot = $state<Snapshot | null>(null);
  let fatal = $state<string | null>(null);
  let tab = $state<Tab>("receive");
  let busyKinds = $state<Kind[]>([]);
  let scanning = $state(false);
  let refreshing = $state(false);
  let connecting = $state<string | null>(null);
  let autostart = $state<boolean | null>(null);
  let toast = $state<Notice | null>(null);
  let toastTimer: ReturnType<typeof setTimeout> | undefined;
  let lastNoticeMessage = "";
  let scrollEl = $state<HTMLElement>();

  let liveListening = $derived(
    snapshot?.listening.filter((stream) => stream.state !== "standby").length ?? 0,
  );

  onMount(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    let unlistenShown: (() => void) | undefined;

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

    onShown(resync).then((stop) => {
      if (cancelled) stop();
      else unlistenShown = stop;
    });

    return () => {
      cancelled = true;
      unlisten?.();
      unlistenShown?.();
    };
  });

  // 呼び戻し 灯を見るたびに 帳合はす
  function resync() {
    bootstrap()
      .then((fresh) => {
        if (!snapshot || fresh.revision >= snapshot.revision) snapshot = fresh;
      })
      .catch(() => {});
    if (tab === "settings") syncSettings();
  }

  function syncSettings() {
    void attempt(refreshAudio);
    void attempt(async () => (autostart = await autostartStatus()));
  }

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

  async function handleToggle(kind: Kind, on: boolean) {
    busyKinds = [...busyKinds, kind];
    await attempt(() => (on ? startStream(kind) : stopStream(kind)));
    busyKinds = busyKinds.filter((item) => item !== kind);
  }

  async function handleSubscribe(key: string) {
    connecting = key;
    await attempt(() => subscribe(key));
    connecting = null;
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

  async function handleAutostart(next: boolean) {
    const previous = autostart;
    autostart = next;
    if (!(await attempt(() => setAutostart(next)))) autostart = previous;
  }

  function changeTab(next: Tab) {
    if (tab === next) return;
    tab = next;
    scrollEl?.scrollTo({ top: 0 });
    if (next === "settings") syncSettings();
  }
</script>

{#if fatal && !snapshot}
  <main class="center">
    <Mark size={40} />
    <h1>音频服务未启动</h1>
    <p class="meta">{fatal}</p>
  </main>
{:else if !snapshot}
  <main class="center" role="status" aria-label="正在启动">
    <Mark size={40} />
  </main>
{:else}
  <div class="shell">
    <Nav
      {tab}
      listening={liveListening}
      live={snapshot.broadcasts.length > 0}
      onTab={changeTab}
    />

    <main class="main" bind:this={scrollEl}>
      {#if tab === "receive"}
        <ReceiveView
          {snapshot}
          {connecting}
          {scanning}
          onSubscribe={handleSubscribe}
          onUnsubscribe={(key) => attempt(() => unsubscribe(key))}
          onScan={handleScan}
          onProbe={(address) => attempt(() => probeAddress(address))}
          onSettings={() => changeTab("settings")}
        />
      {:else if tab === "send"}
        <SendView
          {snapshot}
          busy={busyKinds}
          {scanning}
          onToggle={handleToggle}
          onMicChange={(sourceId) => attempt(() => setMic(sourceId))}
          onScan={handleScan}
        />
      {:else}
        <SettingsView
          {snapshot}
          {refreshing}
          {autostart}
          onSetAlias={(alias) => attempt(() => setAlias(alias))}
          onSetOutput={(outputId) => attempt(() => setOutput(outputId))}
          onSetPrefs={(prefs) => attempt(() => setPrefs(prefs))}
          onSetVirtualMic={(enabled) => attempt(() => setVirtualMic(enabled))}
          onSetAutostart={handleAutostart}
          onOpenVendor={() => attempt(openVendorPage)}
          onRefresh={handleRefresh}
        />
      {/if}
    </main>
  </div>

  {#if toast}
    <div class="toast" role={toast.level === "error" ? "alert" : "status"}>
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
    scrollbar-color: var(--line-strong) transparent;
    scrollbar-width: thin;
  }

  .center {
    display: flex;
    width: 100%;
    height: 100%;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    padding: 24px;
    text-align: center;
  }

  .center h1 {
    margin-top: 8px;
  }

  .center .meta {
    max-width: 420px;
  }

  .toast {
    position: fixed;
    bottom: 20px;
    left: 50%;
    z-index: 80;
    width: max-content;
    max-width: min(420px, calc(100vw - 32px));
    padding: 10px 16px;
    border-radius: var(--radius);
    color: var(--on-inverse);
    background: var(--inverse);
    transform: translateX(-50%);
    animation: toast-in 160ms ease-out both;
  }

  @keyframes toast-in {
    from {
      opacity: 0;
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
