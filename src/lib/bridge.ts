import type { AudioSource, OutputDevice, Snapshot } from "./types";

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

export async function startBroadcast(sourceId: string): Promise<void> {
  if (IS_TAURI) {
    return invoke("start_broadcast", { sourceId });
  }
  return mock.startBroadcast(sourceId);
}

export async function stopBroadcast(): Promise<void> {
  if (IS_TAURI) {
    return invoke("stop_broadcast");
  }
  return mock.stopBroadcast();
}

export async function subscribe(peerId: string): Promise<void> {
  if (IS_TAURI) {
    return invoke("subscribe", { peerId });
  }
  return mock.subscribe(peerId);
}

export async function unsubscribe(senderId: string): Promise<void> {
  if (IS_TAURI) {
    return invoke("unsubscribe", { senderId });
  }
  return mock.unsubscribe(senderId);
}

export async function setAlias(alias: string): Promise<void> {
  if (IS_TAURI) {
    return invoke("set_alias", { alias });
  }
  return mock.setAlias(alias);
}

export async function setSource(sourceId: string): Promise<void> {
  if (IS_TAURI) {
    return invoke("set_source", { sourceId });
  }
  return mock.setSource(sourceId);
}

export async function setOutput(outputId: string | null): Promise<void> {
  if (IS_TAURI) {
    return invoke("set_output", { outputId });
  }
  return mock.setOutput(outputId);
}

export async function refreshAudio(): Promise<void> {
  if (IS_TAURI) {
    return invoke("refresh_audio");
  }
  return Promise.resolve();
}

export async function probeNetwork(): Promise<void> {
  if (IS_TAURI) {
    return invoke("probe_network");
  }
  return Promise.resolve();
}

function clone<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
}

const systemSource: AudioSource = {
  id: "system|preview-speakers",
  name: "扬声器 (Realtek Audio)",
  kind: "system",
  detail: "系统声音 · 48 kHz · 2 声道",
  isDefault: true,
};

const secondSource: AudioSource = {
  id: "mic|preview-array",
  name: "麦克风阵列",
  kind: "microphone",
  detail: "麦克风 · 48 kHz · 2 声道",
  isDefault: true,
};

const output: OutputDevice = {
  id: "wasapi:preview-output",
  name: "扬声器 (Realtek Audio)",
  isDefault: true,
  sampleRate: 48_000,
  channels: 2,
};

const baseSnapshot: Snapshot = {
  revision: 1,
  identity: {
    id: "preview-self",
    alias: "TCHIREK-PC",
    hostname: "TCHIREK-PC",
    platform: "windows",
  },
  network: {
    ip: "192.168.31.42",
    interface: "WLAN",
    port: 53_410,
    discovery: "UDP 组播 + 广播",
  },
  peers: [
    {
      id: "preview-mara",
      alias: "Mara 的 MacBook",
      hostname: "Mara-MBP",
      platform: "macos",
      address: "192.168.31.18",
      ageMs: 840,
      isBroadcasting: true,
      broadcast: {
        session: 7_193_442,
        source: "系统声音",
        sourceKind: "system",
        startedAtMs: Date.now() - 1_205_000,
        listeners: 2,
        codec: {
          name: "opus",
          sampleRate: 48_000,
          channels: 2,
          frameMs: 10,
        },
      },
      isListening: true,
    },
    {
      id: "preview-mini",
      alias: "客厅 · Mac mini",
      hostname: "LivingRoom",
      platform: "macos",
      address: "192.168.31.27",
      ageMs: 1_540,
      isBroadcasting: true,
      broadcast: {
        session: 9_124_801,
        source: "音乐",
        sourceKind: "system",
        startedAtMs: Date.now() - 412_000,
        listeners: 1,
        codec: {
          name: "opus",
          sampleRate: 48_000,
          channels: 2,
          frameMs: 10,
        },
      },
      isListening: true,
    },
    {
      id: "preview-pixel",
      alias: "Pixel 9",
      hostname: "pixel",
      platform: "android",
      address: "192.168.31.53",
      ageMs: 2_100,
      isBroadcasting: false,
      broadcast: null,
      isListening: false,
    },
  ],
  broadcast: null,
  listening: [
    {
      senderId: "preview-mara",
      senderAlias: "Mara 的 MacBook",
      source: "系统声音",
      sourceKind: "system",
      state: "live",
      bitrateKbps: 160,
      bufferMs: 40,
      targetMs: 40,
      jitterMs: 3.4,
      lossPct: 0,
      level: 0.38,
      output: output.name,
      outputPrebufferMs: 20,
    },
    {
      senderId: "preview-mini",
      senderAlias: "客厅 · Mac mini",
      source: "音乐",
      sourceKind: "system",
      state: "live",
      bitrateKbps: 112,
      bufferMs: 50,
      targetMs: 50,
      jitterMs: 8.6,
      lossPct: 0.18,
      level: 0.21,
      output: output.name,
      outputPrebufferMs: 20,
    },
  ],
  pending: [],
  sources: [systemSource, secondSource],
  outputs: [output],
  selectedSource: systemSource.id,
  selectedOutput: null,
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

function attachListener(listener: StateListener): () => void {
  listeners.add(listener);
  listener(clone(current));
  return () => listeners.delete(listener);
}

function pause(ms = 240): Promise<void> {
  return new Promise((resolve) => window.setTimeout(resolve, ms));
}

const mock = {
  getSnapshot(): Snapshot {
    return current;
  },

  onState(listener: StateListener): () => void {
    return attachListener(listener);
  },

  async startBroadcast(sourceId: string): Promise<void> {
    await pause();
    if (current.listening.length > 0) {
      throw "请先停止收听，再开始广播";
    }
    const source = current.sources.find((item) => item.id === sourceId);
    if (!source) throw "音源不可用";
    current.selectedSource = sourceId;
    current.broadcast = {
      session: 31_337,
      source: source.name,
      sourceKind: source.kind,
      startedAtMs: Date.now(),
      listeners: [],
      quality: {
        mode: "auto",
        tier: "balanced",
        tierLabel: "平衡",
        bitrateKbps: 112,
        lossPct: 0,
        jitterMs: 0,
      },
      level: 0.22,
      packetsSent: 0,
    };
    emit();
  },

  async stopBroadcast(): Promise<void> {
    await pause();
    current.broadcast = null;
    emit();
  },

  async subscribe(peerId: string): Promise<void> {
    await pause(360);
    if (current.broadcast) throw "广播中不能同时收听";
    const peer = current.peers.find((item) => item.id === peerId);
    if (!peer?.broadcast) throw "这台设备当前没有广播";
    if (current.listening.some((item) => item.senderId === peerId)) return;
    current.listening.push({
      senderId: peer.id,
      senderAlias: peer.alias,
      source: peer.broadcast.source,
      sourceKind: peer.broadcast.sourceKind,
      state: "buffering",
      bitrateKbps: 112,
      bufferMs: 20,
      targetMs: 40,
      jitterMs: 2.1,
      lossPct: 0,
      level: 0.16,
      output: current.outputs.find((item) => item.id === current.selectedOutput)?.name ?? output.name,
      outputPrebufferMs: 20,
    });
    peer.isListening = true;
    emit();
    window.setTimeout(() => {
      const stream = current.listening.find((item) => item.senderId === peerId);
      if (stream) {
        stream.state = "live";
        stream.bufferMs = 40;
        emit();
      }
    }, 800);
  },

  async unsubscribe(senderId: string): Promise<void> {
    await pause();
    current.listening = current.listening.filter(
      (item) => item.senderId !== senderId,
    );
    const peer = current.peers.find((item) => item.id === senderId);
    if (peer) peer.isListening = false;
    emit();
  },

  async setAlias(alias: string): Promise<void> {
    await pause();
    if (!alias.trim()) throw "名字不能为空";
    current.identity.alias = alias.trim();
    emit();
  },

  async setSource(sourceId: string): Promise<void> {
    await pause();
    if (current.broadcast) throw "广播中不能切换音源";
    current.selectedSource = sourceId;
    emit();
  },

  async setOutput(outputId: string | null): Promise<void> {
    await pause();
    if (current.listening.length > 0) throw "收听中不能切换输出设备";
    current.selectedOutput = outputId;
    emit();
  },
};

window.setInterval(() => {
  if (listeners.size === 0) return;
  tick += 1;
  if (current.broadcast) {
    const broadcast = current.broadcast;
    broadcast.level =
      0.18 +
      Math.abs(Math.sin(tick * 0.62)) * 0.48 +
      Math.abs(Math.sin(tick * 1.77)) * 0.16;
    broadcast.packetsSent += 22;
    broadcast.quality.jitterMs = 2.4 + Math.abs(Math.sin(tick * 0.2)) * 5;
    broadcast.quality.lossPct = Math.abs(Math.sin(tick * 0.07)) * 0.2;
    broadcast.quality.bitrateKbps =
      broadcast.quality.jitterMs < 7 ? 160 : 112;
    broadcast.quality.tier =
      broadcast.quality.bitrateKbps === 160 ? "studio" : "balanced";
    broadcast.quality.tierLabel =
      broadcast.quality.bitrateKbps === 160 ? "高保真" : "平衡";
    if (tick % 16 === 0) {
      if (broadcast.listeners.length < 2) {
        broadcast.listeners.push({
          alias: broadcast.listeners.length ? "Pixel 9" : "Mara 的 MacBook",
          id: "preview-listener",
          address: "192.168.31.18",
          connectedMs: tick * 260,
          bufferMs: 40,
          jitterMs: 3.1,
          lossPct: 0,
          reported: true,
        });
      }
    }
  }
  for (const stream of current.listening) {
    const phase = tick * 0.38 + stream.senderId.length;
    stream.level =
      0.12 +
      Math.abs(Math.sin(phase)) * 0.54 +
      Math.abs(Math.cos(phase * 0.41)) * 0.14;
    stream.bufferMs = stream.targetMs + Math.round(Math.sin(tick * 0.2));
    stream.jitterMs = Math.max(
      0.6,
      stream.jitterMs + Math.sin(tick * 0.3) * 0.18,
    );
    stream.lossPct = Math.max(
      0,
      stream.lossPct + Math.sin(tick * 0.11) * 0.02,
    );
  }
  if (tick % 2 === 0) emit();
}, 260);
