//! The session coordinator.
//!
//! 門番が 文を捌きて 歌絶えず

use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow};

use crate::adaptive::{QualityController, StreamBuffer};
use crate::audio::{
    AudioSource, CaptureGuard, CaptureTaps, Decoder, Encoder, OpenCapture, OutputDevice,
    OutputTaps, PlaybackSink, SourceKind, list_microphones, list_output_devices, open_capture,
    system_source, virtual_mic,
};
use crate::net::{LocalNetwork, bind_with_fallback, local_network, scan_discovery, send_discovery};
use crate::playout::Playout;
use crate::protocol::{
    Announce, BroadcastAnnounce, CHANNELS, CodecAnnounce, Control, FRAME_SAMPLES, Feedback,
    MAX_CONTROL_BYTES, Peer, Subscribe, Subscribed, build_audio_packet, is_audio_datagram,
    parse_audio_packet,
};
use crate::settings::{SavedListen, Settings};
use crate::state::{
    BroadcastOwnView, BroadcastView, IdentityView, ListenerView, ListeningView, NetworkView,
    NoticeView, PeerView, PrefsView, Snapshot, VirtualMicView,
};

const ANNOUNCE_EVERY: Duration = Duration::from_secs(2);
const FEEDBACK_EVERY: Duration = Duration::from_millis(500);
const SNAPSHOT_EVERY: Duration = Duration::from_millis(180);
const PEER_TTL: Duration = Duration::from_secs(25);
const SCAN_EVERY: Duration = Duration::from_secs(12);
const RECONCILE_EVERY: Duration = Duration::from_secs(1);
const AUTO_RETRY: Duration = Duration::from_secs(3);
/// Longer than any healthy graph pause; past this the capture is presumed dead.
const STARVE_LIMIT: Duration = Duration::from_secs(6);
/// Do not keep rebuilding a capture that cannot be revived.
const STARVE_BACKOFF: Duration = Duration::from_secs(20);
const PENDING_TTL: Duration = Duration::from_millis(2_500);
const LISTENER_TTL: Duration = Duration::from_secs(30);
const VIRTUAL_SETTLE: Duration = Duration::from_secs(12);
const MIN_MARGIN: usize = FRAME_SAMPLES / 2;
const MAX_MARGIN: usize = FRAME_SAMPLES * 5;
const MARGIN_CALM: Duration = Duration::from_secs(30);
const MAX_LISTENERS: usize = 24;
const MAX_INTENTS: usize = 8;

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
    StartStream {
        kind: SourceKind,
        reply: Reply,
    },
    StopStream {
        kind: SourceKind,
        reply: Reply,
    },
    Subscribe {
        key: String,
        reply: Reply,
    },
    Unsubscribe {
        key: String,
        reply: Reply,
    },
    SetAlias {
        alias: String,
        reply: Reply,
    },
    SetMic {
        source_id: String,
        reply: Reply,
    },
    SetOutput {
        output_id: Option<String>,
        reply: Reply,
    },
    SetPrefs {
        restore: Option<bool>,
        background: Option<bool>,
        local_playback: Option<bool>,
        reply: Reply,
    },
    SetVirtualMic {
        enabled: bool,
        reply: Reply,
    },
    VirtualInstalled {
        result: Result<(), String>,
    },
    RefreshDevices {
        reply: Reply,
    },
    Probe {
        reply: Reply,
    },
    ProbeAddress {
        address: String,
        reply: Reply,
    },
    Shutdown {
        reply: Reply,
    },
}

pub fn stream_key(peer_id: &str, kind: SourceKind) -> String {
    format!("{peer_id}#{}", kind.label())
}

fn parse_key(key: &str) -> Option<(String, SourceKind)> {
    let (peer_id, kind) = key.split_once('#')?;
    Some((peer_id.to_owned(), SourceKind::parse(kind)?))
}

impl Service {
    pub fn start(settings_dir: PathBuf, port: u16) -> Result<Self> {
        #[cfg(target_os = "linux")]
        crate::audio::system::cleanup_stale();

        let settings = Settings::load_or_create(&settings_dir);
        if settings.virtual_mic
            && let Err(error) = virtual_mic::prepare()
        {
            log::warn!("virtual microphone unavailable: {error:#}");
        }

        let (socket, bound) = bind_with_fallback(port, Duration::from_millis(1_500))?;
        let streams: Streams = Arc::new(Mutex::new(HashMap::new()));
        let (letters, inbox) = mpsc::channel();
        let intake = Intake::start(Arc::clone(&socket), Arc::clone(&streams), letters)?;
        let local = local_network();
        let warning = if bound != port {
            Some(format!("端口 {port} 已被占用，改用 {bound}"))
        } else if local.ip.is_none() {
            Some("没有检测到局域网地址；请检查 Wi-Fi 或网线。".to_owned())
        } else {
            None
        };
        let notice = warning.map(|message| LiveNotice {
            level: "warning".to_owned(),
            message,
            until: Instant::now() + Duration::from_secs(8),
        });
        let shared = Arc::new(Shared {
            snapshot: Mutex::new(Snapshot::default()),
        });
        let (commands, command_rx) = mpsc::channel();

        let mut coordinator = Coordinator {
            settings_dir,
            settings,
            port: bound,
            home_port: port,
            socket,
            streams,
            inbox,
            _intake: intake,
            shared: Arc::clone(&shared),
            commands: commands.clone(),
            peers: HashMap::new(),
            broadcasts: Vec::new(),
            receiver: None,
            intents: HashMap::new(),
            pending: HashMap::new(),
            mics: Vec::new(),
            outputs: Vec::new(),
            virtual_state: VirtualState::Missing,
            virtual_device: None,
            virtual_deadline: None,
            local,
            revision: 1,
            notice,
            next_announce: Instant::now(),
            next_feedback: Instant::now() + FEEDBACK_EVERY,
            next_snapshot: Instant::now() + SNAPSHOT_EVERY,
            next_network_refresh: Instant::now() + Duration::from_secs(5),
            next_scan: Instant::now(),
            next_reconcile: Instant::now(),
            next_starve_retry: Instant::now(),
            command_rx,
        };
        coordinator.reload_devices();
        coordinator.publish();
        thread::Builder::new()
            .name("loco-session".to_owned())
            .spawn(move || coordinator.run())
            .context("could not start network coordinator")?;

        Ok(Self { commands, shared })
    }

    pub fn snapshot(&self) -> Snapshot {
        self.shared.snapshot.lock().unwrap().clone()
    }

    pub fn start_stream(&self, kind: SourceKind) -> Result<(), String> {
        self.call(
            |reply| Command::StartStream { kind, reply },
            Duration::from_secs(6),
        )
    }

    pub fn stop_stream(&self, kind: SourceKind) -> Result<(), String> {
        self.call(
            |reply| Command::StopStream { kind, reply },
            Duration::from_secs(3),
        )
    }

    pub fn subscribe(&self, key: impl Into<String>) -> Result<(), String> {
        self.call(
            |reply| Command::Subscribe {
                key: key.into(),
                reply,
            },
            Duration::from_secs(4),
        )
    }

    pub fn unsubscribe(&self, key: impl Into<String>) -> Result<(), String> {
        self.call(
            |reply| Command::Unsubscribe {
                key: key.into(),
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

    pub fn set_mic(&self, source_id: impl Into<String>) -> Result<(), String> {
        self.call(
            |reply| Command::SetMic {
                source_id: source_id.into(),
                reply,
            },
            Duration::from_secs(6),
        )
    }

    pub fn set_output(&self, output_id: Option<String>) -> Result<(), String> {
        self.call(
            |reply| Command::SetOutput { output_id, reply },
            Duration::from_secs(2),
        )
    }

    pub fn set_prefs(
        &self,
        restore: Option<bool>,
        background: Option<bool>,
        local_playback: Option<bool>,
    ) -> Result<(), String> {
        self.call(
            |reply| Command::SetPrefs {
                restore,
                background,
                local_playback,
                reply,
            },
            Duration::from_secs(3),
        )
    }

    pub fn set_virtual_mic(&self, enabled: bool) -> Result<(), String> {
        self.call(
            |reply| Command::SetVirtualMic { enabled, reply },
            Duration::from_secs(4),
        )
    }

    pub fn refresh_devices(&self) -> Result<(), String> {
        self.call(
            |reply| Command::RefreshDevices { reply },
            Duration::from_secs(3),
        )
    }

    pub fn probe(&self) -> Result<(), String> {
        self.call(|reply| Command::Probe { reply }, Duration::from_secs(3))
    }

    pub fn probe_address(&self, address: impl Into<String>) -> Result<(), String> {
        self.call(
            |reply| Command::ProbeAddress {
                address: address.into(),
                reply,
            },
            Duration::from_secs(2),
        )
    }

    pub fn shutdown(&self) {
        let _ = self.call(|reply| Command::Shutdown { reply }, Duration::from_secs(3));
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

fn microphones(settings: &Settings) -> Vec<AudioSource> {
    let hide_virtual = settings.virtual_mic;
    list_microphones(|id, name| hide_virtual && virtual_mic::hides_input(id, name))
}

enum StopCause {
    User,
    Shutdown,
    Restart,
    Failed(String),
}

impl StopCause {
    // 席替へは 告げずに札を 掛け替ふる
    fn reason(&self) -> Option<&str> {
        match self {
            Self::User | Self::Shutdown => Some("本机已停止广播"),
            Self::Restart => None,
            Self::Failed(message) => Some(message),
        }
    }

    fn announces(&self) -> bool {
        !matches!(self, Self::Shutdown)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum VirtualState {
    Missing,
    Installing,
    Settling,
    Ready,
    Restart,
}

impl VirtualState {
    fn label(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Installing | Self::Settling => "installing",
            Self::Ready => "ready",
            Self::Restart => "restart",
        }
    }
}

struct Coordinator {
    settings_dir: PathBuf,
    settings: Settings,
    port: u16,
    home_port: u16,
    socket: Arc<UdpSocket>,
    streams: Streams,
    inbox: Receiver<(Vec<u8>, SocketAddr)>,
    _intake: Intake,
    shared: Arc<Shared>,
    commands: Sender<Command>,
    peers: HashMap<String, Peer>,
    broadcasts: Vec<BroadcastRuntime>,
    receiver: Option<ReceiverRuntime>,
    intents: HashMap<String, ReceiverIntent>,
    pending: HashMap<String, PendingSub>,
    mics: Vec<AudioSource>,
    outputs: Vec<OutputDevice>,
    virtual_state: VirtualState,
    virtual_device: Option<OutputDevice>,
    virtual_deadline: Option<Instant>,
    local: LocalNetwork,
    revision: u64,
    notice: Option<LiveNotice>,
    next_announce: Instant,
    next_feedback: Instant,
    next_snapshot: Instant,
    next_network_refresh: Instant,
    next_scan: Instant,
    next_reconcile: Instant,
    next_starve_retry: Instant,
    command_rx: Receiver<Command>,
}

struct LiveNotice {
    level: String,
    message: String,
    until: Instant,
}

struct PendingSub {
    peer_id: String,
    kind: SourceKind,
    deadline: Instant,
    reply: Option<Reply>,
}

struct ReceiverIntent {
    peer_id: String,
    kind: SourceKind,
    sender_alias: String,
    source: String,
    last_session: Option<u32>,
    next_attempt: Instant,
}

struct BroadcastRuntime {
    kind: SourceKind,
    session: u32,
    started_at_ms: u64,
    source: AudioSource,
    capture: Option<CaptureGuard>,
    taps: CaptureTaps,
    listeners: Arc<Mutex<Vec<ListenerEntry>>>,
    control: Arc<SenderControl>,
    sender: Option<JoinHandle<()>>,
    quality: QualityController,
    excludes_self: bool,
}

struct SenderControl {
    stop: AtomicBool,
    bitrate_kbps: AtomicU32,
    loss_x100: AtomicU32,
    /// Set when the capture has delivered nothing for longer than any audio
    /// graph should stutter, so the coordinator can rebuild the stream.
    starved: AtomicBool,
}

impl SenderControl {
    fn new() -> Self {
        Self {
            stop: AtomicBool::new(false),
            bitrate_kbps: AtomicU32::new(128),
            loss_x100: AtomicU32::new(0),
            starved: AtomicBool::new(false),
        }
    }
}

#[derive(Clone)]
struct ListenerEntry {
    alias: String,
    address: SocketAddr,
    last_seen: Instant,
    report: Option<Feedback>,
}

type Outlet = Arc<Mutex<Option<Lane>>>;
type Streams = Arc<Mutex<HashMap<String, Arc<Mutex<ReceiverStream>>>>>;

struct VirtualSink {
    _stream: cpal::Stream,
    name: String,
    error: Arc<Mutex<Option<String>>>,
}

struct ReceiverRuntime {
    _playback_stream: cpal::Stream,
    playback: crate::audio::PlaybackInfo,
    error: Arc<Mutex<Option<String>>>,
    streams: Streams,
    virtual_out: Outlet,
    virtual_sink: Option<VirtualSink>,
    stop: Arc<AtomicBool>,
    mixer: Option<JoinHandle<()>>,
}

struct ReceiverStream {
    key: String,
    sender_id: String,
    sender_alias: String,
    kind: SourceKind,
    source: String,
    session: u32,
    address: SocketAddr,
    buffer: StreamBuffer,
    playout: Playout,
    decoder: Decoder,
    last_packet: Instant,
    level: f32,
    to_virtual: bool,
}

impl ReceiverStream {
    fn render(&mut self, now: Instant, pcm: &mut [f32]) -> bool {
        let decoder = &mut self.decoder;
        let sounding = self.playout.render(
            now,
            &mut self.buffer,
            &mut |payload, pcm| decoder.decode(payload, pcm).is_ok(),
            pcm,
        );
        let peak = pcm
            .iter()
            .fold(0.0f32, |peak, sample| peak.max(sample.abs()));
        self.level += (peak - self.level) * 0.35;
        sounding
    }
}

impl Coordinator {
    fn run(mut self) {
        log::info!(
            "session coordinator online on UDP {} ({:?})",
            self.port,
            self.local
        );
        self.probe_network();
        self.restore();
        loop {
            self.drain_inbox();
            while let Ok(command) = self.command_rx.try_recv() {
                if !self.handle_command(command) {
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
                self.settle_virtual(now);
            }
            if now >= self.next_scan {
                self.scan_network();
            }
            if now >= self.next_reconcile {
                self.reconcile_receiver(now);
                self.next_reconcile = now + RECONCILE_EVERY;
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

    // 目覚めては 昨日の歌を また歌ふ
    fn restore(&mut self) {
        if !self.settings.restore {
            return;
        }
        let now = Instant::now();
        for saved in self.settings.listen.clone() {
            let Some(kind) = SourceKind::parse(&saved.kind) else {
                continue;
            };
            self.intents.insert(
                stream_key(&saved.peer_id, kind),
                ReceiverIntent {
                    peer_id: saved.peer_id,
                    kind,
                    sender_alias: saved.alias,
                    source: saved.source,
                    last_session: None,
                    next_attempt: now,
                },
            );
        }
        for (kind, on) in [
            (SourceKind::System, self.settings.system_on),
            (SourceKind::Microphone, self.settings.mic_on),
        ] {
            if on && let Err(error) = self.start_stream(kind) {
                self.set_notice("error", format!("{error:#}"));
            }
        }
        self.publish();
    }

    fn shutdown(&mut self) {
        for kind in [SourceKind::System, SourceKind::Microphone] {
            self.stop_stream(kind, StopCause::Shutdown);
        }
        self.stop_receiver();
        virtual_mic::release();
        self.publish();
        log::info!("session coordinator stopped");
    }

    fn save(&self) {
        if let Err(error) = self.settings.save(&self.settings_dir) {
            log::warn!("could not save settings: {error:#}");
        }
    }

    fn handle_command(&mut self, command: Command) -> bool {
        match command {
            Command::StartStream { kind, reply } => {
                let result = self
                    .start_stream(kind)
                    .map_err(|error| format!("{error:#}"));
                if result.is_ok() {
                    self.remember_stream(kind, true);
                } else if let Err(message) = &result {
                    self.set_notice("error", message.clone());
                    self.publish();
                }
                let _ = reply.send(result);
            }
            Command::StopStream { kind, reply } => {
                self.stop_stream(kind, StopCause::User);
                self.remember_stream(kind, false);
                let _ = reply.send(Ok(()));
            }
            Command::Subscribe { key, reply } => {
                let result = self.begin_subscribe(&key, reply);
                if let Err(message) = &result {
                    self.set_notice("error", message.clone());
                    self.publish();
                }
            }
            Command::Unsubscribe { key, reply } => {
                self.unsubscribe(&key);
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
            Command::SetMic { source_id, reply } => {
                let result = if !self.mics.iter().any(|source| source.id == source_id) {
                    Err("这个麦克风已经不可用".to_owned())
                } else {
                    self.settings.mic_id = Some(source_id);
                    self.save();
                    if self.stream(SourceKind::Microphone).is_some() {
                        self.stop_stream(SourceKind::Microphone, StopCause::Restart);
                        self.start_stream(SourceKind::Microphone)
                            .map_err(|error| format!("{error:#}"))
                    } else {
                        Ok(())
                    }
                };
                if let Err(message) = &result {
                    self.set_notice("error", message.clone());
                }
                self.publish();
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
            Command::SetPrefs {
                restore,
                background,
                local_playback,
                reply,
            } => {
                if let Some(restore) = restore {
                    self.settings.restore = restore;
                }
                if let Some(background) = background {
                    self.settings.background = background;
                }
                let routed = local_playback.filter(|local| *local != self.settings.local_playback);
                if let Some(local) = routed {
                    self.settings.local_playback = local;
                }
                self.save();
                if routed.is_some() && self.stream(SourceKind::System).is_some() {
                    // The takeover has to be rebuilt: it owns the loopback.
                    self.stop_stream(SourceKind::System, StopCause::Restart);
                    if let Err(error) = self.start_stream(SourceKind::System) {
                        self.set_notice("error", format!("{error:#}"));
                    }
                }
                self.publish();
                let _ = reply.send(Ok(()));
            }
            Command::SetVirtualMic { enabled, reply } => {
                let result = self.set_virtual_mic(enabled);
                self.publish();
                let _ = reply.send(result);
            }
            Command::VirtualInstalled { result } => {
                match result {
                    Ok(()) => {
                        self.virtual_state = VirtualState::Settling;
                        self.virtual_deadline = Some(Instant::now() + VIRTUAL_SETTLE);
                    }
                    Err(message) => {
                        self.virtual_state = VirtualState::Missing;
                        self.settings.virtual_mic = false;
                        self.set_notice("error", message);
                    }
                }
                self.reload_devices();
                self.publish();
            }
            Command::RefreshDevices { reply } => {
                self.reload_devices();
                let _ = reply.send(Ok(()));
            }
            Command::Probe { reply } => {
                self.probe_network();
                let _ = reply.send(Ok(()));
            }
            Command::ProbeAddress { address, reply } => {
                let result = self.probe_address(&address);
                if let Err(message) = &result {
                    self.set_notice("error", message.clone());
                    self.publish();
                }
                let _ = reply.send(result);
            }
            Command::Shutdown { reply } => {
                self.shutdown();
                let _ = reply.send(Ok(()));
                return false;
            }
        }
        true
    }

    fn remember_stream(&mut self, kind: SourceKind, on: bool) {
        match kind {
            SourceKind::System => self.settings.system_on = on,
            SourceKind::Microphone => self.settings.mic_on = on,
        }
        self.save();
    }

    fn stream(&self, kind: SourceKind) -> Option<&BroadcastRuntime> {
        self.broadcasts.iter().find(|runtime| runtime.kind == kind)
    }

    fn primary_stream(&self) -> Option<&BroadcastRuntime> {
        self.stream(SourceKind::System)
            .or_else(|| self.broadcasts.first())
    }

    fn start_stream(&mut self, kind: SourceKind) -> Result<()> {
        if self.stream(kind).is_some() {
            return Ok(());
        }
        let source = match kind {
            SourceKind::System => system_source(),
            SourceKind::Microphone => self
                .settings
                .mic_id
                .as_ref()
                .and_then(|id| self.mics.iter().find(|source| &source.id == id))
                .or_else(|| self.mics.iter().find(|source| source.is_default))
                .or_else(|| self.mics.first())
                .cloned()
                .ok_or_else(|| anyhow!("没有可用的麦克风"))?,
        };

        let OpenCapture {
            guard,
            feed,
            source: opened_source,
            sample_rate,
            channels,
            taps,
            excludes_self,
        } = open_capture(&source, self.settings.local_playback)?;
        if let Some(message) = taps.error.lock().unwrap().take() {
            return Err(anyhow!(message));
        }

        let session = random_session();
        let listeners = Arc::new(Mutex::new(Vec::<ListenerEntry>::new()));
        let control = Arc::new(SenderControl::new());
        let encoder = Encoder::new().context("could not initialise Opus encoder")?;
        let socket = Arc::clone(&self.socket);
        let listener_registry = Arc::clone(&listeners);
        let worker_control = Arc::clone(&control);
        let sender = thread::Builder::new()
            .name(format!("loco-encoder-{}", kind.label()))
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

        self.broadcasts.push(BroadcastRuntime {
            kind,
            session,
            started_at_ms: epoch_ms(),
            source: opened_source,
            capture: Some(guard),
            taps,
            listeners,
            control,
            sender: Some(sender),
            quality: QualityController::new(),
            excludes_self,
        });
        self.broadcasts
            .sort_by_key(|runtime| runtime.kind != SourceKind::System);
        self.announce_now();
        self.publish();
        Ok(())
    }

    fn stop_stream(&mut self, kind: SourceKind, cause: StopCause) {
        let Some(index) = self
            .broadcasts
            .iter()
            .position(|runtime| runtime.kind == kind)
        else {
            return;
        };
        let mut runtime = self.broadcasts.remove(index);
        if let Some(reason) = cause.reason() {
            let ended = Control::Ended {
                session: runtime.session,
                reason: reason.to_owned(),
            };
            if let Ok(listeners) = runtime.listeners.lock() {
                for listener in listeners.iter() {
                    self.send_control(listener.address, &ended);
                }
            }
        }
        runtime.control.stop.store(true, Ordering::Release);
        drop(runtime.capture.take());
        if let Some(sender) = runtime.sender.take() {
            let _ = sender.join();
        }
        if cause.announces() {
            self.announce_now();
        }
        if let StopCause::Failed(message) = cause {
            self.set_notice("error", message);
        }
        self.publish();
    }

    fn begin_subscribe(&mut self, key: &str, reply: Reply) -> Result<(), String> {
        let outcome: Result<(String, SourceKind, SocketAddr, u32), String> = (|| {
            let (peer_id, kind) =
                parse_key(key).ok_or_else(|| "这台设备当前没有广播".to_owned())?;
            let key = stream_key(&peer_id, kind);
            if self.pending.contains_key(&key) {
                return Err("正在连接这台设备".to_owned());
            }
            if !self.intents.contains_key(&key) && self.intents.len() >= MAX_INTENTS {
                return Err("一次最多收听 8 路音频".to_owned());
            }
            let peer = self
                .peers
                .get(&peer_id)
                .ok_or_else(|| "设备已离线".to_owned())?;
            let stream = peer
                .stream(kind.label())
                .ok_or_else(|| "这台设备当前没有广播".to_owned())?;
            Ok((
                peer_id.clone(),
                kind,
                SocketAddr::new(peer.address.ip(), peer.port),
                stream.session,
            ))
        })();
        let (peer_id, kind, address, session) = match outcome {
            Ok(target) => target,
            Err(message) => {
                let _ = reply.send(Err(message.clone()));
                return Err(message);
            }
        };
        let key = stream_key(&peer_id, kind);
        let already = self.receiver.as_ref().is_some_and(|runtime| {
            runtime
                .streams
                .lock()
                .unwrap()
                .get(&key)
                .is_some_and(|stream| stream.lock().unwrap().session == session)
        });
        if already {
            let _ = reply.send(Ok(()));
            return Ok(());
        }
        if let Err(message) = self.ensure_receiver() {
            let _ = reply.send(Err(message.clone()));
            return Err(message);
        }
        self.send_subscribe(address, session);
        self.pending.insert(
            key,
            PendingSub {
                peer_id,
                kind,
                deadline: Instant::now() + PENDING_TTL,
                reply: Some(reply),
            },
        );
        self.publish();
        Ok(())
    }

    fn send_subscribe(&self, address: SocketAddr, session: u32) {
        let control = Control::Subscribe(Subscribe {
            id: self.settings.id.clone(),
            alias: self.settings.alias.clone(),
            platform: std::env::consts::OS.to_owned(),
            session: Some(session),
        });
        self.send_control(address, &control);
    }

    fn unsubscribe(&mut self, key: &str) {
        self.intents.remove(key);
        if let Some(pending) = self.pending.remove(key)
            && let Some(reply) = pending.reply
        {
            let _ = reply.send(Err("已取消收听".to_owned()));
        }
        if let Some(runtime) = self.receiver.as_ref() {
            let removed = runtime.streams.lock().unwrap().remove(key);
            if let Some(stream) = removed {
                let stream = stream.lock().unwrap();
                self.send_control(
                    stream.address,
                    &Control::Unsubscribe {
                        session: stream.session,
                    },
                );
            }
        }
        self.forget_listen(key);
        if self.intents.is_empty() && self.pending.is_empty() {
            self.stop_receiver();
        } else {
            self.sync_virtual_route();
        }
        self.publish();
    }

    fn remember_listen(&mut self, peer_id: &str, kind: SourceKind, alias: &str, source: &str) {
        let saved = SavedListen {
            peer_id: peer_id.to_owned(),
            kind: kind.label().to_owned(),
            alias: alias.to_owned(),
            source: source.to_owned(),
        };
        let existing = self
            .settings
            .listen
            .iter_mut()
            .find(|item| item.peer_id == saved.peer_id && item.kind == saved.kind);
        match existing {
            Some(item) if *item == saved => return,
            Some(item) => *item = saved,
            None => self.settings.listen.push(saved),
        }
        self.save();
    }

    fn forget_listen(&mut self, key: &str) {
        let Some((peer_id, kind)) = key.split_once('#') else {
            return;
        };
        let before = self.settings.listen.len();
        self.settings
            .listen
            .retain(|item| !(item.peer_id == peer_id && item.kind == kind));
        if self.settings.listen.len() != before {
            self.save();
        }
    }

    fn stop_receiver(&mut self) {
        if let Some(mut runtime) = self.receiver.take() {
            runtime.stop.store(true, Ordering::Release);
            if let Some(mixer) = runtime.mixer.take() {
                let _ = mixer.join();
            }
        }
        self.streams.lock().unwrap().clear();
        self.publish();
    }

    fn playback_device(&self) -> Option<String> {
        self.settings.output_id.clone().or_else(|| {
            self.broadcasts
                .iter()
                .find_map(|runtime| runtime.capture.as_ref()?.playback_override())
        })
    }

    fn ensure_receiver(&mut self) -> Result<(), String> {
        if self.receiver.is_some() {
            return Ok(());
        }
        let PlaybackSink {
            stream,
            feed,
            info,
            taps,
        } = PlaybackSink::open(self.playback_device().as_deref())
            .map_err(|error| format!("输出设备无法打开：{error}，请在设置中更换"))?;
        let streams = Arc::clone(&self.streams);
        let virtual_out: Outlet = Arc::new(Mutex::new(None));
        let stop = Arc::new(AtomicBool::new(false));
        let mixer = {
            let streams = Arc::clone(&streams);
            let virtual_out = Arc::clone(&virtual_out);
            let stop = Arc::clone(&stop);
            let main = Lane::new(feed, taps.clone());
            thread::Builder::new()
                .name("loco-mixer".to_owned())
                .spawn(move || mixer_loop(streams, main, virtual_out, stop))
                .map_err(|error| format!("混音线程无法启动：{error}"))?
        };
        self.receiver = Some(ReceiverRuntime {
            _playback_stream: stream,
            playback: info,
            error: taps.error,
            streams,
            virtual_out,
            virtual_sink: None,
            stop,
            mixer: Some(mixer),
        });
        Ok(())
    }

    fn virtual_ready(&self) -> bool {
        self.settings.virtual_mic
            && self.virtual_state == VirtualState::Ready
            && self.virtual_device.is_some()
    }

    // 声ひとつ 仮の喉へと 分け入りぬ
    fn sync_virtual_route(&mut self) {
        let ready = self.virtual_ready();
        let candidates = virtual_mic::candidates(&self.outputs);
        let Some(runtime) = self.receiver.as_mut() else {
            return;
        };
        let wants = ready
            && runtime
                .streams
                .lock()
                .unwrap()
                .values()
                .any(|stream| stream.lock().unwrap().kind == SourceKind::Microphone);
        let mut failed = false;
        if wants && runtime.virtual_sink.is_none() {
            for device in &candidates {
                match PlaybackSink::open(Some(&device.id)) {
                    Ok(sink) => {
                        *runtime.virtual_out.lock().unwrap() =
                            Some(Lane::new(sink.feed, sink.taps.clone()));
                        runtime.virtual_sink = Some(VirtualSink {
                            _stream: sink.stream,
                            name: sink.info.device_name,
                            error: sink.taps.error,
                        });
                        break;
                    }
                    Err(error) => {
                        log::warn!(
                            "virtual microphone output {} failed: {error:#}",
                            device.name
                        );
                    }
                }
            }
            failed = runtime.virtual_sink.is_none();
        } else if !wants && runtime.virtual_sink.is_some() {
            *runtime.virtual_out.lock().unwrap() = None;
            runtime.virtual_sink = None;
        }
        let routed = runtime.virtual_sink.is_some();
        for stream in runtime.streams.lock().unwrap().values() {
            let mut stream = stream.lock().unwrap();
            stream.to_virtual = routed && stream.kind == SourceKind::Microphone;
        }
        if failed {
            self.set_notice(
                "warning",
                format!("{} 被占用，麦克风改由扬声器播放", virtual_mic::PROVIDER),
            );
        }
    }

    fn set_virtual_mic(&mut self, enabled: bool) -> Result<(), String> {
        self.settings.virtual_mic = enabled;
        if !enabled {
            virtual_mic::release();
        }
        self.reload_devices();
        let installing = matches!(
            self.virtual_state,
            VirtualState::Installing | VirtualState::Settling
        );
        if enabled && self.virtual_device.is_none() && !installing {
            self.virtual_state = VirtualState::Installing;
            let commands = self.commands.clone();
            thread::Builder::new()
                .name("loco-virtual-mic".to_owned())
                .spawn(move || {
                    let result = virtual_mic::install().map_err(|error| format!("{error:#}"));
                    let _ = commands.send(Command::VirtualInstalled { result });
                })
                .map_err(|error| format!("{error}"))?;
        }
        Ok(())
    }

    fn settle_virtual(&mut self, now: Instant) {
        if self.virtual_state != VirtualState::Settling {
            return;
        }
        self.reload_devices();
        if self.virtual_state == VirtualState::Settling
            && self
                .virtual_deadline
                .is_some_and(|deadline| now >= deadline)
        {
            self.virtual_state = VirtualState::Restart;
        }
    }

    fn reconcile_receiver(&mut self, now: Instant) {
        if self.intents.is_empty() {
            return;
        }
        let keys = self.intents.keys().cloned().collect::<Vec<_>>();
        for key in keys {
            let Some((peer_id, kind)) = self
                .intents
                .get(&key)
                .map(|intent| (intent.peer_id.clone(), intent.kind))
            else {
                continue;
            };
            let active_session = self.receiver.as_ref().and_then(|runtime| {
                runtime.streams.lock().ok().and_then(|streams| {
                    streams
                        .get(&key)
                        .map(|stream| stream.lock().unwrap().session)
                })
            });
            let peer_state = self.peers.get(&peer_id).map(|peer| {
                (
                    SocketAddr::new(peer.address.ip(), peer.port),
                    peer.stream(kind.label()).map(|stream| stream.session),
                )
            });

            let mut dropped_active = false;
            if active_session.is_some()
                && let Some((_, announced)) = peer_state.as_ref()
                && (announced.is_none()
                    || announced.is_some_and(|session| Some(session) != active_session))
            {
                if let Some(runtime) = self.receiver.as_ref() {
                    runtime.streams.lock().unwrap().remove(&key);
                }
                dropped_active = true;
                if let Some(intent) = self.intents.get_mut(&key) {
                    intent.next_attempt = if announced.is_some() {
                        now
                    } else {
                        now + AUTO_RETRY
                    };
                }
            }
            if dropped_active {
                log::debug!("receiver stream {key} is stale; returning to standby");
                self.sync_virtual_route();
                self.publish_soon();
            }

            let active_now = active_session.is_some() && !dropped_active;
            let Some((address, Some(session))) = peer_state else {
                continue;
            };
            if let Some(intent) = self.intents.get_mut(&key)
                && intent.last_session != Some(session)
            {
                intent.last_session = Some(session);
                intent.next_attempt = now;
            }
            let ready = now
                >= self
                    .intents
                    .get(&key)
                    .map(|intent| intent.next_attempt)
                    .unwrap_or(now);
            if !active_now && ready && !self.pending.contains_key(&key) {
                self.auto_subscribe(&key, address, session, now);
            }
        }
    }

    fn auto_subscribe(&mut self, key: &str, address: SocketAddr, session: u32, now: Instant) {
        if let Err(error) = self.ensure_receiver() {
            if let Some(intent) = self.intents.get_mut(key) {
                intent.next_attempt = now + Duration::from_secs(10);
            }
            self.set_notice("warning", error);
            return;
        }
        self.send_subscribe(address, session);
        let Some(intent) = self.intents.get_mut(key) else {
            return;
        };
        intent.last_session = Some(session);
        intent.next_attempt = now + AUTO_RETRY;
        let (peer_id, kind) = (intent.peer_id.clone(), intent.kind);
        self.pending.insert(
            key.to_owned(),
            PendingSub {
                peer_id,
                kind,
                deadline: now + PENDING_TTL,
                reply: None,
            },
        );
        log::debug!("attempting automatic resubscribe to {key}");
        self.publish_soon();
    }

    fn handle_subscribed(&mut self, subscribed: Subscribed, from: SocketAddr) {
        let kind = SourceKind::parse(&subscribed.source_kind).unwrap_or(SourceKind::System);
        let key = stream_key(&subscribed.source_id, kind);
        let Some(pending) = self.pending.remove(&key) else {
            // A late acknowledgement after timeout, or a spoofed reply: there
            // is no listener intent to attach it to.
            log::debug!("unsolicited subscribed from {from}");
            return;
        };
        let Some(runtime) = self.receiver.as_ref() else {
            if let Some(reply) = pending.reply {
                let _ = reply.send(Err("输出设备已停止".to_owned()));
            }
            return;
        };
        let now = Instant::now();
        match Decoder::new() {
            Ok(decoder) => {
                let sender_alias = subscribed.source_alias;
                let source = subscribed.source;
                let stream = ReceiverStream {
                    key: key.clone(),
                    sender_id: pending.peer_id.clone(),
                    sender_alias: sender_alias.clone(),
                    kind: pending.kind,
                    source: source.clone(),
                    session: subscribed.session,
                    address: from,
                    buffer: StreamBuffer::new(now),
                    playout: Playout::new(),
                    decoder,
                    last_packet: now,
                    level: 0.0,
                    to_virtual: false,
                };
                runtime
                    .streams
                    .lock()
                    .unwrap()
                    .insert(key.clone(), Arc::new(Mutex::new(stream)));
                self.intents.insert(
                    key,
                    ReceiverIntent {
                        peer_id: pending.peer_id.clone(),
                        kind: pending.kind,
                        sender_alias: sender_alias.clone(),
                        source: source.clone(),
                        last_session: Some(subscribed.session),
                        next_attempt: now + AUTO_RETRY,
                    },
                );
                self.remember_listen(&pending.peer_id, pending.kind, &sender_alias, &source);
                self.sync_virtual_route();
                if let Some(reply) = pending.reply {
                    let _ = reply.send(Ok(()));
                }
            }
            Err(error) => {
                let message = format!("Opus 解码器初始化失败：{error}");
                if let Some(intent) = self.intents.get_mut(&key) {
                    intent.next_attempt = now + AUTO_RETRY;
                }
                if let Some(reply) = pending.reply {
                    let _ = reply.send(Err(message.clone()));
                }
                self.set_notice("error", message);
            }
        }
        self.publish();
    }

    fn drain_inbox(&mut self) {
        // A bounded batch means a flooded socket cannot starve timers either.
        for _ in 0..96 {
            let Ok((payload, from)) = self.inbox.try_recv() else {
                break;
            };
            match serde_json::from_slice::<Control>(&payload) {
                Ok(control) => self.handle_control(control, from),
                Err(error) => log::debug!("unreadable control from {from}: {error}"),
            }
        }
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
                self.reconcile_receiver(Instant::now());
                self.publish_soon();
            }
            Control::Probe { id, announce } => {
                if id == self.settings.id {
                    return;
                }
                let response_port = announce
                    .as_ref()
                    .map(|announce| announce.port)
                    .unwrap_or_else(|| from.port());
                if let Some(announce) = announce
                    && announce.is_compatible()
                    && announce.id != self.settings.id
                {
                    let address = SocketAddr::new(from.ip(), announce.port);
                    let peer_id = announce.id.clone();
                    self.peers
                        .insert(peer_id, Peer::from_announce(&announce, address));
                    self.reconcile_receiver(Instant::now());
                    self.publish_soon();
                }
                self.send_announce_to(SocketAddr::new(from.ip(), response_port));
            }
            Control::Subscribe(subscribe) => self.accept_subscriber(subscribe, from),
            Control::Subscribed(subscribed) => self.handle_subscribed(subscribed, from),
            Control::Unsubscribe { session } => {
                if let Some(runtime) = self
                    .broadcasts
                    .iter()
                    .find(|runtime| runtime.session == session)
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
                let removed = self.receiver.as_ref().and_then(|runtime| {
                    let mut streams = runtime.streams.lock().unwrap();
                    let key = streams
                        .iter()
                        .find(|(_, stream)| {
                            let stream = stream.lock().unwrap();
                            stream.session == session && stream.address.ip() == from.ip()
                        })
                        .map(|(key, _)| key.clone())?;
                    streams.remove(&key).map(|stream| (key, stream))
                });
                if let Some((key, stream)) = removed {
                    let alias = stream.lock().unwrap().sender_alias.clone();
                    if let Some(intent) = self.intents.get_mut(&key) {
                        intent.next_attempt = Instant::now() + AUTO_RETRY;
                    }
                    self.set_notice("info", format!("{alias} 已停止广播（{reason}）"));
                    self.sync_virtual_route();
                    self.publish_soon();
                }
            }
            Control::Feedback(feedback) => self.handle_feedback(feedback, from),
        }
    }

    fn accept_subscriber(&mut self, subscribe: Subscribe, from: SocketAddr) {
        let runtime = match subscribe.session {
            Some(session) => self
                .broadcasts
                .iter()
                .find(|runtime| runtime.session == session),
            None => self.primary_stream(),
        };
        let Some(runtime) = runtime else {
            self.send_control(
                from,
                &Control::Ended {
                    session: subscribe.session.unwrap_or(0),
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
                alias: subscribe.alias,
                address: SocketAddr::new(from.ip(), from.port()),
                last_seen: Instant::now(),
                report: None,
            });
        } else if let Some(listener) = listeners
            .iter_mut()
            .find(|listener| listener.address.ip() == from.ip())
        {
            // A receiver that reconnects comes back on a fresh source port.
            // Keeping the old one leaves the stream talking to a socket nobody
            // is reading, and the listener's own chatter keeps it alive.
            listener.alias = subscribe.alias;
            listener.address = SocketAddr::new(from.ip(), from.port());
            listener.last_seen = Instant::now();
        }
        drop(listeners);

        let response = Control::Subscribed(Subscribed {
            session: runtime.session,
            source_id: self.settings.id.clone(),
            source_alias: self.settings.alias.clone(),
            source: runtime.source.name.clone(),
            source_kind: runtime.kind.label().to_owned(),
            sample_rate: crate::protocol::SAMPLE_RATE,
            channels: CHANNELS as u16,
            frame_ms: crate::protocol::FRAME_MS as u16,
        });
        self.send_control(SocketAddr::new(from.ip(), from.port()), &response);
        self.announce_now();
        self.publish_soon();
    }

    fn handle_feedback(&mut self, feedback: Feedback, from: SocketAddr) {
        let Some(runtime) = self
            .broadcasts
            .iter_mut()
            .find(|runtime| runtime.session == feedback.session)
        else {
            return;
        };
        let reports = {
            let mut listeners = runtime.listeners.lock().unwrap();
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
        let quality = runtime.quality.observe(&reports);
        runtime
            .control
            .bitrate_kbps
            .store(quality.bitrate_kbps, Ordering::Relaxed);
        runtime
            .control
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
        let probe = self.build_probe();
        if let Ok(json) = serde_json::to_vec(&probe) {
            let sent = send_discovery(&json, self.home_port);
            log::debug!("discovery probe sent on {sent} interface targets");
        }
        self.scan_network();
        self.announce();
        self.next_announce = Instant::now() + ANNOUNCE_EVERY;
    }

    fn scan_network(&mut self) {
        let probe = self.build_probe();
        if let Ok(json) = serde_json::to_vec(&probe) {
            let sent = scan_discovery(&self.socket, &json, self.home_port);
            log::debug!("subnet discovery probe sent to {sent} hosts");
        }
        self.next_scan = Instant::now() + SCAN_EVERY;
    }

    fn probe_address(&mut self, address: &str) -> Result<(), String> {
        let address = address.trim();
        if address.is_empty() {
            return Err("请输入对方的局域网 IP".to_owned());
        }
        let target = match address.parse::<SocketAddr>() {
            Ok(target) => target,
            Err(_) => format!("{address}:{}", self.home_port)
                .parse::<SocketAddr>()
                .map_err(|_| "IP 地址格式不正确".to_owned())?,
        };
        if !target.ip().is_ipv4() {
            return Err("目前只支持 IPv4 地址".to_owned());
        }
        if target.ip() == IpAddr::V4(Ipv4Addr::UNSPECIFIED) || target.ip().is_loopback() {
            return Err("请输入同一局域网内另一台设备的地址".to_owned());
        }
        let probe = self.build_probe();
        self.send_control(target, &probe);
        self.set_notice("info", format!("已向 {target} 发送发现请求"));
        self.publish_soon();
        Ok(())
    }

    fn build_probe(&self) -> Control {
        Control::Probe {
            id: self.settings.id.clone(),
            announce: Some(self.build_announce()),
        }
    }

    fn announce(&mut self) {
        let announce = self.build_announce();
        let control = Control::Announce(announce);
        if let Ok(json) = serde_json::to_vec(&control) {
            send_discovery(&json, self.home_port);
        }
        let addresses = self
            .peers
            .values()
            .map(|peer| SocketAddr::new(peer.address.ip(), peer.port))
            .collect::<Vec<_>>();
        for address in addresses {
            self.send_control(address, &control);
        }
    }

    fn send_announce_to(&self, address: SocketAddr) {
        let announce = self.build_announce();
        self.send_control(address, &Control::Announce(announce));
    }

    fn build_announce(&self) -> Announce {
        let hostname = hostname::get()
            .ok()
            .and_then(|name| name.into_string().ok())
            .unwrap_or_else(|| "unknown".to_owned());
        let streams = self
            .broadcasts
            .iter()
            .map(|runtime| BroadcastAnnounce {
                session: runtime.session,
                source: runtime.source.name.clone(),
                source_kind: runtime.kind.label().to_owned(),
                started_at_ms: runtime.started_at_ms,
                listeners: runtime
                    .listeners
                    .lock()
                    .map(|listeners| listeners.len() as u32)
                    .unwrap_or(0),
                codec: CodecAnnounce::default(),
            })
            .collect();
        let mut announce = Announce::new(
            self.settings.id.clone(),
            self.settings.alias.clone(),
            std::env::consts::OS,
            None,
        )
        .with_streams(streams);
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
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        for key in expired {
            if let Some(pending) = self.pending.remove(&key) {
                if let Some(reply) = pending.reply {
                    let _ = reply.send(Err("对方没有回应，请确认两台设备在同一局域网".to_owned()));
                } else if let Some(intent) = self.intents.get_mut(&key) {
                    intent.next_attempt = now + AUTO_RETRY;
                }
            }
        }
        let unused_receiver = self.intents.is_empty()
            && self.pending.is_empty()
            && self
                .receiver
                .as_ref()
                .is_some_and(|runtime| runtime.streams.lock().unwrap().is_empty());
        if unused_receiver {
            self.stop_receiver();
        }
    }

    fn prune_peers(&mut self, now: Instant) {
        self.peers
            .retain(|_, peer| now.saturating_duration_since(peer.last_seen) < PEER_TTL);
    }

    fn reap_stale_listeners(&mut self, now: Instant) {
        let mut changed = false;
        for runtime in &self.broadcasts {
            if let Ok(mut listeners) = runtime.listeners.lock() {
                let before = listeners.len();
                listeners.retain(|listener| {
                    now.saturating_duration_since(listener.last_seen) < LISTENER_TTL
                });
                changed |= listeners.len() != before;
            }
        }
        if changed {
            self.announce_now();
        }
    }

    fn check_health(&mut self) {
        let starved = self
            .broadcasts
            .iter()
            .filter(|runtime| runtime.control.starved.load(Ordering::Relaxed))
            .map(|runtime| runtime.kind)
            .collect::<Vec<_>>();
        for kind in starved {
            let now = Instant::now();
            if now < self.next_starve_retry {
                continue;
            }
            self.next_starve_retry = now + STARVE_BACKOFF;
            log::warn!(
                "{} capture delivered nothing for {STARVE_LIMIT:?}; rebuilding it",
                kind.label()
            );
            self.stop_stream(kind, StopCause::Restart);
            if let Err(error) = self.start_stream(kind) {
                self.set_notice("error", format!("{error:#}"));
            }
        }
        let failed = self
            .broadcasts
            .iter()
            .filter_map(|runtime| {
                runtime
                    .taps
                    .error
                    .lock()
                    .ok()
                    .and_then(|mut slot| slot.take())
                    .map(|error| (runtime.kind, error))
            })
            .collect::<Vec<_>>();
        for (kind, error) in failed {
            log::warn!("{} capture stopped: {error}", kind.label());
            self.stop_stream(kind, StopCause::Failed(format!("采集已中断：{error}")));
        }
        let virtual_error = self.receiver.as_ref().and_then(|runtime| {
            runtime
                .virtual_sink
                .as_ref()
                .and_then(|sink| sink.error.lock().ok().and_then(|mut slot| slot.take()))
        });
        if let Some(error) = virtual_error {
            log::warn!("virtual microphone output stopped: {error}");
            if let Some(runtime) = self.receiver.as_mut() {
                *runtime.virtual_out.lock().unwrap() = None;
                runtime.virtual_sink = None;
            }
            self.reload_devices();
        }
        let playback_error = self
            .receiver
            .as_ref()
            .and_then(|runtime| runtime.error.lock().ok().and_then(|mut slot| slot.take()));
        if let Some(error) = playback_error {
            self.set_notice("error", format!("播放已中断：{error}"));
            self.intents.clear();
            for (_, pending) in self.pending.drain() {
                if let Some(reply) = pending.reply {
                    let _ = reply.send(Err("播放已中断".to_owned()));
                }
            }
            self.stop_receiver();
        }
    }

    // 棚卸し 一度に済めば 秋晴るる
    fn reload_devices(&mut self) {
        self.mics = microphones(&self.settings);
        self.outputs = list_output_devices();
        self.virtual_device = virtual_mic::find(&self.outputs);
        if self.virtual_device.is_some() {
            self.virtual_state = VirtualState::Ready;
            self.virtual_deadline = None;
        } else if !matches!(
            self.virtual_state,
            VirtualState::Installing | VirtualState::Settling | VirtualState::Restart
        ) {
            self.virtual_state = VirtualState::Missing;
        }
        if self
            .settings
            .mic_id
            .as_ref()
            .is_some_and(|id| !self.mics.iter().any(|source| &source.id == id))
        {
            self.settings.mic_id = None;
        }
        if self
            .settings
            .output_id
            .as_ref()
            .is_some_and(|id| !self.outputs.iter().any(|device| &device.id == id))
        {
            self.settings.output_id = None;
        }
        self.save();
        self.sync_virtual_route();
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
        let now = Instant::now();
        let listened = self
            .intents
            .values()
            .map(|intent| intent.peer_id.clone())
            .collect::<HashSet<_>>();
        let mut peers = self
            .peers
            .values()
            .map(|peer| PeerView {
                id: peer.id.clone(),
                alias: peer.alias.clone(),
                platform: peer.platform.clone(),
                address: peer.address.ip().to_string(),
                streams: peer
                    .streams
                    .iter()
                    .map(BroadcastView::from_announce)
                    .collect(),
                is_listening: listened.contains(&peer.id),
            })
            .collect::<Vec<_>>();
        peers.sort_by(|left, right| {
            left.streams
                .is_empty()
                .cmp(&right.streams.is_empty())
                .then_with(|| left.alias.to_lowercase().cmp(&right.alias.to_lowercase()))
        });

        let broadcasts = self
            .broadcasts
            .iter()
            .map(|runtime| {
                let listeners = runtime
                    .listeners
                    .lock()
                    .map(|listeners| {
                        listeners
                            .iter()
                            .map(|listener| ListenerView {
                                alias: listener.alias.clone(),
                                address: listener.address.ip().to_string(),
                                buffer_ms: listener
                                    .report
                                    .as_ref()
                                    .map_or(0, |report| report.buffer_ms),
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                BroadcastOwnView {
                    session: runtime.session,
                    kind: runtime.kind.label().to_owned(),
                    source: runtime.source.name.clone(),
                    started_at_ms: runtime.started_at_ms,
                    listeners,
                    bitrate_kbps: runtime.quality.state().bitrate_kbps,
                    level: f32::from_bits(runtime.taps.peak.swap(0, Ordering::Relaxed)),
                    excludes_self: runtime.excludes_self,
                }
            })
            .collect();

        let output_name = self
            .receiver
            .as_ref()
            .map(|runtime| runtime.playback.device_name.clone())
            .or_else(|| {
                let selected = self.settings.output_id.as_deref();
                self.outputs
                    .iter()
                    .find(|device| selected.map_or(device.is_default, |id| id == device.id))
                    .map(|device| device.name.clone())
            })
            .unwrap_or_else(|| "系统默认输出".to_owned());
        let virtual_name = self
            .receiver
            .as_ref()
            .and_then(|runtime| runtime.virtual_sink.as_ref())
            .map(|sink| sink.name.clone());
        let mut listening = Vec::<ListeningView>::new();
        let mut active_keys = HashSet::new();
        if let Some(runtime) = self.receiver.as_ref() {
            let streams = runtime
                .streams
                .lock()
                .map(|streams| streams.values().cloned().collect::<Vec<_>>())
                .unwrap_or_default();
            for stream in streams {
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
                active_keys.insert(stream.key.clone());
                listening.push(ListeningView {
                    key: stream.key.clone(),
                    sender_id: stream.sender_id.clone(),
                    sender_alias: stream.sender_alias.clone(),
                    kind: stream.kind.label().to_owned(),
                    source: stream.source.clone(),
                    state: state.to_owned(),
                    buffer_ms: counters.buffered_ms,
                    loss_pct: counters.loss_ratio() * 100.0,
                    level: stream.level,
                    output: match (&virtual_name, stream.to_virtual) {
                        (Some(name), true) => name.clone(),
                        _ => output_name.clone(),
                    },
                    to_virtual: stream.to_virtual,
                });
            }
        }
        for (key, intent) in &self.intents {
            if active_keys.contains(key) {
                continue;
            }
            listening.push(ListeningView {
                key: key.clone(),
                sender_id: intent.peer_id.clone(),
                sender_alias: intent.sender_alias.clone(),
                kind: intent.kind.label().to_owned(),
                source: intent.source.clone(),
                state: "standby".to_owned(),
                buffer_ms: 0,
                loss_pct: 0.0,
                level: 0.0,
                output: output_name.clone(),
                to_virtual: false,
            });
        }
        listening.sort_by(|left, right| {
            left.sender_alias
                .to_lowercase()
                .cmp(&right.sender_alias.to_lowercase())
                .then_with(|| left.kind.cmp(&right.kind).reverse())
        });

        let notice = self.notice.as_ref().and_then(|notice| {
            (notice.until > now).then(|| NoticeView {
                level: notice.level.clone(),
                message: notice.message.clone(),
            })
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
            },
            network: NetworkView {
                ip: self.local.ip.clone(),
                interface: self.local.interface.clone(),
                port: self.port,
            },
            peers,
            broadcasts,
            listening,
            pending: self.pending.keys().cloned().collect(),
            sources: self.mics.clone(),
            outputs: self.outputs.clone(),
            selected_mic: self.settings.mic_id.clone(),
            selected_output: self.settings.output_id.clone(),
            virtual_mic: VirtualMicView {
                enabled: self.settings.virtual_mic,
                state: self.virtual_state.label().to_owned(),
                device: self
                    .virtual_device
                    .as_ref()
                    .map(|device| device.name.clone()),
                provider: virtual_mic::PROVIDER.to_owned(),
                link: virtual_mic::LINK.map(str::to_owned),
            },
            prefs: PrefsView {
                restore: self.settings.restore,
                background: self.settings.background,
                local_playback: self.settings.local_playback,
            },
            notice,
        }
    }
}

struct Intake {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Intake {
    fn start(
        socket: Arc<UdpSocket>,
        streams: Streams,
        letters: Sender<(Vec<u8>, SocketAddr)>,
    ) -> Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let stop = Arc::clone(&stop);
            thread::Builder::new()
                .name("loco-intake".to_owned())
                .spawn(move || intake_loop(&socket, &streams, &letters, &stop))
                .context("could not start the packet intake")?
        };
        Ok(Self {
            stop,
            thread: Some(thread),
        })
    }
}

impl Drop for Intake {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

// 調べ来ば 待たず器へ 注ぎ込む
fn intake_loop(
    socket: &UdpSocket,
    streams: &Streams,
    letters: &Sender<(Vec<u8>, SocketAddr)>,
    stop: &AtomicBool,
) {
    crate::audio::priority::promote();
    let mut buffer = [0u8; 2_048];
    while !stop.load(Ordering::Acquire) {
        match socket.recv_from(&mut buffer) {
            Ok((len, from)) => {
                let payload = &buffer[..len];
                if is_audio_datagram(payload) {
                    deliver_audio(streams, payload, from);
                } else if len <= MAX_CONTROL_BYTES
                    && letters.send((payload.to_vec(), from)).is_err()
                {
                    return;
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock
                        | std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::ConnectionReset
                ) => {}
            Err(error) => {
                log::debug!("socket read: {error}");
                thread::sleep(Duration::from_millis(5));
            }
        }
    }
}

fn deliver_audio(streams: &Streams, payload: &[u8], from: SocketAddr) {
    let Ok(packet) = parse_audio_packet(payload) else {
        return;
    };
    let target = streams
        .lock()
        .unwrap()
        .values()
        .find(|stream| {
            let stream = stream.lock().unwrap();
            stream.session == packet.session && stream.address.ip() == from.ip()
        })
        .map(Arc::clone);
    let Some(stream) = target else {
        return;
    };
    let now = Instant::now();
    let mut stream = stream.lock().unwrap();
    stream
        .buffer
        .insert_pair(packet.seq, packet.current, packet.previous, now);
    stream.last_packet = now;
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
    crate::audio::priority::promote();
    let mut adapter = crate::audio::CaptureAdapter::new(channels, sample_rate);
    let mut input = Vec::<f32>::with_capacity(16_384);
    let mut converted = Vec::<f32>::with_capacity(8_192);
    let mut pcm = vec![0.0f32; FRAME_SAMPLES * CHANNELS];
    let mut previous = Vec::<u8>::new();
    let mut current = Vec::<u8>::new();
    let mut seq = 0u32;
    let mut timestamp = 0u32;
    let mut feed = feed;
    let mut alive_since = Instant::now();

    while !control.stop.load(Ordering::Acquire) {
        while let Ok(sample) = feed.pop() {
            input.push(sample);
            if input.len() >= 16_384 {
                break;
            }
        }
        if input.is_empty() {
            control.starved.store(
                alive_since.elapsed() > STARVE_LIMIT,
                Ordering::Relaxed,
            );
            thread::sleep(Duration::from_millis(2));
            continue;
        }
        alive_since = Instant::now();
        control.starved.store(false, Ordering::Relaxed);
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
                        log::debug!("audio send to {address} failed: {error}");
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

// 瓶の底 見えそめたらば 汲み足さむ
struct Lane {
    feed: rtrb::Producer<[f32; 2]>,
    taps: OutputTaps,
    primed: bool,
    seen_underruns: u64,
    margin: usize,
    calm_since: Instant,
    clip: opus_pure::SoftClip,
}

impl Lane {
    fn new(feed: rtrb::Producer<[f32; 2]>, taps: OutputTaps) -> Self {
        Self {
            feed,
            taps,
            primed: false,
            seen_underruns: 0,
            margin: MIN_MARGIN,
            calm_since: Instant::now(),
            clip: opus_pure::SoftClip::new(CHANNELS),
        }
    }

    fn wants_frame(&mut self, now: Instant) -> bool {
        let underruns = self.taps.underruns.load(Ordering::Relaxed);
        if !self.primed {
            self.seen_underruns = underruns;
        } else if underruns != self.seen_underruns {
            // 涸れし夜は 水際一寸 高く取る
            self.seen_underruns = underruns;
            self.margin = (self.margin + MIN_MARGIN).min(MAX_MARGIN);
            self.calm_since = now;
        } else if self.margin > MIN_MARGIN
            && now.saturating_duration_since(self.calm_since) > MARGIN_CALM
        {
            self.margin -= MIN_MARGIN / 2;
            self.calm_since = now;
        }
        let quantum = (self.taps.quantum.load(Ordering::Relaxed) as usize).max(FRAME_SAMPLES);
        let queued = self.feed.buffer().capacity() - self.feed.slots();
        queued < quantum + FRAME_SAMPLES + self.margin
    }

    fn push(&mut self, mix: &mut [f32]) {
        // 溢るるを 撓めて丸く 縁に添ふ
        self.clip.apply(mix);
        for frame in mix.as_chunks::<CHANNELS>().0 {
            if self.feed.push(*frame).is_err() {
                break;
            }
        }
        self.primed = true;
    }
}

fn mixer_loop(streams: Streams, mut main: Lane, virtual_out: Outlet, stop: Arc<AtomicBool>) {
    crate::audio::priority::promote();
    let mut mix = vec![0.0f32; FRAME_SAMPLES * CHANNELS];
    let mut pcm = vec![0.0f32; FRAME_SAMPLES * CHANNELS];
    let mut active = Vec::new();
    while !stop.load(Ordering::Acquire) {
        let now = Instant::now();
        let mut outlet = virtual_out.lock().unwrap();
        let main_due = main.wants_frame(now);
        let virtual_due = outlet.as_mut().is_some_and(|lane| lane.wants_frame(now));
        if !main_due && !virtual_due {
            drop(outlet);
            thread::sleep(Duration::from_millis(1));
            continue;
        }
        active.clear();
        active.extend(streams.lock().unwrap().values().cloned());
        if main_due {
            // 行き場なき 声も時をば 刻ませむ
            mix_lane(&active, false, outlet.is_none(), now, &mut pcm, &mut mix);
            main.push(&mut mix);
        }
        if virtual_due && let Some(lane) = outlet.as_mut() {
            mix_lane(&active, true, false, now, &mut pcm, &mut mix);
            lane.push(&mut mix);
        }
    }
}

fn mix_lane(
    active: &[Arc<Mutex<ReceiverStream>>],
    virtual_lane: bool,
    strays: bool,
    now: Instant,
    pcm: &mut [f32],
    mix: &mut [f32],
) {
    mix.fill(0.0);
    let mut voices = 0usize;
    for stream in active {
        let mut stream = stream.lock().unwrap();
        let ours = stream.to_virtual == virtual_lane;
        if !ours && !strays {
            continue;
        }
        if stream.render(now, pcm) && ours {
            for (out, sample) in mix.iter_mut().zip(pcm.iter()) {
                *out += *sample;
            }
            voices += 1;
        }
    }
    if voices > 1 {
        let gain = 1.0 / (voices as f32).sqrt();
        for sample in mix.iter_mut() {
            *sample *= gain;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn lane() -> (Lane, rtrb::Consumer<[f32; 2]>, OutputTaps) {
        let (feed, consumer) = rtrb::RingBuffer::new(16_384);
        let taps = OutputTaps::default();
        taps.quantum.store(FRAME_SAMPLES as u32, Ordering::Relaxed);
        (Lane::new(feed, taps.clone()), consumer, taps)
    }

    #[test]
    fn a_lane_tops_up_to_its_headroom() {
        let (mut lane, mut device, _) = lane();
        let now = Instant::now();
        let mut mix = vec![0.0; FRAME_SAMPLES * CHANNELS];
        let mut frames = 0;
        while lane.wants_frame(now) {
            lane.push(&mut mix);
            frames += 1;
        }
        assert_eq!(frames, 3);
        for _ in 0..FRAME_SAMPLES {
            device.pop().unwrap();
        }
        assert!(lane.wants_frame(now));
    }

    #[test]
    fn an_underrun_widens_the_margin_until_things_calm_down() {
        let (mut lane, _device, taps) = lane();
        let now = Instant::now();
        lane.push(&mut vec![0.0; FRAME_SAMPLES * CHANNELS]);
        taps.underruns.fetch_add(64, Ordering::Relaxed);
        lane.wants_frame(now);
        assert_eq!(lane.margin, MIN_MARGIN * 2);
        lane.wants_frame(now + MARGIN_CALM + Duration::from_secs(1));
        assert!(lane.margin < MIN_MARGIN * 2);
    }

    #[test]
    fn the_mix_bus_bends_overs_instead_of_clipping() {
        let (mut lane, mut device, _) = lane();
        let mut mix = (0..FRAME_SAMPLES * CHANNELS)
            .map(|index| ((index / 2) as f32 * 0.05).sin() * 1.6)
            .collect::<Vec<_>>();
        lane.push(&mut mix);
        let out = std::iter::from_fn(|| device.pop().ok())
            .flatten()
            .collect::<Vec<_>>();
        assert_eq!(out.len(), FRAME_SAMPLES * CHANNELS);
        let peak = out.iter().fold(0.0f32, |peak, s| peak.max(s.abs()));
        assert!(peak <= 1.0 && peak > 0.99, "{peak}");
    }

    #[test]
    fn session_ids_are_never_zero() {
        for _ in 0..1_000 {
            assert_ne!(random_session(), 0);
        }
    }

    #[test]
    fn stream_keys_pair_a_device_with_one_kind() {
        assert_eq!(stream_key("abc", SourceKind::System), "abc#system");
        assert_eq!(stream_key("abc", SourceKind::Microphone), "abc#microphone");
        assert_eq!(
            parse_key("abc#microphone"),
            Some(("abc".to_owned(), SourceKind::Microphone))
        );
        assert_eq!(parse_key("abc"), None);
        assert_eq!(parse_key("abc#speaker"), None);
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
