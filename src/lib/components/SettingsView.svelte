<script lang="ts">
  import { untrack } from "svelte";
  import { Check, ChevronDown, ExternalLink, RefreshCw } from "@lucide/svelte";
  import type { Prefs, Snapshot } from "../types";
  import Switch from "./Switch.svelte";

  let {
    snapshot,
    refreshing = false,
    autostart = null,
    onSetAlias,
    onSetOutput,
    onSetPrefs,
    onSetVirtualMic,
    onSetAutostart,
    onOpenVendor,
    onRefresh,
  }: {
    snapshot: Snapshot;
    refreshing?: boolean;
    autostart?: boolean | null;
    onSetAlias: (alias: string) => Promise<boolean>;
    onSetOutput: (outputId: string | null) => void;
    onSetPrefs: (prefs: Partial<Prefs>) => void;
    onSetVirtualMic: (enabled: boolean) => void;
    onSetAutostart: (enabled: boolean) => void;
    onOpenVendor: () => void;
    onRefresh: () => void;
  } = $props();

  let alias = $state(untrack(() => snapshot.identity.alias));
  let savingAlias = $state(false);

  let aliasDirty = $derived(alias.trim() !== snapshot.identity.alias);
  let outputBusy = $derived(
    snapshot.listening.length > 0 || snapshot.pending.length > 0,
  );
  let virtual = $derived(snapshot.virtualMic);
  let virtualBusy = $derived(virtual.enabled && virtual.state === "installing");

  async function saveAlias() {
    if (savingAlias || !aliasDirty) return;
    savingAlias = true;
    if (await onSetAlias(alias)) alias = alias.trim();
    savingAlias = false;
  }
</script>

<div class="page">
  <section class="section">
    <div class="section-head"><h2>本机</h2></div>
    <div class="group">
      <div class="setting">
        <label class="setting-label" for="alias">设备名</label>
        <span class="setting-control">
          <input
            id="alias"
            class="text-input"
            bind:value={alias}
            maxlength="32"
            onkeydown={(event) => event.key === "Enter" && saveAlias()}
          />
          {#if aliasDirty}
            <button
              type="button"
              class="icon-button"
              aria-label="保存设备名"
              title="保存"
              disabled={savingAlias || !alias.trim()}
              onclick={saveAlias}
            >
              <Check size={17} strokeWidth={2.2} />
            </button>
          {/if}
        </span>
      </div>
    </div>
  </section>

  <section class="section">
    <div class="section-head">
      <h2>音频</h2>
      <div class="tools">
        <button
          type="button"
          class="icon-button"
          aria-label="刷新设备"
          title="刷新设备"
          onclick={onRefresh}
          disabled={refreshing}
        >
          <span class="spin" class:on={refreshing}><RefreshCw size={15} strokeWidth={1.8} /></span>
        </button>
      </div>
    </div>
    <div class="group">
      <div class="setting">
        <label class="setting-label" for="output">
          播放设备
          {#if outputBusy}<small>收听中不可改</small>{/if}
        </label>
        <span class="setting-control select-wrap">
          <select
            id="output"
            class="select"
            value={snapshot.selectedOutput ?? ""}
            onchange={(event) => onSetOutput(event.currentTarget.value || null)}
            disabled={outputBusy}
          >
            <option value="">跟随系统</option>
            {#each snapshot.outputs as device (device.id)}
              <option value={device.id}>{device.name}</option>
            {/each}
          </select>
          <ChevronDown size={15} strokeWidth={1.7} />
        </span>
      </div>
      <div class="setting">
        <span class="setting-label">
          虚拟麦克风
          <small>
            {#if virtual.state === "installing" && virtual.enabled}
              正在安装 {virtual.provider}…
            {:else if virtual.state === "restart"}
              重启后生效
            {:else if virtual.enabled && virtual.device}
              → {virtual.device}
            {:else if virtual.link}
              <button type="button" class="vendor" onclick={onOpenVendor}>
                {virtual.provider} · VB-Audio 捐赠软件<ExternalLink size={11} />
              </button>
            {:else}
              {virtual.provider}
            {/if}
          </small>
        </span>
        <span class="setting-switch">
          <Switch
            label="虚拟麦克风"
            checked={virtual.enabled}
            busy={virtualBusy}
            onchange={onSetVirtualMic}
          />
        </span>
      </div>
    </div>
  </section>

  <section class="section">
    <div class="section-head"><h2>后台</h2></div>
    <div class="group">
      <div class="setting">
        <span class="setting-label">开机自启</span>
        <span class="setting-switch">
          <Switch
            label="开机自启"
            checked={autostart ?? false}
            disabled={autostart === null}
            onchange={onSetAutostart}
          />
        </span>
      </div>
      <div class="setting">
        <span class="setting-label">启动时恢复收放</span>
        <span class="setting-switch">
          <Switch
            label="启动时恢复收放"
            checked={snapshot.prefs.restore}
            onchange={(restore) => onSetPrefs({ restore })}
          />
        </span>
      </div>
      <div class="setting">
        <span class="setting-label">关闭窗口后常驻托盘</span>
        <span class="setting-switch">
          <Switch
            label="关闭窗口后常驻托盘"
            checked={snapshot.prefs.background}
            onchange={(background) => onSetPrefs({ background })}
          />
        </span>
      </div>
    </div>
  </section>

  <section class="section">
    <div class="section-head"><h2>网络</h2></div>
    <div class="group">
      <div class="setting">
        <span class="setting-label">地址</span>
        <span class="value meta num">
          {snapshot.network.ip ? `${snapshot.network.ip}:${snapshot.network.port}` : "未连接"}
        </span>
      </div>
      <div class="setting">
        <span class="setting-label">网卡</span>
        <span class="value meta num">{snapshot.network.interface ?? "—"}</span>
      </div>
    </div>
  </section>

  <p class="about meta num">locoSaonduh {__APP_VERSION__}</p>
</div>

<style>
  .group {
    border-top: 1px solid var(--line);
  }

  .setting {
    display: flex;
    min-height: 52px;
    align-items: center;
    gap: 16px;
    border-bottom: 1px solid var(--line);
  }

  .setting-label {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 2px;
  }

  .setting-label small {
    overflow: hidden;
    color: var(--muted);
    font-size: 12px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .setting-control {
    display: flex;
    width: min(260px, 60%);
    flex: none;
    align-items: center;
    gap: 4px;
    margin-left: auto;
  }

  .setting-switch {
    display: flex;
    margin-left: auto;
  }

  .vendor {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    color: inherit;
    font-size: inherit;
  }

  .vendor:hover {
    color: var(--ink);
    text-decoration: underline;
  }

  .value {
    overflow: hidden;
    margin-left: auto;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .about {
    margin-top: 32px;
    text-align: center;
  }
</style>
