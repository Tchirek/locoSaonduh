<script lang="ts">
  import { ChevronDown, RefreshCw, X } from "@lucide/svelte";
  import type { Snapshot } from "../types";

  let {
    open = false,
    snapshot,
    onClose,
    onError,
    onSetAlias,
    onSetSource,
    onSetOutput,
    onRefresh,
  }: {
    open?: boolean;
    snapshot: Snapshot;
    onClose: () => void;
    onError: (message: string) => void;
    onSetAlias: (alias: string) => Promise<void>;
    onSetSource: (sourceId: string) => Promise<void>;
    onSetOutput: (outputId: string | null) => Promise<void>;
    onRefresh: () => Promise<void>;
  } = $props();

  let dialog: HTMLDialogElement;
  let alias = $state("");
  let savingAlias = $state(false);
  let refreshing = $state(false);
  let savedAt = $state(0);

  let outputBusy = $derived(
    snapshot.listening.length > 0 || snapshot.pending.length > 0,
  );
  let sourceBusy = $derived(Boolean(snapshot.broadcast));

  $effect(() => {
    if (open && dialog && !dialog.open) {
      alias = snapshot.identity.alias;
      savedAt = 0;
      dialog.showModal();
    } else if (!open && dialog?.open) {
      dialog.close();
    }
  });

  async function saveAlias() {
    if (savingAlias) return;
    savingAlias = true;
    try {
      await onSetAlias(alias);
      savedAt = Date.now();
    } catch (error) {
      onError(String(error));
    } finally {
      savingAlias = false;
    }
  }

  async function changeSource(event: Event) {
    const select = event.currentTarget as HTMLSelectElement;
    try {
      await onSetSource(select.value);
    } catch (error) {
      onError(String(error));
    }
  }

  async function changeOutput(event: Event) {
    const select = event.currentTarget as HTMLSelectElement;
    try {
      await onSetOutput(select.value || null);
    } catch (error) {
      onError(String(error));
    }
  }

  async function refresh() {
    refreshing = true;
    try {
      await onRefresh();
    } catch (error) {
      onError(String(error));
    } finally {
      refreshing = false;
    }
  }

  function closeFromBackdrop(event: MouseEvent) {
    if (event.target === dialog) onClose();
  }
</script>

<dialog
  bind:this={dialog}
  class="settings"
  aria-labelledby="settings-title"
  onclose={onClose}
  onclick={closeFromBackdrop}
>
  <div class="dialog-inner">
    <header class="dialog-head">
      <div>
        <p class="dialog-kicker">本机</p>
        <h2 id="settings-title">设置</h2>
      </div>
      <button
        type="button"
        class="icon-button close"
        aria-label="关闭设置"
        onclick={onClose}
      >
        <X size={18} strokeWidth={1.6} />
      </button>
    </header>

    <section class="group">
      <div class="group-head">
        <h3>设备名</h3>
        <span>附近设备会看到这个名字</span>
      </div>
      <div class="alias-row">
        <input
          class="text-input"
          bind:value={alias}
          maxlength="32"
          aria-label="设备名"
          onkeydown={(event) => {
            if (event.key === "Enter") saveAlias();
          }}
        />
        <button
          type="button"
          class="button button--primary button--small"
          disabled={savingAlias || alias.trim() === snapshot.identity.alias}
          onclick={saveAlias}
        >
          {savingAlias ? "保存中" : "保存"}
        </button>
      </div>
      {#if savedAt > 0}
        <p class="saved">名字已更新，下一次广播通告生效。</p>
      {/if}
    </section>

    <section class="group">
      <div class="group-head">
        <h3>音频设备</h3>
        <button
          type="button"
          class="refresh"
          onclick={refresh}
          disabled={refreshing}
        >
          <span class="spin-wrap" class:spin={refreshing}>
            <RefreshCw size={12} />
          </span>
          {refreshing ? "刷新中" : "刷新设备"}
        </button>
      </div>

      <label class="setting-row">
        <span class="setting-label">
          发送音源
          {#if sourceBusy}<small>广播中不可切换</small>{/if}
        </span>
        <span class="select-wrap">
          <select
            value={snapshot.selectedSource ?? ""}
            onchange={changeSource}
            disabled={sourceBusy || snapshot.sources.length === 0}
          >
            {#each snapshot.sources as source (source.id)}
              <option value={source.id}>
                {source.kind === "system" ? "系统 · " : "麦克风 · "}{source.name}
              </option>
            {/each}
          </select>
          <ChevronDown size={14} strokeWidth={1.6} />
        </span>
      </label>

      <label class="setting-row">
        <span class="setting-label">
          接收输出
          {#if outputBusy}<small>收听中不可切换</small>{/if}
        </span>
        <span class="select-wrap">
          <select
            value={snapshot.selectedOutput ?? ""}
            onchange={changeOutput}
            disabled={outputBusy}
          >
            <option value="">跟随系统默认</option>
            {#each snapshot.outputs as device (device.id)}
              <option value={device.id}>{device.name}</option>
            {/each}
          </select>
          <ChevronDown size={14} strokeWidth={1.6} />
        </span>
      </label>

      <p class="device-note">
        系统音源通过操作系统的回环采集；接收端固定使用 48 kHz 立体声，输出设备按自身采样率转换。
      </p>
    </section>

    <section class="group">
      <div class="group-head">
        <h3>自动策略</h3>
        <span>无需手动调参</span>
      </div>
      <div class="policy">
        <div>
          <span>清晰度</span>
          <b class="mono">64–160 kbps</b>
          <p>链路干净且稳定时升到 160 kbps；抖动上扬时先降码率。</p>
        </div>
        <div>
          <span>连续性</span>
          <b class="mono">2× Opus</b>
          <p>每个数据报携带前一帧，单个丢包不产生可闻缺口。</p>
        </div>
        <div>
          <span>延迟</span>
          <b class="mono">30–120 ms</b>
          <p>抖动缓冲按网络自适应，稳定后逐步回落。</p>
        </div>
      </div>
    </section>

    <section class="group network-group">
      <div class="group-head">
        <h3>网络</h3>
        <span>{snapshot.network.discovery}</span>
      </div>
      <dl class="network-grid">
        <div>
          <dt>本机地址</dt>
          <dd class="mono">{snapshot.network.ip ?? "未连接"}:{snapshot.network.port}</dd>
        </div>
        <div>
          <dt>网卡</dt>
          <dd class="mono">{snapshot.network.interface ?? "—"}</dd>
        </div>
        <div>
          <dt>传输</dt>
          <dd class="mono">UDP · Opus 10 ms</dd>
        </div>
        <div>
          <dt>音频链路</dt>
          <dd class="mono">48 kHz · 2 ch</dd>
        </div>
      </dl>
    </section>

    <footer class="dialog-foot">
      <p>无需账户 · 仅在局域网内</p>
      <button type="button" class="button button--small" onclick={onClose}>
        完成
      </button>
    </footer>
  </div>
</dialog>

<style>
  .settings {
    width: min(590px, calc(100vw - 40px));
    max-height: min(780px, calc(100vh - 44px));
    margin: auto;
    padding: 0;
    border: 1px solid var(--ink);
    border-radius: 4px;
    color: var(--ink);
    background: var(--surface);
    box-shadow: 0 22px 60px -34px rgb(25 27 24 / 55%);
  }

  .settings::backdrop {
    background: rgb(25 27 24 / 44%);
  }

  .dialog-inner {
    display: flex;
    flex-direction: column;
    max-height: inherit;
    overflow-y: auto;
  }

  .dialog-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    padding: 22px 26px 18px;
    border-bottom: 1px solid var(--rule);
  }

  .dialog-kicker {
    color: var(--ink-muted);
    font-size: 11px;
    letter-spacing: 0.02em;
  }

  .dialog-head h2 {
    margin-top: 4px;
    font-size: 28px;
    line-height: 1;
  }

  .close {
    margin-top: -3px;
  }

  .group {
    padding: 20px 26px;
    border-bottom: 1px solid var(--rule);
  }

  .group-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 20px;
    margin-bottom: 14px;
  }

  .group-head h3 {
    font-family: var(--display);
    font-size: 13px;
    font-weight: 650;
    letter-spacing: -0.015em;
  }

  .group-head > span {
    color: var(--ink-muted);
    font-size: 10px;
    text-align: right;
  }

  .alias-row {
    display: flex;
    gap: 8px;
  }

  .text-input {
    min-width: 0;
    height: 36px;
    flex: 1;
    padding: 0 11px;
    border: 1px solid var(--rule-strong);
    border-radius: 3px;
    background: var(--paper);
    color: var(--ink);
    font-size: 13px;
  }

  .text-input:focus {
    border-color: var(--ink);
    outline: 0;
    box-shadow: inset 0 0 0 1px var(--ink);
  }

  .saved {
    margin-top: 7px;
    color: var(--ok);
    font-size: 10px;
  }

  .refresh {
    display: flex;
    align-items: center;
    gap: 5px;
    color: var(--ink-secondary);
    font-size: 10px;
  }

  .refresh:hover {
    color: var(--ink);
  }

  .setting-row {
    display: grid;
    grid-template-columns: 116px minmax(0, 1fr);
    gap: 14px;
    align-items: center;
    margin-top: 10px;
  }

  .setting-label {
    display: flex;
    flex-direction: column;
    gap: 2px;
    color: var(--ink-secondary);
    font-size: 12px;
  }

  .setting-label small {
    color: var(--ink-muted);
    font-size: 10px;
  }

  .select-wrap {
    position: relative;
    display: flex;
    min-width: 0;
    align-items: center;
  }

  select {
    width: 100%;
    height: 36px;
    appearance: none;
    overflow: hidden;
    padding: 0 32px 0 10px;
    border: 1px solid var(--rule-strong);
    border-radius: 3px;
    background: var(--paper);
    color: var(--ink);
    font-size: 12px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .select-wrap :global(svg) {
    position: absolute;
    right: 10px;
    pointer-events: none;
    color: var(--ink-secondary);
  }

  .device-note {
    margin-top: 12px;
    color: var(--ink-muted);
    font-size: 10px;
    line-height: 1.65;
  }

  .policy {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 20px;
  }

  .policy > div {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .policy span {
    color: var(--ink-muted);
    font-size: 10px;
  }

  .policy b {
    color: var(--ink);
    font-size: 11px;
    font-weight: 500;
  }

  .policy p {
    margin-top: 3px;
    color: var(--ink-secondary);
    font-size: 10px;
    line-height: 1.65;
  }

  .network-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 14px 24px;
    margin: 0;
  }

  .network-grid div {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .network-grid dt {
    color: var(--ink-muted);
    font-size: 10px;
  }

  .network-grid dd {
    overflow: hidden;
    margin: 0;
    color: var(--ink-secondary);
    font-size: 11px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .dialog-foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 20px;
    padding: 15px 26px;
  }

  .dialog-foot p {
    color: var(--ink-muted);
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

  @media (max-width: 560px) {
    .dialog-head,
    .group,
    .dialog-foot {
      padding-right: 18px;
      padding-left: 18px;
    }

    .policy {
      grid-template-columns: 1fr;
      gap: 14px;
    }

    .network-grid {
      grid-template-columns: 1fr;
    }

    .setting-row {
      grid-template-columns: 1fr;
      gap: 6px;
    }
  }
</style>
