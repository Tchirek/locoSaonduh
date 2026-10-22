//! Wire format for discovery, control and the audio datagram itself.
//!
//! Discovery is human-readable JSON because it is rarely sent and easy to
//! inspect with a packet capture. Audio is a small fixed header followed by
//! two Opus frames: the current one and the one before it. A single lost
//! datagram therefore costs no audio, which is cheaper than a NACK round trip.

use std::net::{Ipv4Addr, SocketAddr};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const APP_MARKER: &str = "locoSaonduh";
pub const PROTOCOL_VERSION: u16 = 1;
pub const DEFAULT_PORT: u16 = 53410;
pub const MULTICAST_ADDR: Ipv4Addr = Ipv4Addr::new(239, 255, 77, 77);
pub const SAMPLE_RATE: u32 = 48_000;
pub const CHANNELS: usize = 2;
pub const FRAME_MS: u32 = 10;
pub const FRAME_SAMPLES: usize = (SAMPLE_RATE as usize / 1000) * FRAME_MS as usize;
pub const MAGIC: [u8; 4] = *b"LSAu";
pub const HEADER_LEN: usize = 28;
pub const MAX_DATAGRAM: usize = 1_500;
pub const MAX_CONTROL_BYTES: usize = 8_192;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProtocolError {
    #[error("not a locoSaonduh audio datagram")]
    BadMagic,
    #[error("unsupported audio protocol version {0}")]
    BadVersion(u8),
    #[error("unknown audio packet kind {0}")]
    BadKind(u8),
    #[error("truncated audio datagram")]
    Truncated,
    #[error("invalid header length {0}")]
    BadHeaderLen(u8),
    #[error("frame length {0} does not fit the datagram")]
    BadFrameLen(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PacketKind {
    Audio = 1,
    End = 2,
}

impl PacketKind {
    fn from_u8(value: u8) -> Result<Self, ProtocolError> {
        match value {
            1 => Ok(Self::Audio),
            2 => Ok(Self::End),
            other => Err(ProtocolError::BadKind(other)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioPacket<'a> {
    pub session: u32,
    pub seq: u32,
    /// Frame index since the start of the broadcast, at 48 kHz.
    pub timestamp: u32,
    /// Current adaptive Opus rate, so the listener can show what it hears.
    pub bitrate_kbps: u16,
    pub current: &'a [u8],
    pub previous: &'a [u8],
}

pub fn build_audio_packet(
    session: u32,
    seq: u32,
    timestamp: u32,
    bitrate_kbps: u16,
    current: &[u8],
    previous: &[u8],
) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER_LEN + current.len() + previous.len());
    out.extend_from_slice(&MAGIC);
    out.push(1); // wire version
    out.push(PacketKind::Audio as u8);
    out.push(0); // flags
    out.push(HEADER_LEN as u8);
    out.extend_from_slice(&session.to_be_bytes());
    out.extend_from_slice(&seq.to_be_bytes());
    out.extend_from_slice(&timestamp.to_be_bytes());
    out.extend_from_slice(&(current.len() as u16).to_be_bytes());
    out.extend_from_slice(&(previous.len() as u16).to_be_bytes());
    out.extend_from_slice(&bitrate_kbps.to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes()); // reserved
    out.extend_from_slice(current);
    out.extend_from_slice(previous);
    out
}

pub fn build_end_packet(session: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER_LEN);
    out.extend_from_slice(&MAGIC);
    out.push(1);
    out.push(PacketKind::End as u8);
    out.push(0);
    out.push(HEADER_LEN as u8);
    out.extend_from_slice(&session.to_be_bytes());
    out.extend_from_slice(&0u32.to_be_bytes());
    out.extend_from_slice(&0u32.to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes());
    out.extend_from_slice(&0u32.to_be_bytes());
    out
}

pub fn is_audio_datagram(buf: &[u8]) -> bool {
    buf.starts_with(&MAGIC)
}

pub fn parse_audio_packet(buf: &[u8]) -> Result<AudioPacket<'_>, ProtocolError> {
    if buf.len() < HEADER_LEN {
        return Err(ProtocolError::Truncated);
    }
    if buf[..4] != MAGIC {
        return Err(ProtocolError::BadMagic);
    }
    if buf[4] != 1 {
        return Err(ProtocolError::BadVersion(buf[4]));
    }
    let kind = PacketKind::from_u8(buf[5])?;
    if kind != PacketKind::Audio {
        return Err(ProtocolError::BadKind(buf[5]));
    }
    let header_len = buf[7] as usize;
    if header_len != HEADER_LEN || buf.len() < header_len {
        return Err(ProtocolError::BadHeaderLen(buf[7]));
    }
    let session = u32::from_be_bytes(buf[8..12].try_into().unwrap());
    let seq = u32::from_be_bytes(buf[12..16].try_into().unwrap());
    let timestamp = u32::from_be_bytes(buf[16..20].try_into().unwrap());
    let current_len = u16::from_be_bytes(buf[20..22].try_into().unwrap()) as usize;
    let previous_len = u16::from_be_bytes(buf[22..24].try_into().unwrap()) as usize;
    let bitrate_kbps = u16::from_be_bytes(buf[24..26].try_into().unwrap());
    let total = header_len
        .checked_add(current_len)
        .and_then(|n| n.checked_add(previous_len))
        .filter(|n| *n <= buf.len())
        .ok_or(ProtocolError::Truncated)?;
    if current_len == 0 {
        return Err(ProtocolError::BadFrameLen(0));
    }
    Ok(AudioPacket {
        session,
        seq,
        timestamp,
        bitrate_kbps,
        current: &buf[header_len..header_len + current_len],
        previous: &buf[header_len + current_len..total],
    })
}

pub fn parse_end_session(buf: &[u8]) -> Result<u32, ProtocolError> {
    if buf.len() < HEADER_LEN {
        return Err(ProtocolError::Truncated);
    }
    if buf[..4] != MAGIC {
        return Err(ProtocolError::BadMagic);
    }
    if buf[5] != PacketKind::End as u8 {
        return Err(ProtocolError::BadKind(buf[5]));
    }
    Ok(u32::from_be_bytes(buf[8..12].try_into().unwrap()))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
pub enum Control {
    Announce(Announce),
    Probe {
        id: String,
        #[serde(default)]
        announce: Option<Announce>,
    },
    Subscribe(Subscribe),
    Subscribed(Subscribed),
    Unsubscribe {
        session: u32,
    },
    Ended {
        session: u32,
        reason: String,
    },
    Feedback(Feedback),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Announce {
    pub app: String,
    pub version: u16,
    pub id: String,
    pub alias: String,
    pub hostname: String,
    pub platform: String,
    pub port: u16,
    pub broadcast: Option<BroadcastAnnounce>,
}

impl Announce {
    pub fn new(
        id: impl Into<String>,
        alias: impl Into<String>,
        platform: impl Into<String>,
        broadcast: Option<BroadcastAnnounce>,
    ) -> Self {
        Self {
            app: APP_MARKER.to_owned(),
            version: PROTOCOL_VERSION,
            id: id.into(),
            alias: alias.into(),
            hostname: hostname::get()
                .ok()
                .and_then(|h| h.into_string().ok())
                .unwrap_or_else(|| "unknown".to_owned()),
            platform: platform.into(),
            port: DEFAULT_PORT,
            broadcast,
        }
    }

    pub fn is_compatible(&self) -> bool {
        self.app == APP_MARKER && self.version == PROTOCOL_VERSION
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BroadcastAnnounce {
    pub session: u32,
    pub source: String,
    pub source_kind: String,
    pub started_at_ms: u64,
    pub listeners: u32,
    pub codec: CodecAnnounce,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodecAnnounce {
    pub name: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub frame_ms: u16,
}

impl Default for CodecAnnounce {
    fn default() -> Self {
        Self {
            name: "opus".to_owned(),
            sample_rate: SAMPLE_RATE,
            channels: CHANNELS as u16,
            frame_ms: FRAME_MS as u16,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Subscribe {
    pub id: String,
    pub alias: String,
    pub platform: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Subscribed {
    pub session: u32,
    pub source_id: String,
    pub source_alias: String,
    pub source: String,
    pub source_kind: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub frame_ms: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Feedback {
    pub session: u32,
    pub received: u64,
    pub lost: u64,
    pub jitter_us: u64,
    pub buffer_ms: u32,
    pub concealment: u64,
    /// The receiver's clean-network estimate, used to lift fidelity only when
    /// the path has enough headroom for the extra bits.
    pub stable: bool,
}

#[derive(Debug, Clone)]
pub struct Peer {
    pub id: String,
    pub alias: String,
    pub platform: String,
    pub address: SocketAddr,
    pub port: u16,
    pub hostname: String,
    pub broadcast: Option<BroadcastAnnounce>,
    pub last_seen: std::time::Instant,
}

impl Peer {
    pub fn from_announce(announce: &Announce, address: SocketAddr) -> Self {
        Self {
            id: announce.id.clone(),
            alias: announce.alias.clone(),
            platform: announce.platform.clone(),
            address,
            port: announce.port,
            hostname: announce.hostname.clone(),
            broadcast: announce.broadcast.clone(),
            last_seen: std::time::Instant::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_round_trip_keeps_both_frames() {
        let current = [7u8; 91];
        let previous = [3u8; 88];
        let buf = build_audio_packet(42, 17, 34_560, 128, &current, &previous);
        let packet = parse_audio_packet(&buf).unwrap();
        assert_eq!(packet.session, 42);
        assert_eq!(packet.seq, 17);
        assert_eq!(packet.timestamp, 34_560);
        assert_eq!(packet.bitrate_kbps, 128);
        assert_eq!(packet.current, current);
        assert_eq!(packet.previous, previous);
    }

    #[test]
    fn malformed_lengths_are_rejected() {
        let mut buf = build_audio_packet(1, 2, 3, 96, &[1, 2, 3], &[]);
        buf[20..22].copy_from_slice(&999u16.to_be_bytes());
        assert_eq!(parse_audio_packet(&buf), Err(ProtocolError::Truncated));
    }

    #[test]
    fn control_messages_are_compact_and_typed() {
        let control = Control::Probe {
            id: "device-a".to_owned(),
            announce: None,
        };
        let json = serde_json::to_string(&control).unwrap();
        assert!(json.contains("\"type\":\"probe\""));
        let decoded: Control = serde_json::from_str(&json).unwrap();
        assert!(matches!(decoded, Control::Probe { .. }));
    }

    #[test]
    fn an_old_probe_without_announce_still_parses() {
        let probe: Control =
            serde_json::from_str(r#"{"type":"probe","data":{"id":"old"}}"#).unwrap();
        assert!(matches!(probe, Control::Probe { announce: None, .. }));
    }

    #[test]
    fn end_packets_do_not_parse_as_audio() {
        let buf = build_end_packet(9);
        assert_eq!(parse_end_session(&buf).unwrap(), 9);
        assert!(parse_audio_packet(&buf).is_err());
    }
}
