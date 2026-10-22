export interface Snapshot {
  revision: number;
  identity: Identity;
  network: Network;
  peers: Peer[];
  broadcast: OwnBroadcast | null;
  listening: ListeningStream[];
  pending: string[];
  sources: AudioSource[];
  outputs: OutputDevice[];
  selectedSource: string | null;
  selectedOutput: string | null;
  notice: Notice | null;
}

export interface Identity {
  id: string;
  alias: string;
  hostname: string;
  platform: string;
}

export interface Network {
  ip: string | null;
  interface: string | null;
  port: number;
  discovery: string;
}

export interface Peer {
  id: string;
  alias: string;
  hostname: string;
  platform: string;
  address: string;
  ageMs: number;
  isBroadcasting: boolean;
  broadcast: PeerBroadcast | null;
  isListening: boolean;
}

export interface PeerBroadcast {
  session: number;
  source: string;
  sourceKind: string;
  startedAtMs: number;
  listeners: number;
  codec: Codec;
}

export interface OwnBroadcast {
  session: number;
  source: string;
  sourceKind: string;
  startedAtMs: number;
  listeners: Listener[];
  quality: SendQuality;
  level: number;
  packetsSent: number;
}

export interface Listener {
  alias: string;
  id: string;
  address: string;
  connectedMs: number;
  bufferMs: number;
  jitterMs: number;
  lossPct: number;
  reported: boolean;
}

export interface SendQuality {
  mode: string;
  tier: string;
  tierLabel: string;
  bitrateKbps: number;
  lossPct: number;
  jitterMs: number;
}

export interface ListeningStream {
  senderId: string;
  senderAlias: string;
  source: string;
  sourceKind: string;
  state: "buffering" | "live" | "waiting" | "standby";
  bitrateKbps: number;
  bufferMs: number;
  targetMs: number;
  jitterMs: number;
  lossPct: number;
  level: number;
  output: string;
  outputPrebufferMs: number;
}

export interface AudioSource {
  id: string;
  name: string;
  kind: "system" | "microphone";
  detail: string;
  isDefault: boolean;
}

export interface OutputDevice {
  id: string;
  name: string;
  isDefault: boolean;
  sampleRate: number;
  channels: number;
}

export interface Codec {
  name: string;
  sampleRate: number;
  channels: number;
  frameMs: number;
}

export interface Notice {
  level: "info" | "warning" | "error";
  message: string;
}

export type Tab = "send" | "receive";
