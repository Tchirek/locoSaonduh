//! The session coordinator.
//!
//! One thread owns discovery, the broadcast/listener state and the UDP socket.
//! Audio callbacks stay on their own realtime threads; this thread only moves
//! small packets and UI snapshots. That separation is what keeps a busy LAN
//! from ever blocking the speaker.

use std::collections::HashMap;
use std::net::{SocketAddr, UdpSocket};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow};

use crate::adaptive::{FRAME_DURATION, QualityController, QualityTier, StreamBuffer};
use crate::audio::{
    AudioSource, Decoder, Encoder, OpenCapture, OutputDevice, PlaybackSink, SourceKind,
    list_output_devices, list_sources, open_capture,
};
use crate::net::{LocalNetwork, bind_discovery_socket, local_network, send_discovery};
use crate::protocol::{
    Announce, BroadcastAnnounce, CHANNELS, CodecAnnounce, Control, FRAME_SAMPLES, Feedback,
    MAX_CONTROL_BYTES, Peer, Subscribe, Subscribed, build_audio_packet, is_audio_datagram,
    parse_audio_packet,
};
use crate::settings::Settings;
use crate::state::{
    BroadcastOwnView, BroadcastView, IdentityView, ListenerView, ListeningView, NetworkView,
    NoticeView, PeerView, SendQualityView, Snapshot,
};

const ANNOUNCE_EVERY: Duration = Duration::from_secs(2);
const FEEDBACK_EVERY: Duration = Duration::from_millis(500);
const SNAPSHOT_EVERY: Duration = Duration::from_millis(180);
const PEER_TTL: Duration = Duration::from_secs(7);
const PENDING_TTL: Duration = Duration::from_millis(2_500);
const LISTENER_TTL: Duration = Duration::from_secs(30);
const MAX_LISTENERS: usize = 24;

type Reply = Sender<Result<(), String>>;

#[derive(Clone)]
pub struct Service {
    commands: Sender<Command>,
    shared: Arc<Shared>,
}

struct Shared {
    snapshot: Mutex<Snapshot>,
}

enum Command {
    StartBroadcast {
        source_id: String,
        reply: Reply,
    },
    StopBroadcast {
        reply: Reply,
    },
    Subscribe {
        peer_id: String,
        reply: Reply,
    },
    Unsubscribe {
        sender_id: String,
        reply: Reply,
    },
    SetAlias {
        alias: String,
        reply: Reply,
    },
    SetSource {
        source_id: String,
        reply: Reply,
    },
    SetOutput {
        output_id: Option<String>,
        reply: Reply,
    },
    RefreshDevices {
        reply: Reply,
    },
    Probe {
        reply: Reply,
    },
    Shutdown,
}

impl Service {
    pub fn start(settings_dir: PathBuf, port: u16) -> Result<Self> {
        let mut settings = Settings::load_or_create(&settings_dir);
        let sources = list_sources();
        let outputs = list_output_devices();
        let mut changed = false;

        if !sources
            .iter()
            .any(|source| Some(&source.id) == settings.source_id.as_ref())
        {
            let fallback = sources
                .iter()
                .find(|source| source.is_default && source.kind == SourceKind::System)
                .or_else(|| sources.iter().find(|source| source.is_default))
                .or_else(|| sources.first());
            let fallback = fallback.map(|source| source.id.clone());
            if settings.source_id != fallback {
                settings.source_id = fallback;
                changed = true;
            }
        }
        if settings
            .output_id
            .as_ref()
            .is_some_and(|id| !outputs.iter().any(|device| &device.id == id))
        {
            settings.output_id = None;
            changed = true;
        }
        if changed && let Err(error) = settings.save(&settings_dir) {
            log::warn!("could not save normalised settings: {error}");
        }

        let socket = bind_discovery_socket(port)?;
        let local = local_network();
        let hostname = hostname::get()
            .ok()
            .and_then(|name| name.into_string().ok())
            .unwrap_or_else(|| "unknown".to_owned());
        let notice = if sources.is_empty() {
            Some(NoticeView {
                level: "warning".to_owned(),
                message: "没有找到可采集的音频设备；仍可收听其他设备。".to_owned(),
            })
        } else if local.ip.is_none() {
            Some(NoticeView {
                level: "warning".to_owned(),
                message: "没有检测到局域网地址；请检查 Wi-Fi 或网线。".to_owned(),
            })
        } else {
            None
        };
        let snapshot = Snapshot {
            revision: 1,
            identity: IdentityView {
                id: settings.id.clone(),
                alias: settings.alias.clone(),
                hostname,
                platform: std::env::consts::OS.to_owned(),
            },
            network: NetworkView {
                ip: local.ip.clone(),
                interface: local.interface.clone(),
                port,
                discovery: "UDP 组播 + 广播".to_owned(),
            },
            sources: sources.clone(),
            outputs: outputs.clone(),
            selected_source: settings.source_id.clone(),
            selected_output: settings.output_id.clone(),
            notice,
            ..Snapshot::default()
        };
        let shared = Arc::new(Shared {
            snapshot: Mutex::new(snapshot),
        });
        let (commands, command_rx) = mpsc::channel();

        let coordinator = Coordinator {
            settings_dir,
            settings,
            port,
            socket,
            shared: Arc::clone(&shared),
            peers: HashMap::new(),
            broadcast: None,
            receiver: None,
            pending: HashMap::new(),
            quality: QualityController::new(),
            sources,
            outputs,
            local,
            revision: 1,
            notice: None,
            next_announce: Instant::now(),
            next_feedback: Instant::now() + FEEDBACK_EVERY,
            next_snapshot: Instant::now() + SNAPSHOT_EVERY,
            next_network_refresh: Instant::now() + Duration::from_secs(5),
            command_rx,
        };
        thread::Builder::new()
            .name("loco-session".to_owned())
            .spawn(move || coordinator.run())
            .context("could not start network coordinator")?;

        Ok(Self { commands, shared })
    }

    pub fn snapshot(&self) -> Snapshot {
        self.shared.snapshot.lock().unwrap().clone()
    }

    pub fn start_broadcast(&self, source_id: impl Into<String>) -> Result<(), String> {
        self.call(
            |reply| Command::StartBroadcast {
                source_id: source_id.into(),
                reply,
            },
            Duration::from_secs(5),
        )
    }

    pub fn stop_broadcast(&self) -> Result<(), String> {
        self.call(
            |reply| Command::StopBroadcast { reply },
            Duration::from_secs(3),
        )
    }

    pub fn subscribe(&self, peer_id: impl Into<String>) -> Result<(), String> {
        self.call(
            |reply| Command::Subscribe {
                peer_id: peer_id.into(),
                reply,
            },
            Duration::from_secs(4),
        )
    }

    pub fn unsubscribe(&self, sender_id: impl Into<String>) -> Result<(), String> {
        self.call(
            |reply| Command::Unsubscribe {
                sender_id: sender_id.into(),
                reply,
            },
            Duration::from_secs(2),
        )
    }

    pub fn set_alias(&self, alias: impl Into<String>) -> Result<(), String> {
        self.call(
            |reply| Command::SetAlias {
                alias: alias.into(),
                reply,
            },
            Duration::from_secs(2),
        )
    }

    pub fn set_source(&self, source_id: impl Into<String>) -> Result<(), String> {
        self.call(
            |reply| Command::SetSource {
                source_id: source_id.into(),
                reply,
            },
            Duration::from_secs(2),
        )
    }

    pub fn set_output(&self, output_id: Option<String>) -> Result<(), String> {
        self.call(
            |reply| Command::SetOutput { output_id, reply },
            Duration::from_secs(2),
        )
    }

    pub fn refresh_devices(&self) -> Result<(), String> {
        self.call(
            |reply| Command::RefreshDevices { reply },
            Duration::from_secs(2),
        )
    }

    pub fn probe(&self) -> Result<(), String> {
        self.call(|reply| Command::Probe { reply }, Duration::from_secs(1))
    }

    pub fn shutdown(&self) {
        let _ = self.commands.send(Command::Shutdown);
    }

    fn call<F>(&self, factory: F, timeout: Duration) -> Result<(), String>
    where
        F: FnOnce(Reply) -> Command,
    {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.commands
            .send(factory(reply_tx))
            .map_err(|_| "音频服务已停止".to_owned())?;
        reply_rx
            .recv_timeout(timeout)
            .map_err(|_| "操作超时，请检查局域网连接".to_owned())?
    }
}

struct Coordinator {
    settings_dir: PathBuf,
    settings: Settings,
    port: u16,
    socket: Arc<UdpSocket>,
    shared: Arc<Shared>,
    peers: HashMap<String, Peer>,
    broadcast: Option<BroadcastRuntime>,
    receiver: Option<ReceiverRuntime>,
    pending: HashMap<String, PendingSub>,
    quality: QualityController,
    sources: Vec<AudioSource>,
    outputs: Vec<OutputDevice>,
    local: LocalNetwork,
    revision: u64,
    notice: Option<LiveNotice>,
    next_announce: Instant,
    next_feedback: Instant,
    next_snapshot: Instant,
    next_network_refresh: Instant,
    command_rx: Receiver<Command>,
}

struct LiveNotice {
    level: String,
    message: String,
    until: Instant,
}

struct PendingSub {
    peer_id: String,
    deadline: Instant,
    reply: Reply,
}

struct BroadcastRuntime {
    session: u32,
    started_at_ms: u64,
    source: AudioSource,
    _capture_stream: cpal::Stream,
    capture_error: Arc<Mutex<Option<String>>>,
    capture_peak: Arc<AtomicU32>,
    overflowed_samples: Arc<AtomicU64>,
    listeners: Arc<Mutex<Vec<ListenerEntry>>>,
    control: Arc<SenderControl>,
    sender: Option<JoinHandle<()>>,
}

struct SenderControl {
    stop: AtomicBool,
    bitrate_kbps: AtomicU32,
    loss_x100: AtomicU32,
    packets_sent: AtomicU64,
    send_errors: AtomicU64,
}

impl SenderControl {
    fn new() -> Self {
        Self {
            stop: AtomicBool::new(false),
            bitrate_kbps: AtomicU32::new(112),
            loss_x100: AtomicU32::new(0),
            packets_sent: AtomicU64::new(0),
            send_errors: AtomicU64::new(0),
        }
    }
}

#[derive(Clone)]
struct ListenerEntry {
    id: String,
    alias: String,
    address: SocketAddr,
    joined_at: Instant,
    last_seen: Instant,
    report: Option<Feedback>,
}

struct ReceiverRuntime {
    _playback_stream: cpal::Stream,
    playback: crate::audio::PlaybackInfo,
    _peak: Arc<AtomicU32>,
    _underruns: Arc<AtomicU64>,
    error: Arc<Mutex<Option<String>>>,
    streams: Arc<Mutex<HashMap<String, Arc<Mutex<ReceiverStream>>>>>,
    stop: Arc<AtomicBool>,
    mixer: Option<JoinHandle<()>>,
}

struct ReceiverStream {
    sender_id: String,
    sender_alias: String,
    source: String,
    source_kind: String,
    session: u32,
    address: SocketAddr,
    buffer: StreamBuffer,
    decoder: Decoder,
    bitrate_kbps: u32,
    last_packet: Instant,
    level: f32,
}

impl Coordinator {
    fn run(mut self) {
        log::info!(
            "session coordinator online on UDP {} ({:?})",
            self.port,
            self.local
        );
        self.probe_network();
        loop {
            if let Err(error) = self.drain_socket() {
                log::debug!("socket read: {error}");
            }
            while let Ok(command) = self.command_rx.try_recv() {
                if !self.handle_command(command) {
                    self.shutdown();
                    return;
                }
            }

            let now = Instant::now();
            if now >= self.next_announce {
                self.announce();
                self.next_announce = now + ANNOUNCE_EVERY;
            }
            if now >= self.next_feedback {
                self.send_feedback();
                self.next_feedback = now + FEEDBACK_EVERY;
            }
            if now >= self.next_network_refresh {
                self.local = local_network();
                self.next_network_refresh = now + Duration::from_secs(5);
            }
            self.expire_pending(now);
            self.prune_peers(now);
            self.reap_stale_listeners(now);
            self.check_health();
            if now >= self.next_snapshot {
                self.publish();
                self.next_snapshot = now + SNAPSHOT_EVERY;
            }
            // Non-blocking intake plus a short yield keeps a 100 Hz audio flow
            // from monopolising the coordinator while keeping command latency
            // below a frame.
            thread::sleep(Duration::from_millis(2));
        }
    }

    fn shutdown(&mut self) {
        self.stop_broadcast(Some("本机已停止广播".to_owned()), false);
        self.stop_receiver();
        self.publish();
        log::info!("session coordinator stopped");
    }

    fn handle_command(&mut self, command: Command) -> bool {
        match command {
            Command::StartBroadcast { source_id, reply } => {
                let result = self
                    .start_broadcast(&source_id)
                    .map_err(|error| format!("{error:#}"));
                if let Err(message) = &result {
                    self.set_notice("error", message.clone());
                    self.publish();
                }
                let _ = reply.send(result);
            }
            Command::StopBroadcast { reply } => {
                self.stop_broadcast(Some("本机已停止广播".to_owned()), true);
                let _ = reply.send(Ok(()));
            }
            Command::Subscribe { peer_id, reply } => {
                let result = self.begin_subscribe(&peer_id, reply);
                if let Err(message) = &result {
                    self.set_notice("error", message.clone());
                    self.publish();
                }
            }
            Command::Unsubscribe { sender_id, reply } => {
                self.unsubscribe(&sender_id);
                let _ = reply.send(Ok(()));
            }
            Command::SetAlias { alias, reply } => {
                let alias = alias.trim();
                let result = if alias.is_empty() {
                    Err("名字不能为空".to_owned())
                } else if alias.chars().count() > 32 {
                    Err("名字请控制在 32 个字符以内".to_owned())
                } else {
                    self.settings.alias = alias.to_owned();
                    let persisted = self
                        .settings
                        .save(&self.settings_dir)
                        .map_err(|error| format!("{error:#}"));
                    self.announce_now();
                    persisted
                };
                if let Err(message) = &result {
                    self.set_notice("error", message.clone());
                }
                let _ = reply.send(result);
            }
            Command::SetSource { source_id, reply } => {
                let result = if self.broadcast.is_some() {
                    Err("广播中不能切换音源".to_owned())
                } else if !self.sources.iter().any(|source| source.id == source_id) {
                    Err("这个音源已经不可用".to_owned())
                } else {
                    self.settings.source_id = Some(source_id);
                    self.settings
                        .save(&self.settings_dir)
                        .map_err(|error| format!("{error:#}"))
                };
                if let Err(message) = &result {
                    self.set_notice("error", message.clone());
                }
                let _ = reply.send(result);
            }
            Command::SetOutput { output_id, reply } => {
                let result = if self.receiver.is_some() {
                    Err("收听中不能切换输出设备".to_owned())
                } else if output_id
                    .as_ref()
                    .is_some_and(|id| !self.outputs.iter().any(|device| &device.id == id))
                {
                    Err("这个输出设备已经不可用".to_owned())
                } else {
                    self.settings.output_id = output_id;
                    self.settings
                        .save(&self.settings_dir)
                        .map_err(|error| format!("{error:#}"))
                };
                if let Err(message) = &result {
                    self.set_notice("error", message.clone());
                }
                let _ = reply.send(result);
            }
            Command::RefreshDevices { reply } => {
                self.refresh_devices();
                let _ = reply.send(Ok(()));
            }
            Command::Probe { reply } => {
                self.probe_network();
                self.announce();
                let _ = reply.send(Ok(()));
            }
            Command::Shutdown => return false,
        }
        true
    }

    fn start_broadcast(&mut self, source_id: &str) -> Result<()> {
        if self.broadcast.is_some() {
            return Err(anyhow!("已经在广播"));
        }
        if self.receiver.is_some() || !self.pending.is_empty() {
            return Err(anyhow!("请先停止收听，再开始广播"));
        }
        let source = self
            .sources
            .iter()
            .find(|source| source.id == source_id)
            .cloned()
            .ok_or_else(|| anyhow!("音源不存在，请重新选择"))?;

        let OpenCapture {
            stream,
            feed,
            source: opened_source,
            sample_rate,
            channels,
            peak,
            overflowed_samples,
            error,
        } = open_capture(&source)?;
        if let Some(message) = error.lock().unwrap().take() {
            return Err(anyhow!(message));
        }

        let session = random_session();
        let started_at_ms = epoch_ms();
        let listeners = Arc::new(Mutex::new(Vec::<ListenerEntry>::new()));
        let control = Arc::new(SenderControl::new());
        let encoder = Encoder::new().context("could not initialise Opus encoder")?;
        let socket = Arc::clone(&self.socket);
        let listener_registry = Arc::clone(&listeners);
        let worker_control = Arc::clone(&control);
        let sender = thread::Builder::new()
            .name("loco-encoder".to_owned())
            .spawn(move || {
                sender_loop(SenderJob {
                    feed,
                    sample_rate,
                    channels,
                    session,
                    socket,
                    listeners: listener_registry,
                    control: worker_control,
                    encoder,
                });
            })
            .context("could not start audio sender")?;

        self.broadcast = Some(BroadcastRuntime {
            session,
            started_at_ms,
            source: opened_source.clone(),
            _capture_stream: stream,
            capture_error: error,
            capture_peak: peak,
            overflowed_samples,
            listeners,
            control,
            sender: Some(sender),
        });
        self.settings.source_id = Some(source_id.to_owned());
        if let Err(error) = self.settings.save(&self.settings_dir) {
            log::warn!("could not save source choice: {error}");
        }
        self.set_notice("info", format!("正在广播「{}」", opened_source.name));
        self.announce_now();
        self.publish();
        Ok(())
    }

    fn stop_broadcast(&mut self, reason: Option<String>, announce: bool) {
        let Some(mut runtime) = self.broadcast.take() else {
            return;
        };
        let ended = Control::Ended {
            session: runtime.session,
            reason: reason.clone().unwrap_or_else(|| "广播结束".to_owned()),
        };
        if let Ok(listeners) = runtime.listeners.lock() {
            for listener in listeners.iter() {
                self.send_control(listener.address, &ended);
            }
        }
        runtime.control.stop.store(true, Ordering::Release);
        drop(runtime._capture_stream);
        if let Some(sender) = runtime.sender.take() {
            let _ = sender.join();
        }
        self.quality = QualityController::new();
        if announce {
            self.announce_now();
        }
        if let Some(reason) = reason
            && reason != "本机已停止广播"
        {
            self.set_notice("error", reason);
        }
        self.publish();
    }

    fn begin_subscribe(&mut self, peer_id: &str, reply: Reply) -> Result<(), String> {
        if self
            .receiver
            .as_ref()
            .is_some_and(|runtime| runtime.streams.lock().unwrap().contains_key(peer_id))
        {
            let _ = reply.send(Ok(()));
            return Ok(());
        }
        let outcome: Result<SocketAddr, String> =
            (|| {
                if self.broadcast.is_some() {
                    return Err("广播中不能同时收听".to_owned());
                }
                if self.pending.contains_key(peer_id) {
                    return Err("正在连接这台设备".to_owned());
                }
                if self.receiver.as_ref().is_some_and(|runtime| {
                    runtime.streams.lock().map(|s| s.len()).unwrap_or(0) >= 8
                }) {
                    return Err("一次最多收听 8 路音频".to_owned());
                }
                let peer = self
                    .peers
                    .get(peer_id)
                    .ok_or_else(|| "设备已离线".to_owned())?;
                if peer.broadcast.is_none() {
                    return Err("这台设备当前没有广播".to_owned());
                }
                Ok(SocketAddr::new(peer.address.ip(), peer.port))
            })();
        let address = match outcome {
            Ok(address) => address,
            Err(message) => {
                let _ = reply.send(Err(message.clone()));
                return Err(message);
            }
        };
        if let Err(message) = self.ensure_receiver() {
            let _ = reply.send(Err(message.clone()));
            return Err(message);
        }
        let control = Control::Subscribe(Subscribe {
            id: self.settings.id.clone(),
            alias: self.settings.alias.clone(),
            platform: std::env::consts::OS.to_owned(),
        });
        self.send_control(address, &control);
        self.pending.insert(
            peer_id.to_owned(),
            PendingSub {
                peer_id: peer_id.to_owned(),
                deadline: Instant::now() + PENDING_TTL,
                reply,
            },
        );
        self.publish();
        Ok(())
    }

    fn unsubscribe(&mut self, sender_id: &str) {
        let Some(runtime) = self.receiver.as_ref() else {
            return;
        };
        let removed = runtime.streams.lock().unwrap().remove(sender_id);
        if let Some(stream) = removed {
            let stream = stream.lock().unwrap();
            self.send_control(
                stream.address,
                &Control::Unsubscribe {
                    session: stream.session,
                },
            );
        }
        let empty = runtime
            .streams
            .lock()
            .map(|streams| streams.is_empty())
            .unwrap_or(true);
        if empty {
            self.stop_receiver();
        }
        self.publish();
    }

    fn stop_receiver(&mut self) {
        if let Some(mut runtime) = self.receiver.take() {
            runtime.stop.store(true, Ordering::Release);
            if let Some(mixer) = runtime.mixer.take() {
                let _ = mixer.join();
            }
        }
        self.publish();
    }

    fn ensure_receiver(&mut self) -> Result<(), String> {
        if self.receiver.is_some() {
            return Ok(());
        }
        let PlaybackSink {
            stream,
            feed,
            info,
            peak,
            underrun_samples,
            error,
        } = PlaybackSink::open(self.settings.output_id.as_deref())
            .map_err(|error| format!("输出设备无法打开：{error}. 请在右上角设置里换一个输出。"))?;
        let streams = Arc::new(Mutex::new(HashMap::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let mixer = {
            let streams = Arc::clone(&streams);
            let stop = Arc::clone(&stop);
            thread::Builder::new()
                .name("loco-mixer".to_owned())
                .spawn(move || mixer_loop(streams, feed, stop))
                .map_err(|error| format!("混音线程无法启动：{error}"))?
        };
        self.receiver = Some(ReceiverRuntime {
            _playback_stream: stream,
            playback: info,
            _peak: peak,
            _underruns: underrun_samples,
            error,
            streams,
            stop,
            mixer: Some(mixer),
        });
        Ok(())
    }

    fn handle_subscribed(&mut self, subscribed: Subscribed, from: SocketAddr) {
        let Some(pending) = self.pending.remove(&subscribed.source_id) else {
            // A late acknowledgement after timeout, or a spoofed reply: there
            // is no listener intent to attach it to.
            log::debug!("unsolicited subscribed from {from}");
            return;
        };
        let Some(runtime) = self.receiver.as_ref() else {
            let _ = pending.reply.send(Err("输出设备已停止".to_owned()));
            return;
        };
        let now = Instant::now();
        match Decoder::new() {
            Ok(decoder) => {
                let stream = ReceiverStream {
                    sender_id: pending.peer_id.clone(),
                    sender_alias: subscribed.source_alias,
                    source: subscribed.source,
                    source_kind: subscribed.source_kind,
                    session: subscribed.session,
                    address: from,
                    buffer: StreamBuffer::new(now),
                    decoder,
                    bitrate_kbps: 112,
                    last_packet: now,
                    level: 0.0,
                };
                runtime
                    .streams
                    .lock()
                    .unwrap()
                    .insert(pending.peer_id.clone(), Arc::new(Mutex::new(stream)));
                let _ = pending.reply.send(Ok(()));
            }
            Err(error) => {
                let message = format!("Opus 解码器初始化失败：{error}");
                let _ = pending.reply.send(Err(message.clone()));
                self.set_notice("error", message);
            }
        }
        self.publish();
    }

    fn drain_socket(&mut self) -> Result<()> {
        let mut buffer = [0u8; 2_048];
        // A bounded batch means a flooded socket cannot starve timers either.
        for _ in 0..96 {
            match self.socket.recv_from(&mut buffer) {
                Ok((len, from)) => {
                    let payload = &buffer[..len];
                    if is_audio_datagram(payload) {
                        self.handle_audio(payload, from);
                    } else if payload.len() <= MAX_CONTROL_BYTES {
                        match serde_json::from_slice::<Control>(payload) {
                            Ok(control) => self.handle_control(control, from),
                            Err(error) => log::debug!("unreadable control from {from}: {error}"),
                        }
                    }
                }
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        || error.kind() == std::io::ErrorKind::TimedOut =>
                {
                    break;
                }
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }

    fn handle_audio(&mut self, payload: &[u8], from: SocketAddr) {
        let Ok(packet) = parse_audio_packet(payload) else {
            return;
        };
        let Some(runtime) = self.receiver.as_ref() else {
            return;
        };
        let target = {
            let streams = runtime.streams.lock().unwrap();
            streams
                .iter()
                .find(|(_, stream)| {
                    let stream = stream.lock().unwrap();
                    stream.session == packet.session && stream.address.ip() == from.ip()
                })
                .map(|(id, stream)| (id.clone(), Arc::clone(stream)))
        };
        let Some((_, stream)) = target else {
            return;
        };
        let mut stream = stream.lock().unwrap();
        stream
            .buffer
            .insert_pair(packet.seq, packet.current, packet.previous, Instant::now());
        stream.bitrate_kbps = packet.bitrate_kbps as u32;
        stream.last_packet = Instant::now();
    }

    fn handle_control(&mut self, control: Control, from: SocketAddr) {
        match control {
            Control::Announce(announce) => {
                if !announce.is_compatible() || announce.id == self.settings.id {
                    return;
                }
                let address = SocketAddr::new(from.ip(), announce.port);
                let peer = Peer::from_announce(&announce, address);
                self.peers.insert(announce.id.clone(), peer);
                self.publish_soon();
            }
            Control::Probe { id } => {
                if id == self.settings.id {
                    return;
                }
                self.send_announce_to(SocketAddr::new(from.ip(), from.port()));
            }
            Control::Subscribe(subscribe) => self.accept_subscriber(subscribe, from),
            Control::Subscribed(subscribed) => self.handle_subscribed(subscribed, from),
            Control::Unsubscribe { session } => {
                if let Some(runtime) = self.broadcast.as_ref()
                    && runtime.session == session
                {
                    runtime
                        .listeners
                        .lock()
                        .unwrap()
                        .retain(|listener| listener.address.ip() != from.ip());
                    self.announce_now();
                }
            }
            Control::Ended { session, reason } => {
                let key = self.receiver.as_ref().and_then(|runtime| {
                    runtime
                        .streams
                        .lock()
                        .unwrap()
                        .iter()
                        .find(|(_, stream)| {
                            let stream = stream.lock().unwrap();
                            stream.session == session && stream.address.ip() == from.ip()
                        })
                        .map(|(id, _)| id.clone())
                });
                if let Some(key) = key {
                    let removed = self
                        .receiver
                        .as_ref()
                        .and_then(|runtime| runtime.streams.lock().unwrap().remove(&key));
                    if let Some(stream) = removed {
                        let alias = stream.lock().unwrap().sender_alias.clone();
                        self.set_notice("info", format!("{alias} 已停止广播：{reason}"));
                    }
                    if self
                        .receiver
                        .as_ref()
                        .is_some_and(|runtime| runtime.streams.lock().unwrap().is_empty())
                    {
                        self.stop_receiver();
                    }
                }
            }
            Control::Feedback(feedback) => self.handle_feedback(feedback, from),
        }
    }

    fn accept_subscriber(&mut self, subscribe: Subscribe, from: SocketAddr) {
        let Some(runtime) = self.broadcast.as_ref() else {
            self.send_control(
                from,
                &Control::Ended {
                    session: 0,
                    reason: "这台设备没有在广播".to_owned(),
                },
            );
            return;
        };
        let mut listeners = runtime.listeners.lock().unwrap();
        if !listeners
            .iter()
            .any(|listener| listener.address.ip() == from.ip())
        {
            if listeners.len() >= MAX_LISTENERS {
                drop(listeners);
                self.send_control(
                    from,
                    &Control::Ended {
                        session: 0,
                        reason: "收听人数已满".to_owned(),
                    },
                );
                return;
            }
            listeners.push(ListenerEntry {
                id: subscribe.id,
                alias: subscribe.alias,
                address: SocketAddr::new(from.ip(), from.port()),
                joined_at: Instant::now(),
                last_seen: Instant::now(),
                report: None,
            });
        } else if let Some(listener) = listeners
            .iter_mut()
            .find(|listener| listener.address.ip() == from.ip())
        {
            listener.alias = subscribe.alias;
            listener.id = subscribe.id;
            listener.last_seen = Instant::now();
        }
        drop(listeners);

        let response = Control::Subscribed(Subscribed {
            session: runtime.session,
            source_id: self.settings.id.clone(),
            source_alias: self.settings.alias.clone(),
            source: runtime.source.name.clone(),
            source_kind: source_kind_label(runtime.source.kind).to_owned(),
            sample_rate: crate::protocol::SAMPLE_RATE,
            channels: CHANNELS as u16,
            frame_ms: crate::protocol::FRAME_MS as u16,
        });
        self.send_control(SocketAddr::new(from.ip(), from.port()), &response);
        self.announce_now();
        self.publish_soon();
    }

    fn handle_feedback(&mut self, feedback: Feedback, from: SocketAddr) {
        let Some(runtime) = self.broadcast.as_ref() else {
            return;
        };
        if feedback.session != runtime.session {
            return;
        }
        let listeners_handle = Arc::clone(&runtime.listeners);
        let control = Arc::clone(&runtime.control);
        let reports = {
            let mut listeners = listeners_handle.lock().unwrap();
            let Some(listener) = listeners
                .iter_mut()
                .find(|listener| listener.address.ip() == from.ip())
            else {
                return;
            };
            listener.last_seen = Instant::now();
            listener.report = Some(feedback);
            listeners
                .iter()
                .filter_map(|listener| listener.report.clone())
                .collect::<Vec<_>>()
        };
        let quality = self.quality.observe(&reports);
        control
            .bitrate_kbps
            .store(quality.bitrate_kbps, Ordering::Relaxed);
        control
            .loss_x100
            .store((quality.loss_pct * 100.0) as u32, Ordering::Relaxed);
    }

    fn send_feedback(&mut self) {
        let Some(runtime) = self.receiver.as_ref() else {
            return;
        };
        let reports = runtime
            .streams
            .lock()
            .unwrap()
            .values()
            .map(|stream| {
                let stream = stream.lock().unwrap();
                (stream.address, stream.buffer.feedback(stream.session))
            })
            .collect::<Vec<_>>();
        for (address, feedback) in reports {
            self.send_control(address, &Control::Feedback(feedback));
        }
    }

    fn probe_network(&mut self) {
        let probe = Control::Probe {
            id: self.settings.id.clone(),
        };
        self.broadcast_control(&probe);
        self.announce();
    }

    fn announce(&mut self) {
        let announce = self.build_announce();
        if let Ok(json) = serde_json::to_vec(&Control::Announce(announce)) {
            send_discovery(&self.socket, &json, self.port);
        }
    }

    fn send_announce_to(&self, address: SocketAddr) {
        let announce = self.build_announce();
        self.send_control(address, &Control::Announce(announce));
    }

    fn broadcast_control(&self, control: &Control) {
        if let Ok(json) = serde_json::to_vec(control) {
            send_discovery(&self.socket, &json, self.port);
        }
    }

    fn build_announce(&self) -> Announce {
        let hostname = hostname::get()
            .ok()
            .and_then(|name| name.into_string().ok())
            .unwrap_or_else(|| "unknown".to_owned());
        let broadcast = self.broadcast.as_ref().map(|runtime| BroadcastAnnounce {
            session: runtime.session,
            source: runtime.source.name.clone(),
            source_kind: source_kind_label(runtime.source.kind).to_owned(),
            started_at_ms: runtime.started_at_ms,
            listeners: runtime
                .listeners
                .lock()
                .map(|listeners| listeners.len() as u32)
                .unwrap_or(0),
            codec: CodecAnnounce::default(),
        });
        let mut announce = Announce::new(
            self.settings.id.clone(),
            self.settings.alias.clone(),
            std::env::consts::OS,
            broadcast,
        );
        announce.hostname = hostname;
        announce.port = self.port;
        announce
    }

    fn announce_now(&mut self) {
        self.next_announce = Instant::now();
    }

    fn publish_soon(&mut self) {
        self.next_snapshot = Instant::now();
    }

    fn send_control(&self, address: SocketAddr, control: &Control) {
        match serde_json::to_vec(control) {
            Ok(json) if json.len() <= MAX_CONTROL_BYTES => {
                if let Err(error) = self.socket.send_to(&json, address) {
                    log::debug!("control send to {address} failed: {error}");
                }
            }
            Ok(json) => log::warn!("control datagram too large: {} bytes", json.len()),
            Err(error) => log::warn!("could not encode control: {error}"),
        }
    }

    fn expire_pending(&mut self, now: Instant) {
        let expired = self
            .pending
            .iter()
            .filter(|(_, pending)| pending.deadline <= now)
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        for id in expired {
            if let Some(pending) = self.pending.remove(&id) {
                let _ = pending
                    .reply
                    .send(Err("对方没有回应，请确认两台设备在同一局域网".to_owned()));
            }
        }
        let unused_receiver = self.pending.is_empty()
            && self
                .receiver
                .as_ref()
                .is_some_and(|runtime| runtime.streams.lock().unwrap().is_empty());
        if unused_receiver && self.broadcast.is_none() {
            self.stop_receiver();
        }
    }

    fn prune_peers(&mut self, now: Instant) {
        self.peers
            .retain(|_, peer| now.saturating_duration_since(peer.last_seen) < PEER_TTL);
        let _ = now;
    }

    fn reap_stale_listeners(&mut self, now: Instant) {
        let mut changed = false;
        if let Some(runtime) = self.broadcast.as_ref()
            && let Ok(mut listeners) = runtime.listeners.lock()
        {
            let before = listeners.len();
            listeners.retain(|listener| {
                now.saturating_duration_since(listener.last_seen) < LISTENER_TTL
            });
            changed = listeners.len() != before;
        }
        if changed {
            self.announce_now();
        }
    }

    fn check_health(&mut self) {
        let capture_error = self.broadcast.as_ref().and_then(|runtime| {
            runtime
                .capture_error
                .lock()
                .ok()
                .and_then(|mut slot| slot.take())
        });
        if let Some(error) = capture_error {
            self.set_notice("error", format!("采集已中断：{error}"));
            self.stop_broadcast(Some("采集设备断开，广播已停止".to_owned()), true);
        }
        let playback_error = self
            .receiver
            .as_ref()
            .and_then(|runtime| runtime.error.lock().ok().and_then(|mut slot| slot.take()));
        if let Some(error) = playback_error {
            self.set_notice("error", format!("播放已中断：{error}"));
            self.stop_receiver();
        }
    }

    fn refresh_devices(&mut self) {
        self.sources = list_sources();
        self.outputs = list_output_devices();
        if !self
            .sources
            .iter()
            .any(|source| Some(&source.id) == self.settings.source_id.as_ref())
        {
            self.settings.source_id = self.sources.first().map(|source| source.id.clone());
        }
        if self
            .settings
            .output_id
            .as_ref()
            .is_some_and(|id| !self.outputs.iter().any(|device| &device.id == id))
        {
            self.settings.output_id = None;
        }
        if let Err(error) = self.settings.save(&self.settings_dir) {
            log::warn!("could not save refreshed device choices: {error}");
        }
        self.publish_soon();
    }

    fn set_notice(&mut self, level: &str, message: String) {
        self.notice = Some(LiveNotice {
            level: level.to_owned(),
            message,
            until: Instant::now() + Duration::from_secs(8),
        });
    }

    fn publish(&mut self) {
        self.revision += 1;
        let snapshot = self.build_snapshot();
        *self.shared.snapshot.lock().unwrap() = snapshot;
    }

    fn build_snapshot(&mut self) -> Snapshot {
        let hostname = hostname::get()
            .ok()
            .and_then(|name| name.into_string().ok())
            .unwrap_or_else(|| "unknown".to_owned());
        let now = Instant::now();
        let mut peers = self
            .peers
            .values()
            .map(|peer| PeerView {
                id: peer.id.clone(),
                alias: peer.alias.clone(),
                hostname: peer.hostname.clone(),
                platform: peer.platform.clone(),
                address: peer.address.ip().to_string(),
                age_ms: now.saturating_duration_since(peer.last_seen).as_millis() as u64,
                is_broadcasting: peer.broadcast.is_some(),
                broadcast: peer.broadcast.as_ref().map(|broadcast| BroadcastView {
                    session: broadcast.session,
                    source: broadcast.source.clone(),
                    source_kind: broadcast.source_kind.clone(),
                    started_at_ms: broadcast.started_at_ms,
                    listeners: broadcast.listeners,
                    codec: broadcast.codec.clone(),
                }),
                is_listening: self
                    .receiver
                    .as_ref()
                    .and_then(|runtime| {
                        runtime
                            .streams
                            .lock()
                            .ok()
                            .map(|streams| streams.contains_key(&peer.id))
                    })
                    .unwrap_or(false),
            })
            .collect::<Vec<_>>();
        peers.sort_by(|left, right| {
            right
                .is_broadcasting
                .cmp(&left.is_broadcasting)
                .then_with(|| left.alias.to_lowercase().cmp(&right.alias.to_lowercase()))
        });

        let broadcast = self.broadcast.as_ref().map(|runtime| {
            let listeners = runtime
                .listeners
                .lock()
                .map(|listeners| {
                    listeners
                        .iter()
                        .map(|listener| {
                            let loss = listener
                                .report
                                .as_ref()
                                .map(|report| {
                                    let total = report.received + report.lost;
                                    if total == 0 {
                                        0.0
                                    } else {
                                        report.lost as f64 / total as f64 * 100.0
                                    }
                                })
                                .unwrap_or(0.0);
                            ListenerView {
                                alias: listener.alias.clone(),
                                id: listener.id.clone(),
                                address: listener.address.ip().to_string(),
                                connected_ms: now
                                    .saturating_duration_since(listener.joined_at)
                                    .as_millis()
                                    as u64,
                                buffer_ms: listener
                                    .report
                                    .as_ref()
                                    .map(|report| report.buffer_ms)
                                    .unwrap_or(0),
                                jitter_ms: listener
                                    .report
                                    .as_ref()
                                    .map(|report| report.jitter_us as f64 / 1_000.0)
                                    .unwrap_or(0.0),
                                loss_pct: loss,
                                reported: listener.report.is_some(),
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();
            let quality = self.quality.state();
            let level = f32::from_bits(runtime.capture_peak.swap(0, Ordering::Relaxed));
            let _ = runtime.overflowed_samples.load(Ordering::Relaxed);
            BroadcastOwnView {
                session: runtime.session,
                source: runtime.source.name.clone(),
                source_kind: source_kind_label(runtime.source.kind).to_owned(),
                started_at_ms: runtime.started_at_ms,
                listeners,
                quality: SendQualityView {
                    mode: "auto".to_owned(),
                    tier: quality_tier_key(quality.tier).to_owned(),
                    tier_label: quality.tier.label().to_owned(),
                    bitrate_kbps: quality.bitrate_kbps,
                    loss_pct: quality.loss_pct,
                    jitter_ms: quality.jitter_ms,
                },
                level,
                packets_sent: runtime.control.packets_sent.load(Ordering::Relaxed),
            }
        });

        let listening = self
            .receiver
            .as_ref()
            .map(|runtime| {
                let output_name = runtime.playback.device_name.clone();
                let streams = runtime
                    .streams
                    .lock()
                    .map(|streams| streams.values().cloned().collect::<Vec<_>>())
                    .unwrap_or_default();
                streams
                    .into_iter()
                    .map(|stream| {
                        let stream = stream.lock().unwrap();
                        let counters = stream.buffer.counters();
                        let silent_for = now.saturating_duration_since(stream.last_packet);
                        let state = if !counters.started || counters.paused {
                            "buffering"
                        } else if silent_for > Duration::from_secs(2) {
                            "waiting"
                        } else {
                            "live"
                        };
                        ListeningView {
                            sender_id: stream.sender_id.clone(),
                            sender_alias: stream.sender_alias.clone(),
                            source: stream.source.clone(),
                            source_kind: stream.source_kind.clone(),
                            state: state.to_owned(),
                            bitrate_kbps: stream.bitrate_kbps,
                            buffer_ms: counters.buffered_ms,
                            target_ms: counters.target_ms,
                            jitter_ms: counters.jitter_ms,
                            loss_pct: counters.loss_ratio() * 100.0,
                            level: stream.level,
                            output: output_name.clone(),
                            output_prebuffer_ms: 20,
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let notice = self.notice.as_ref().and_then(|notice| {
            if notice.until > now {
                Some(NoticeView {
                    level: notice.level.clone(),
                    message: notice.message.clone(),
                })
            } else {
                None
            }
        });
        if self
            .notice
            .as_ref()
            .is_some_and(|notice| notice.until <= now)
        {
            self.notice = None;
        }

        Snapshot {
            revision: self.revision,
            identity: IdentityView {
                id: self.settings.id.clone(),
                alias: self.settings.alias.clone(),
                hostname,
                platform: std::env::consts::OS.to_owned(),
            },
            network: NetworkView {
                ip: self.local.ip.clone(),
                interface: self.local.interface.clone(),
                port: self.port,
                discovery: "UDP 组播 + 广播".to_owned(),
            },
            peers,
            broadcast,
            listening,
            pending: self.pending.keys().cloned().collect(),
            sources: self.sources.clone(),
            outputs: self.outputs.clone(),
            selected_source: self.settings.source_id.clone(),
            selected_output: self.settings.output_id.clone(),
            notice,
        }
    }
}

struct SenderJob {
    feed: rtrb::Consumer<f32>,
    sample_rate: u32,
    channels: usize,
    session: u32,
    socket: Arc<UdpSocket>,
    listeners: Arc<Mutex<Vec<ListenerEntry>>>,
    control: Arc<SenderControl>,
    encoder: Encoder,
}

fn sender_loop(job: SenderJob) {
    let SenderJob {
        sample_rate,
        channels,
        session,
        socket,
        listeners,
        control,
        mut encoder,
        feed,
    } = job;
    let mut adapter = crate::audio::CaptureAdapter::new(channels, sample_rate);
    let mut input = Vec::<f32>::with_capacity(16_384);
    let mut converted = Vec::<f32>::with_capacity(8_192);
    let mut pcm = vec![0.0f32; FRAME_SAMPLES * CHANNELS];
    let mut previous = Vec::<u8>::new();
    let mut current = Vec::<u8>::new();
    let mut seq = 0u32;
    let mut timestamp = 0u32;
    let mut feed = feed;

    while !control.stop.load(Ordering::Acquire) {
        while let Ok(sample) = feed.pop() {
            input.push(sample);
            if input.len() >= 16_384 {
                break;
            }
        }
        if input.is_empty() {
            thread::sleep(Duration::from_millis(2));
            continue;
        }
        adapter.push(&input, &mut converted);
        input.clear();
        let mut consumed = 0usize;
        let frame_samples = pcm.len();
        while converted.len().saturating_sub(consumed) >= frame_samples {
            pcm.copy_from_slice(&converted[consumed..consumed + frame_samples]);
            consumed += frame_samples;
            let bitrate = control.bitrate_kbps.load(Ordering::Relaxed);
            let loss = control.loss_x100.load(Ordering::Relaxed) as f64 / 100.0;
            encoder.set_quality(bitrate, loss);
            current.clear();
            if let Err(error) = encoder.encode(&pcm, &mut current) {
                log::warn!("Opus encode failed: {error}");
                continue;
            }
            let addresses = listeners
                .lock()
                .map(|listeners| {
                    listeners
                        .iter()
                        .map(|listener| listener.address)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            if !addresses.is_empty() {
                let packet = build_audio_packet(
                    session,
                    seq,
                    timestamp,
                    bitrate as u16,
                    &current,
                    &previous,
                );
                for address in addresses {
                    if let Err(error) = socket.send_to(&packet, address) {
                        control.send_errors.fetch_add(1, Ordering::Relaxed);
                        log::debug!("audio send to {address} failed: {error}");
                    } else {
                        control.packets_sent.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
            previous.clear();
            previous.extend_from_slice(&current);
            seq = seq.wrapping_add(1);
            timestamp = timestamp.wrapping_add(FRAME_SAMPLES as u32);
        }
        if consumed > 0 {
            converted.drain(..consumed);
        }
        if converted.len() > pcm.len() * 8 {
            converted.clear();
        }
    }
}

fn mixer_loop(
    streams: Arc<Mutex<HashMap<String, Arc<Mutex<ReceiverStream>>>>>,
    mut feed: rtrb::Producer<[f32; 2]>,
    stop: Arc<AtomicBool>,
) {
    let mut mixed = vec![0.0f32; FRAME_SAMPLES * CHANNELS];
    let mut decoded = vec![0.0f32; FRAME_SAMPLES * CHANNELS];
    let mut next_tick = Instant::now();
    while !stop.load(Ordering::Acquire) {
        let now = Instant::now();
        if now < next_tick {
            thread::sleep(next_tick - now);
            continue;
        }
        if now.saturating_duration_since(next_tick) > Duration::from_millis(80) {
            next_tick = now;
        } else {
            next_tick += FRAME_DURATION;
        }

        mixed.fill(0.0);
        let active = streams
            .lock()
            .map(|streams| streams.values().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        let mut contributors = 0usize;
        for stream in active {
            let mut stream = stream.lock().unwrap();
            let Some(frame) = stream.buffer.tick(now) else {
                stream.level *= 0.8;
                continue;
            };
            if stream
                .decoder
                .decode(frame.payload.as_deref(), &mut decoded)
                .is_ok()
            {
                let peak = decoded
                    .iter()
                    .fold(0.0f32, |peak, sample| peak.max(sample.abs()));
                stream.level += (peak - stream.level) * 0.35;
                for (out, sample) in mixed.iter_mut().zip(decoded.iter()) {
                    *out += *sample;
                }
                contributors += 1;
            }
        }
        if contributors > 1 {
            let gain = 1.0 / (contributors as f32).sqrt();
            for sample in &mut mixed {
                *sample *= gain;
            }
        }
        for frame in mixed.chunks_exact(CHANNELS) {
            let left = frame[0].clamp(-0.98, 0.98);
            let right = frame[1].clamp(-0.98, 0.98);
            if feed.push([left, right]).is_err() {
                break;
            }
        }
    }
}

fn random_session() -> u32 {
    loop {
        let value = rand::random::<u32>();
        if value != 0 {
            return value;
        }
    }
}

fn epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn source_kind_label(kind: SourceKind) -> &'static str {
    match kind {
        SourceKind::System => "system",
        SourceKind::Microphone => "microphone",
    }
}

fn quality_tier_key(tier: QualityTier) -> &'static str {
    match tier {
        QualityTier::Lean => "lean",
        QualityTier::Balanced => "balanced",
        QualityTier::Studio => "studio",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_ids_are_never_zero() {
        for _ in 0..1_000 {
            assert_ne!(random_session(), 0);
        }
    }

    #[test]
    fn quality_keys_are_stable_wire_values() {
        assert_eq!(quality_tier_key(QualityTier::Lean), "lean");
        assert_eq!(quality_tier_key(QualityTier::Studio), "studio");
    }

    #[test]
    fn audio_datagram_detection_does_not_confuse_json() {
        assert!(!is_audio_datagram(b"{\"type\":\"announce\"}"));
        assert!(is_audio_datagram(&build_audio_packet(
            1,
            1,
            0,
            112,
            &[1],
            &[]
        )));
    }
}
