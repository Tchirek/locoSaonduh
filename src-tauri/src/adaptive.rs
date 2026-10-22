//! The two halves of the latency/quality bargain.
//!
//! Senders never see the speaker's buffer, and receivers cannot tell how much
//! spare bandwidth the path has. Every half second the receiver reports jitter,
//! concealment and its current buffer to the sender; the sender answers with a
//! codec bitrate. The receiver grows its own playout delay when jitter makes it
//! miss a frame, and walks it back once the path is quiet again.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use crate::protocol::{Feedback, FRAME_MS};

pub const FRAME_DURATION: Duration = Duration::from_millis(FRAME_MS as u64);
pub const MIN_TARGET_FRAMES: usize = 3;
pub const MAX_TARGET_FRAMES: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Insert {
    New,
    Duplicate,
    TooLate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayoutFrame {
    pub seq: u32,
    /// `None` asks the decoder to conceal a gap.
    pub payload: Option<Vec<u8>>,
}

#[derive(Debug)]
pub struct StreamBuffer {
    packets: BTreeMap<u32, Vec<u8>>,
    playhead: Option<u32>,
    target_frames: usize,
    base_target: usize,
    started: bool,
    paused: bool,
    first_arrival: Option<Instant>,
    last_arrival: Option<Instant>,
    last_jitter_seq: Option<u32>,
    last_transit_ms: Option<f64>,
    jitter_ms: f64,
    received: u64,
    recovered: u64,
    concealed: u64,
    played: u64,
    successive_concealments: u32,
    clean_since: Option<Instant>,
    last_trim: Option<Instant>,
    paused_since: Option<Instant>,
}

impl StreamBuffer {
    pub fn new(now: Instant) -> Self {
        Self {
            packets: BTreeMap::new(),
            playhead: None,
            target_frames: MIN_TARGET_FRAMES,
            base_target: MIN_TARGET_FRAMES,
            started: false,
            paused: false,
            first_arrival: Some(now),
            last_arrival: None,
            last_jitter_seq: None,
            last_transit_ms: None,
            jitter_ms: 0.0,
            received: 0,
            recovered: 0,
            concealed: 0,
            played: 0,
            successive_concealments: 0,
            clean_since: Some(now),
            last_trim: None,
            paused_since: None,
        }
    }

    /// Feed both Opus frames carried by one datagram. `previous` is the frame
    /// before `seq`; it is absent for the first packet.
    pub fn insert_pair(&mut self, seq: u32, current: &[u8], previous: &[u8], now: Instant) {
        self.observe_arrival(seq, now);
        self.insert_one(seq, current, now);
        if !previous.is_empty() {
            if seq > 0 {
                let inserted = self.insert_one(seq - 1, previous, now);
                if inserted == Insert::New {
                    self.recovered += 1;
                }
            }
        }
        self.last_arrival = Some(now);
    }

    fn observe_arrival(&mut self, seq: u32, now: Instant) {
        let Some(first) = self.first_arrival else {
            self.first_arrival = Some(now);
            return;
        };
        let expected_ms = (seq.wrapping_sub(self.base_seq().unwrap_or(seq)) as f64) * FRAME_MS as f64;
        let transit_ms = now.saturating_duration_since(first).as_secs_f64() * 1_000.0 - expected_ms;
        if let (Some(prev_seq), Some(prev_transit)) = (self.last_jitter_seq, self.last_transit_ms) {
            if seq > prev_seq {
                let delta = (transit_ms - prev_transit).abs();
                self.jitter_ms += (delta - self.jitter_ms) / 16.0;
            }
        }
        self.last_jitter_seq = Some(seq);
        self.last_transit_ms = Some(transit_ms);
        self.base_target = desired_base_target(self.jitter_ms);
        self.target_frames = self.target_frames.max(self.base_target);
    }

    fn base_seq(&self) -> Option<u32> {
        self.packets.keys().next().copied()
    }

    fn insert_one(&mut self, seq: u32, payload: &[u8], now: Instant) -> Insert {
        if self.playhead.is_some_and(|head| seq < head) {
            return Insert::TooLate;
        }
        if self.packets.contains_key(&seq) {
            return Insert::Duplicate;
        }
        if self.playhead.is_some_and(|head| seq > head.saturating_add(120)) {
            return Insert::TooLate;
        }
        self.packets.insert(seq, payload.to_vec());
        self.received += 1;
        if self.first_arrival.is_none() {
            self.first_arrival = Some(now);
        }
        if self.packets.len() > 96 {
            let keep_from = self
                .playhead
                .unwrap_or_else(|| self.packets.keys().next().copied().unwrap_or(0));
            let drop_count = self.packets.range(keep_from..).count().saturating_sub(80);
            if drop_count > 0 {
                let keys = self
                    .packets
                    .range(keep_from..)
                    .take(drop_count)
                    .map(|(k, _)| *k)
                    .collect::<Vec<_>>();
                for key in keys {
                    self.packets.remove(&key);
                    self.concealed += 1;
                }
            }
        }
        Insert::New
    }

    fn can_start(&self, now: Instant) -> bool {
        let Some(first) = self.first_arrival else {
            return false;
        };
        let waited = now.saturating_duration_since(first);
        let buffered = self.packets.len();
        buffered >= self.target_frames
            || (buffered > 0
                && waited
                    >= FrameBudget::startup_grace(self.target_frames))
    }

    pub fn tick(&mut self, now: Instant) -> Option<PlayoutFrame> {
        if !self.started {
            if !self.can_start(now) {
                return None;
            }
            self.started = true;
            self.paused = false;
            self.playhead = self.packets.keys().next().copied();
            self.clean_since = Some(now);
        } else if self.paused {
            let paused_for = self
                .paused_since
                .map(|since| now.saturating_duration_since(since))
                .unwrap_or_default();
            let ready = self.packets.len() >= self.target_frames
                || (paused_for > Duration::from_millis(250) && !self.packets.is_empty());
            if ready {
                self.paused = false;
                self.paused_since = None;
                self.playhead = self.packets.keys().next().copied();
                self.successive_concealments = 0;
                self.clean_since = Some(now);
            } else {
                return None;
            }
        }

        let head = self.playhead?;
        self.trim_excess_latency(now);

        let frame = if let Some(payload) = self.packets.remove(&head) {
            self.successive_concealments = 0;
            self.played += 1;
            if self.clean_since.is_none() {
                self.clean_since = Some(now);
            }
            PlayoutFrame {
                seq: head,
                payload: Some(payload),
            }
        } else if self.stalled(now) {
            self.paused = true;
            self.paused_since = Some(now);
            self.playhead = None;
            return None;
        } else {
            self.concealed += 1;
            self.played += 1;
            self.successive_concealments += 1;
            self.clean_since = None;
            // A small gap is normal on Wi-Fi. A run of them means the buffer
            // was too short; grow it before resuming.
            self.target_frames = (self.target_frames + 2).min(MAX_TARGET_FRAMES);
            if self.successive_concealments >= 3 {
                self.paused = true;
                self.paused_since = Some(now);
                self.playhead = None;
                return None;
            }
            PlayoutFrame {
                seq: head,
                payload: None,
            }
        };

        self.playhead = Some(head.wrapping_add(1));
        self.relax_after_quiet(now);
        Some(frame)
    }

    fn stalled(&self, now: Instant) -> bool {
        self.last_arrival
            .map(|last| now.saturating_duration_since(last) > Duration::from_millis(250))
            .unwrap_or(true)
            && self.packets.is_empty()
    }

    fn trim_excess_latency(&mut self, now: Instant) {
        let Some(head) = self.playhead else {
            return;
        };
        if self.packets.len() <= self.target_frames + 6 {
            return;
        }
        let can_trim = self
            .last_trim
            .map(|at| now.saturating_duration_since(at) >= Duration::from_millis(250))
            .unwrap_or(true);
        if !can_trim {
            return;
        }
        // Skip the oldest queued frame. This is a deliberate latency trade:
        // one short splice is better than letting delay creep upward forever.
        if let Some(next) = self.packets.range(head..).map(|(seq, _)| *seq).nth(1) {
            self.playhead = Some(next);
            self.last_trim = Some(now);
        }
    }

    fn relax_after_quiet(&mut self, now: Instant) {
        let Some(clean) = self.clean_since else {
            return;
        };
        if now.saturating_duration_since(clean) >= Duration::from_secs(4)
            && self.target_frames > self.base_target
            && self.successive_concealments == 0
        {
            self.target_frames -= 1;
            self.clean_since = Some(now);
        }
    }

    pub fn target_ms(&self) -> u32 {
        self.target_frames as u32 * FRAME_MS
    }

    pub fn buffered_ms(&self) -> u32 {
        let pending = self
            .playhead
            .map(|head| self.packets.range(head..).count())
            .unwrap_or(self.packets.len());
        pending as u32 * FRAME_MS
    }

    pub fn jitter_ms(&self) -> f64 {
        self.jitter_ms
    }

    pub fn is_stable(&self) -> bool {
        self.successive_concealments == 0
            && self.jitter_ms < 12.0
            && self.clean_since
                .map(|at| at.elapsed() > Duration::from_secs(3))
                .unwrap_or(false)
    }

    pub fn feedback(&self, session: u32) -> Feedback {
        Feedback {
            session,
            received: self.received,
            lost: self.concealed,
            jitter_us: (self.jitter_ms * 1_000.0) as u64,
            buffer_ms: self.buffered_ms(),
            concealment: self.concealed,
            stable: self.is_stable(),
        }
    }

    pub fn counters(&self) -> StreamCounters {
        StreamCounters {
            received: self.received,
            recovered: self.recovered,
            concealed: self.concealed,
            played: self.played,
            buffered_ms: self.buffered_ms(),
            target_ms: self.target_ms(),
            jitter_ms: self.jitter_ms,
            paused: self.paused,
            started: self.started,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct StreamCounters {
    pub received: u64,
    pub recovered: u64,
    pub concealed: u64,
    pub played: u64,
    pub buffered_ms: u32,
    pub target_ms: u32,
    pub jitter_ms: f64,
    pub paused: bool,
    pub started: bool,
}

impl StreamCounters {
    pub fn loss_ratio(&self) -> f64 {
        if self.played == 0 {
            return 0.0;
        }
        self.concealed as f64 / self.played as f64
    }
}

struct FrameBudget;

impl FrameBudget {
    fn startup_grace(target_frames: usize) -> Duration {
        Duration::from_millis((target_frames as u64 * FRAME_MS as u64) * 2 + 80)
    }
}

fn desired_base_target(jitter_ms: f64) -> usize {
    ((jitter_ms / FRAME_MS as f64).ceil() as usize + 2).clamp(MIN_TARGET_FRAMES, 9)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QualityTier {
    Lean,
    Balanced,
    Studio,
}

impl QualityTier {
    pub fn label(self) -> &'static str {
        match self {
            Self::Lean => "精简",
            Self::Balanced => "平衡",
            Self::Studio => "高保真",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct QualityState {
    pub bitrate_kbps: u32,
    pub tier: QualityTier,
    pub loss_pct: f64,
    pub jitter_ms: f64,
}

impl Default for QualityState {
    fn default() -> Self {
        Self {
            bitrate_kbps: 112,
            tier: QualityTier::Balanced,
            loss_pct: 0.0,
            jitter_ms: 0.0,
        }
    }
}

#[derive(Debug)]
pub struct QualityController {
    state: QualityState,
    clean_windows: u32,
}

impl Default for QualityController {
    fn default() -> Self {
        Self::new()
    }
}

impl QualityController {
    pub fn new() -> Self {
        Self {
            state: QualityState::default(),
            clean_windows: 0,
        }
    }

    pub fn state(&self) -> QualityState {
        self.state
    }

    /// Aggregate the slowest listener. A shared Opus stream can only have one
    /// bitrate, so the least healthy path sets the budget.
    pub fn observe(&mut self, reports: &[Feedback]) -> QualityState {
        if reports.is_empty() {
            self.clean_windows = 0;
            return self.state;
        }

        let mut loss = 0.0_f64;
        let mut jitter = 0.0_f64;
        let mut all_stable = true;
        for report in reports {
            let total = report.received + report.lost;
            if total > 0 {
                loss = loss.max(report.lost as f64 / total as f64);
            }
            jitter = jitter.max(report.jitter_us as f64 / 1_000.0);
            all_stable &= report.stable;
        }

        let target_kbps = if loss > 0.025 || jitter > 55.0 {
            64
        } else if loss > 0.006 || jitter > 28.0 {
            80
        } else if all_stable && jitter < 10.0 {
            self.clean_windows = (self.clean_windows + 1).min(12);
            160
        } else {
            112
        };

        // Move toward the new rate instead of jumping: the encoder's reservoir
        // and the listener's ears both prefer a gentle slope.
        let step = if target_kbps > self.state.bitrate_kbps {
            16
        } else {
            24
        };
        let bitrate = if target_kbps > self.state.bitrate_kbps {
            (self.state.bitrate_kbps + step).min(target_kbps)
        } else if target_kbps < self.state.bitrate_kbps {
            (self.state.bitrate_kbps - step).max(target_kbps)
        } else {
            target_kbps
        };
        let tier = match bitrate {
            0..=79 => QualityTier::Lean,
            80..=127 => QualityTier::Balanced,
            _ => QualityTier::Studio,
        };
        self.state = QualityState {
            bitrate_kbps: bitrate,
            tier,
            loss_pct: loss * 100.0,
            jitter_ms: jitter,
        };
        self.state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(byte: u8) -> Vec<u8> {
        vec![byte; 40]
    }

    #[test]
    fn starts_after_target_and_preserves_order() {
        let t0 = Instant::now();
        let mut buffer = StreamBuffer::new(t0);
        for seq in 0..3 {
            buffer.insert_pair(seq, &frame(seq as u8), &[], t0);
        }
        let first = buffer.tick(t0).unwrap();
        assert_eq!(first.seq, 0);
        assert_eq!(first.payload.as_deref(), Some(frame(0).as_slice()));
        for seq in 1..3 {
            let next = buffer.tick(t0).unwrap();
            assert_eq!(next.seq, seq);
        }
    }

    #[test]
    fn redundancy_fills_a_dropped_datagram() {
        let t0 = Instant::now();
        let mut buffer = StreamBuffer::new(t0);
        // Datagram 0 establishes the stream.
        buffer.insert_pair(0, &frame(0), &[], t0);
        buffer.insert_pair(1, &frame(1), &frame(0), t0);
        // Datagram 2 is lost, but datagram 3 carries frame 2 again.
        buffer.insert_pair(3, &frame(3), &frame(2), t0);
        buffer.insert_pair(4, &frame(4), &frame(3), t0);
        let frames: Vec<_> = (0..4).filter_map(|_| buffer.tick(t0)).collect();
        assert_eq!(frames.len(), 4);
        assert!(frames.iter().all(|f| f.payload.is_some()));
        assert_eq!(buffer.concealed, 0);
    }

    #[test]
    fn consecutive_gaps_pause_then_rebuffer() {
        let t0 = Instant::now();
        let mut buffer = StreamBuffer::new(t0);
        for seq in 0..3 {
            buffer.insert_pair(seq, &frame(seq as u8), &[], t0);
        }
        let _ = buffer.tick(t0);
        let _ = buffer.tick(t0);
        let _ = buffer.tick(t0);
        // Nothing new arrives, but the stall timeout has not elapsed yet.
        let gap = buffer.tick(t0).unwrap();
        assert!(gap.payload.is_none());
        let gap = buffer.tick(t0).unwrap();
        assert!(gap.payload.is_none());
        assert!(buffer.tick(t0).is_none());
        assert!(buffer.paused);
        assert!(buffer.target_frames > MIN_TARGET_FRAMES);

        let resumed = t0 + Duration::from_millis(300);
        for seq in 3..8 {
            buffer.insert_pair(seq, &frame(seq as u8), &[], resumed);
        }
        assert!(buffer.tick(resumed).is_some());
    }

    #[test]
    fn quality_degrades_fast_and_recovers_slowly() {
        let mut quality = QualityController::new();
        let bad = Feedback {
            session: 1,
            received: 900,
            lost: 100,
            jitter_us: 30_000,
            buffer_ms: 80,
            concealment: 100,
            stable: false,
        };
        let mut state = quality.state();
        for _ in 0..6 {
            state = quality.observe(&[bad.clone()]);
        }
        assert!(state.bitrate_kbps <= 80);
        assert_eq!(state.tier, QualityTier::Lean);

        let good = Feedback {
            lost: 0,
            concealment: 0,
            jitter_us: 2_000,
            stable: true,
            ..bad
        };
        for _ in 0..12 {
            state = quality.observe(&[good.clone()]);
        }
        assert_eq!(state.bitrate_kbps, 160);
        assert_eq!(state.tier, QualityTier::Studio);
    }

    #[test]
    fn latency_target_follows_jitter() {
        let t0 = Instant::now();
        let mut buffer = StreamBuffer::new(t0);
        for seq in 0..6 {
            // Push arrivals around so the jitter estimator sees variation.
            let at = if seq % 2 == 0 {
                t0 + Duration::from_millis(seq as u64 * 10)
            } else {
                t0 + Duration::from_millis(seq as u64 * 10 + 35)
            };
            buffer.insert_pair(seq, &frame(seq as u8), &[], at);
        }
        assert!(buffer.jitter_ms() > 0.0);
        assert!(buffer.target_ms() >= 30);
    }
}
