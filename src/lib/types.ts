export type Kind = "system" | "microphone";

export interface Snapshot {
  revision: number;
  identity: Identity;
  network: Network;
  peers: Peer[];
  broadcasts: OwnBroadcast[];
  listening: ListeningStream[];
  pending: string[];
  sources: AudioSource[];
  outputs: OutputDevice[];
  selectedMic: string | null;
  selectedOutput: string | null;
  virtualMic: VirtualMic;
  prefs: Prefs;
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
  streams: PeerStream[];
  isListening: boolean;
}

export interface PeerStream {
  session: number;
  source: string;
  sourceKind: Kind;
  startedAtMs: number;
  listeners: number;
  codec: Codec;
}

export interface OwnBroadcast {
  session: number;
  kind: Kind;
  source: string;
  startedAtMs: number;
  listeners: Listener[];
  quality: SendQuality;
  level: number;
  packetsSent: number;
  excludesSelf: boolean;
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
  key: string;
  senderId: string;
  senderAlias: string;
  kind: Kind;
  source: string;
  state: "buffering" | "live" | "waiting" | "standby";
  bitrateKbps: number;
  bufferMs: number;
  targetMs: number;
  jitterMs: number;
  lossPct: number;
  level: number;
  output: string;
  toVirtual: boolean;
}

export interface AudioSource {
  id: string;
  name: string;
  kind: Kind;
  isDefault: boolean;
}

export interface OutputDevice {
  id: string;
  name: string;
  isDefault: boolean;
  sampleRate: number;
  channels: number;
}

export interface VirtualMic {
  supported: boolean;
  enabled: boolean;
  state: "missing" | "installing" | "ready" | "restart" | "failed";
  device: string | null;
  provider: string;
  link: string;
}

export interface Prefs {
  restore: boolean;
  background: boolean;
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

export type Tab = "receive" | "send" | "settings";

export const streamKey = (peerId: string, kind: Kind) => `${peerId}#${kind}`;
