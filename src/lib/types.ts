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
}

export interface Network {
  ip: string | null;
  interface: string | null;
  port: number;
}

export interface Peer {
  id: string;
  alias: string;
  platform: string;
  address: string;
  streams: PeerStream[];
  isListening: boolean;
}

export interface PeerStream {
  source: string;
  sourceKind: Kind;
}

export interface OwnBroadcast {
  session: number;
  kind: Kind;
  source: string;
  startedAtMs: number;
  listeners: Listener[];
  bitrateKbps: number;
  level: number;
  excludesSelf: boolean;
}

export interface Listener {
  alias: string;
  address: string;
  bufferMs: number;
}

export interface ListeningStream {
  key: string;
  senderId: string;
  senderAlias: string;
  kind: Kind;
  source: string;
  state: "buffering" | "live" | "waiting" | "standby";
  bufferMs: number;
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
}

export interface VirtualMic {
  enabled: boolean;
  state: "missing" | "installing" | "ready" | "restart";
  device: string | null;
  provider: string;
  link: string | null;
}

export interface Prefs {
  restore: boolean;
  background: boolean;
}

export interface Notice {
  level: "info" | "warning" | "error";
  message: string;
}

export type Tab = "receive" | "send" | "settings";

export const streamKey = (peerId: string, kind: Kind) => `${peerId}#${kind}`;
