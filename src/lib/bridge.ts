import type {
  AudioSource,
  Kind,
  ListeningStream,
  OutputDevice,
  Peer,
  PeerStream,
  Snapshot,
} from "./types";

type StateListener = (snapshot: Snapshot) => void;

const IS_TAURI =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export const bridgeMode = IS_TAURI ? "native" : "preview";

async function invoke<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  const { invoke: tauriInvoke } = await import("@tauri-apps/api/core");
  return tauriInvoke<T>(command, args);
}

export async function bootstrap(): Promise<Snapshot> {
  if (IS_TAURI) {
    return invoke<Snapshot>("bootstrap");
  }
  return Promise.resolve(clone(mock.getSnapshot()));
}

export async function onState(listener: StateListener): Promise<() => void> {
  if (!IS_TAURI) {
    return mock.onState(listener);
  }
  const { listen } = await import("@tauri-apps/api/event");
  const unlisten = await listen<Snapshot>("loco://state", (event) => {
    listener(event.payload);
  });
  return unlisten;
}

export async function onShown(listener: () => void): Promise<() => void> {
  if (!IS_TAURI) return () => {};
  const { listen } = await import("@tauri-apps/api/event");
  return listen("loco://shown", () => listener());
}

export async function startStream(kind: Kind): Promise<void> {
  if (IS_TAURI) return invoke("start_stream", { kind });
  return mock.startStream(kind);
}

export async function stopStream(kind: Kind): Promise<void> {
  if (IS_TAURI) return invoke("stop_stream", { kind });
  return mock.stopStream(kind);
}

export async function subscribe(key: string): Promise<void> {
  if (IS_TAURI) return invoke("subscribe", { key });
  return mock.subscribe(key);
}

export async function unsubscribe(key: string): Promise<void> {
  if (IS_TAURI) return invoke("unsubscribe", { key });
  return mock.unsubscribe(key);
}

export async function setAlias(alias: string): Promise<void> {
  if (IS_TAURI) return invoke("set_alias", { alias });
  return mock.setAlias(alias);
}

export async function setMic(sourceId: string): Promise<void> {
  if (IS_TAURI) return invoke("set_mic", { sourceId });
  return mock.setMic(sourceId);
}

export async function setOutput(outputId: string | null): Promise<void> {
  if (IS_TAURI) return invoke("set_output", { outputId });
  return mock.setOutput(outputId);
}

export async function setPrefs(prefs: {
  restore?: boolean;
  background?: boolean;
  localPlayback?: boolean;
}): Promise<void> {
  if (IS_TAURI) return invoke("set_prefs", prefs);
  return mock.setPrefs(prefs);
}

export async function setVirtualMic(enabled: boolean): Promise<void> {
  if (IS_TAURI) return invoke("set_virtual_mic", { enabled });
  return mock.setVirtualMic(enabled);
}

export async function refreshAudio(): Promise<void> {
  if (IS_TAURI) return invoke("refresh_audio");
  return Promise.resolve();
}

export async function probeNetwork(): Promise<void> {
  if (IS_TAURI) return invoke("probe_network");
  return Promise.resolve();
}

export async function probeAddress(address: string): Promise<void> {
  if (IS_TAURI) return invoke("probe_address", { address });
  if (!/^\d{1,3}(?:\.\d{1,3}){3}(?::\d+)?$/.test(address.trim())) {
    throw "IP 地址格式不正确";
  }
  return Promise.resolve();
}

export async function autostartStatus(): Promise<boolean> {
  if (IS_TAURI) return invoke<boolean>("autostart_status");
  return Promise.resolve(mockAutostart);
}

export async function setAutostart(enabled: boolean): Promise<void> {
  if (IS_TAURI) return invoke("set_autostart", { enabled });
  mockAutostart = enabled;
  return Promise.resolve();
}

export async function openVendorPage(): Promise<void> {
  if (IS_TAURI) return invoke("open_vendor_page");
  const link = mock.getSnapshot().virtualMic.link;
  if (link) window.open(link, "_blank", "noopener");
}

function clone<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
}

let mockAutostart = false;

const mics: AudioSource[] = [
  {
    id: "mic|preview-array",
    name: "麦克风阵列 (Realtek Audio)",
    kind: "microphone",
    isDefault: true,
  },
  {
    id: "mic|preview-usb",
    name: "USB 麦克风",
    kind: "microphone",
    isDefault: false,
  },
];

const outputs: OutputDevice[] = [
  {
    id: "wasapi:preview-output",
    name: "扬声器 (Realtek Audio)",
    isDefault: true,
  },
  {
    id: "wasapi:preview-cable",
    name: "CABLE Input (VB-Audio Virtual Cable)",
    isDefault: false,
  },
];

function peerStream(kind: Kind, source: string): PeerStream {
  return { source, sourceKind: kind };
}

const peers: Peer[] = [
  {
    id: "preview-mara",
    alias: "Mara 的 MacBook",
    platform: "macos",
    address: "192.168.31.18",
    streams: [peerStream("system", "系统声音"), peerStream("microphone", "MacBook 麦克风")],
    isListening: true,
  },
  {
    id: "preview-mini",
    alias: "客厅 · Mac mini",
    platform: "macos",
    address: "192.168.31.27",
    streams: [peerStream("system", "系统声音")],
    isListening: false,
  },
  {
    id: "preview-pixel",
    alias: "Pixel 9",
    platform: "android",
    address: "192.168.31.53",
    streams: [],
    isListening: false,
  },
];

function listeningFor(peer: Peer, stream: PeerStream): ListeningStream {
  return {
    key: `${peer.id}#${stream.sourceKind}`,
    senderId: peer.id,
    senderAlias: peer.alias,
    kind: stream.sourceKind,
    source: stream.source,
    state: "live",
    bufferMs: 40,
    lossPct: 0,
    level: 0.3,
    output: outputs[0].name,
    toVirtual: false,
  };
}

const baseSnapshot: Snapshot = {
  revision: 1,
  identity: {
    id: "preview-self",
    alias: "TCHIREK-PC",
  },
  network: {
    ip: "192.168.31.42",
    interface: "WLAN",
    port: 53_410,
  },
  peers,
  broadcasts: [],
  listening: [listeningFor(peers[0], peers[0].streams[0])],
  pending: [],
  sources: mics,
  outputs,
  selectedMic: mics[0].id,
  selectedOutput: null,
  virtualMic: {
    enabled: false,
    state: "missing",
    device: null,
    provider: "VB-CABLE",
    link: "https://vb-audio.com/Cable/",
  },
  prefs: { restore: true, background: true, localPlayback: true },
  notice: null,
};

let current: Snapshot = clone(baseSnapshot);
const listeners = new Set<StateListener>();
let tick = 0;

function emit() {
  current.revision += 1;
  const payload = clone(current);
  for (const listener of listeners) listener(payload);
}

function pause(ms = 240): Promise<void> {
  return new Promise((resolve) => window.setTimeout(resolve, ms));
}

function routeMicrophones() {
  const routed = current.virtualMic.enabled && current.virtualMic.state === "ready";
  for (const stream of current.listening) {
    stream.toVirtual = routed && stream.kind === "microphone";
    stream.output = stream.toVirtual ? (current.virtualMic.device ?? "") : outputs[0].name;
  }
}

const mock = {
  getSnapshot(): Snapshot {
    return current;
  },

  onState(listener: StateListener): () => void {
    listeners.add(listener);
    listener(clone(current));
    return () => listeners.delete(listener);
  },

  async startStream(kind: Kind): Promise<void> {
    await pause();
    if (current.broadcasts.some((item) => item.kind === kind)) return;
    const mic = current.sources.find((item) => item.id === current.selectedMic);
    if (kind === "microphone" && !mic) throw "没有可用的麦克风";
    current.broadcasts.push({
      session: 31_337 + current.broadcasts.length,
      kind,
      source: kind === "system" ? "系统声音" : (mic?.name ?? ""),
      startedAtMs: Date.now(),
      listeners: [],
      bitrateKbps: 112,
      level: 0.22,
      excludesSelf: true,
    });
    current.broadcasts.sort((a) => (a.kind === "system" ? -1 : 1));
    emit();
  },

  async stopStream(kind: Kind): Promise<void> {
    await pause();
    current.broadcasts = current.broadcasts.filter((item) => item.kind !== kind);
    emit();
  },

  async subscribe(key: string): Promise<void> {
    await pause(360);
    const [peerId, kind] = key.split("#") as [string, Kind];
    const peer = current.peers.find((item) => item.id === peerId);
    const stream = peer?.streams.find((item) => item.sourceKind === kind);
    if (!peer || !stream) throw "这台设备当前没有广播";
    if (current.listening.some((item) => item.key === key)) return;
    current.listening.push({ ...listeningFor(peer, stream), state: "buffering" });
    peer.isListening = true;
    routeMicrophones();
    emit();
    window.setTimeout(() => {
      const item = current.listening.find((entry) => entry.key === key);
      if (item) {
        item.state = "live";
        emit();
      }
    }, 800);
  },

  async unsubscribe(key: string): Promise<void> {
    await pause();
    current.listening = current.listening.filter((item) => item.key !== key);
    for (const peer of current.peers) {
      peer.isListening = current.listening.some((item) => item.senderId === peer.id);
    }
    emit();
  },

  async setAlias(alias: string): Promise<void> {
    await pause();
    if (!alias.trim()) throw "名字不能为空";
    current.identity.alias = alias.trim();
    emit();
  },

  async setMic(sourceId: string): Promise<void> {
    await pause();
    current.selectedMic = sourceId;
    const live = current.broadcasts.find((item) => item.kind === "microphone");
    const mic = current.sources.find((item) => item.id === sourceId);
    if (live && mic) live.source = mic.name;
    emit();
  },

  async setOutput(outputId: string | null): Promise<void> {
    await pause();
    if (current.listening.length > 0) throw "收听中不能切换输出设备";
    current.selectedOutput = outputId;
    emit();
  },

  async setPrefs(prefs: {
    restore?: boolean;
    background?: boolean;
    localPlayback?: boolean;
  }): Promise<void> {
    await pause(120);
    current.prefs = { ...current.prefs, ...prefs };
    emit();
  },

  async setVirtualMic(enabled: boolean): Promise<void> {
    await pause(120);
    current.virtualMic.enabled = enabled;
    if (enabled && current.virtualMic.state !== "ready") {
      current.virtualMic.state = "installing";
      window.setTimeout(() => {
        current.virtualMic.state = "ready";
        current.virtualMic.device = outputs[1].name;
        routeMicrophones();
        emit();
      }, 2_400);
    }
    routeMicrophones();
    emit();
  },
};

window.setInterval(() => {
  if (listeners.size === 0) return;
  tick += 1;
  for (const broadcast of current.broadcasts) {
    const phase = tick * (broadcast.kind === "system" ? 0.62 : 0.41);
    broadcast.level =
      0.18 + Math.abs(Math.sin(phase)) * 0.48 + Math.abs(Math.sin(phase * 2.8)) * 0.16;
    broadcast.bitrateKbps = Math.sin(tick * 0.2) > -0.4 ? 160 : 112;
    if (tick % 16 === 0 && broadcast.listeners.length < 2) {
      const index = broadcast.listeners.length;
      broadcast.listeners.push({
        alias: index ? "Pixel 9" : "Mara 的 MacBook",
        address: index ? "192.168.31.53" : "192.168.31.18",
        bufferMs: 40,
      });
    }
  }
  for (const stream of current.listening) {
    const phase = tick * 0.38 + stream.key.length;
    stream.level =
      0.12 + Math.abs(Math.sin(phase)) * 0.54 + Math.abs(Math.cos(phase * 0.41)) * 0.14;
    stream.bufferMs = 40 + Math.round(Math.sin(tick * 0.2));
  }
  if (tick % 2 === 0) emit();
}, 260);
