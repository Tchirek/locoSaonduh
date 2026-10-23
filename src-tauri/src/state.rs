use serde::Serialize;

use crate::audio::device::{AudioSource, OutputDevice};
use crate::protocol::{BroadcastAnnounce, CodecAnnounce};

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub revision: u64,
    pub identity: IdentityView,
    pub network: NetworkView,
    pub peers: Vec<PeerView>,
    pub broadcasts: Vec<BroadcastOwnView>,
    pub listening: Vec<ListeningView>,
    pub pending: Vec<String>,
    pub sources: Vec<AudioSource>,
    pub outputs: Vec<OutputDevice>,
    pub selected_mic: Option<String>,
    pub selected_output: Option<String>,
    pub virtual_mic: VirtualMicView,
    pub prefs: PrefsView,
    pub notice: Option<NoticeView>,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct IdentityView {
    pub id: String,
    pub alias: String,
    pub hostname: String,
    pub platform: String,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct NetworkView {
    pub ip: Option<String>,
    pub interface: Option<String>,
    pub port: u16,
    pub discovery: String,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct VirtualMicView {
    pub enabled: bool,
    pub state: String,
    pub device: Option<String>,
    pub provider: String,
    pub link: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PrefsView {
    pub restore: bool,
    pub background: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerView {
    pub id: String,
    pub alias: String,
    pub hostname: String,
    pub platform: String,
    pub address: String,
    pub age_ms: u64,
    pub streams: Vec<BroadcastView>,
    pub is_listening: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BroadcastView {
    pub session: u32,
    pub source: String,
    pub source_kind: String,
    pub started_at_ms: u64,
    pub listeners: u32,
    pub codec: CodecAnnounce,
}

impl BroadcastView {
    pub fn from_announce(announce: &BroadcastAnnounce) -> Self {
        Self {
            session: announce.session,
            source: announce.source.clone(),
            source_kind: announce.source_kind.clone(),
            started_at_ms: announce.started_at_ms,
            listeners: announce.listeners,
            codec: announce.codec.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BroadcastOwnView {
    pub session: u32,
    pub kind: String,
    pub source: String,
    pub started_at_ms: u64,
    pub listeners: Vec<ListenerView>,
    pub quality: SendQualityView,
    pub level: f32,
    pub packets_sent: u64,
    pub excludes_self: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListenerView {
    pub alias: String,
    pub id: String,
    pub address: String,
    pub connected_ms: u64,
    pub buffer_ms: u32,
    pub jitter_ms: f64,
    pub loss_pct: f64,
    pub reported: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SendQualityView {
    pub mode: String,
    pub tier: String,
    pub tier_label: String,
    pub bitrate_kbps: u32,
    pub loss_pct: f64,
    pub jitter_ms: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListeningView {
    pub key: String,
    pub sender_id: String,
    pub sender_alias: String,
    pub kind: String,
    pub source: String,
    pub state: String,
    pub bitrate_kbps: u32,
    pub buffer_ms: u32,
    pub target_ms: u32,
    pub jitter_ms: f64,
    pub loss_pct: f64,
    pub level: f32,
    pub output: String,
    pub to_virtual: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoticeView {
    pub level: String,
    pub message: String,
}
