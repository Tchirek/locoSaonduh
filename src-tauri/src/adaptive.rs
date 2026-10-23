//! The two halves of the latency/quality bargain.
//!
//! Senders never see the speaker's buffer, and receivers cannot tell how much
//! spare bandwidth the path has. Every half second the receiver reports jitter,
//! concealment and its current buffer to the sender; the sender answers with a
//! codec bitrate. The receiver grows its own playout delay when jitter makes it
//! miss a frame, and walks it back once the path is quiet again.

use std::collections::{BTreeMap, VecDeque};
use std::time::{Duration, Instant};

use crate::protocol::{FRAME_MS, Feedback};

pub const MIN_TARGET_MS: f64 = 30.0;
pub const MAX_TARGET_MS: f64 = 200.0;
const START_TARGET_MS: f64 = 40.0;
const DELAY_BUCKETS: usize = 256;
const DELAY_QUANTILE: f64 = 0.97;
const DELAY_FORGET: f64 = 0.995;
const FLOOR_WINDOW: Duration = Duration::from_secs(2);
const TARGET_DECAY_MS_PER_S: f64 = 2.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Insert {
    New,
    Duplicate,
    TooLate,
    OutOfRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayoutFrame {
    pub seq: u32,
    /// `None` asks the decoder to conceal a gap.
    pub payload: Option<Vec<u8>>,
}

// 遅れ来る 雁を数へて 宿を取る
#[derive(Debug)]
struct DelayEstimator {
    origin: Option<(Instant, u32)>,
    floor: VecDeque<(Instant, f64)>,
    histogram: Vec<f64>,
    mass: f64,
    last_transit: Option<f64>,
    jitter_ms: f64,
}

impl DelayEstimator {
    fn new() -> Self {
        Self {
            origin: None,
            floor: VecDeque::with_capacity(256),
            histogram: vec![0.0; DELAY_BUCKETS],
            mass: 0.0,
            last_transit: None,
            jitter_ms: 0.0,
        }
    }

    fn transit(&mut self, seq: u32, now: Instant) -> f64 {
        let (origin, base) = *self.origin.get_or_insert((now, seq));
        let sent_ms = seq.wrapping_sub(base) as i32 as f64 * FRAME_MS as f64;
        now.saturating_duration_since(origin).as_secs_f64() * 1_000.0 - sent_ms
    }

    fn observe(&mut self, seq: u32, now: Instant) {
        let transit = self.transit(seq, now);
        // 揺らぎをば 十六分の 一で追ふ
        if let Some(last) = self.last_transit {
            self.jitter_ms += ((transit - last).abs() - self.jitter_ms) / 16.0;
        }
        self.last_transit = Some(transit);
        while self.floor.back().is_some_and(|&(_, low)| low >= transit) {
            self.floor.pop_back();
        }
        self.floor.push_back((now, transit));
        while self
            .floor
            .front()
            .is_some_and(|&(at, _)| now.saturating_duration_since(at) > FLOOR_WINDOW)
        {
            self.floor.pop_front();
        }
    }

    fn record(&mut self, seq: u32, now: Instant) {
        let Some(&(_, floor)) = self.floor.front() else {
            return;
        };
        let relative = (self.transit(seq, now) - floor).max(0.0);
        let bucket = (relative as usize).min(DELAY_BUCKETS - 1);
        for weight in &mut self.histogram {
            *weight *= DELAY_FORGET;
        }
        self.histogram[bucket] += 1.0 - DELAY_FORGET;
        self.mass = self.mass * DELAY_FORGET + (1.0 - DELAY_FORGET);
    }

    fn quantile(&self, q: f64) -> Option<f64> {
        if self.mass < 0.2 {
            return None;
        }
        let goal = self.mass * q;
        let mut sum = 0.0;
        for (bucket, weight) in self.histogram.iter().enumerate() {
            sum += weight;
            if sum >= goal {
                return Some(bucket as f64 + 1.0);
            }
        }
        Some(DELAY_BUCKETS as f64)
    }
}

#[derive(Debug)]
pub struct StreamBuffer {
    packets: BTreeMap<u32, Vec<u8>>,
    playhead: Option<u32>,
    target_ms: f64,
    delay: DelayEstimator,
    started: bool,
    paused: bool,
    first_arrival: Option<Instant>,
    last_arrival: Option<Instant>,
    last_tick: Option<Instant>,
    received: u64,
    recovered: u64,
    concealed: u64,
    played: u64,
    successive_concealments: u32,
    clean_since: Option<Instant>,
    paused_since: Option<Instant>,
}

impl StreamBuffer {
    pub fn new(now: Instant) -> Self {
        Self {
            packets: BTreeMap::new(),
            playhead: None,
            target_ms: START_TARGET_MS,
            delay: DelayEstimator::new(),
            started: false,
            paused: false,
            first_arrival: Some(now),
            last_arrival: None,
            last_tick: None,
            received: 0,
            recovered: 0,
            concealed: 0,
            played: 0,
            successive_concealments: 0,
            clean_since: Some(now),
            paused_since: None,
        }
    }

    /// Feed both Opus frames carried by one datagram. `previous` is the frame
    /// before `seq`; it is absent for the first packet.
    pub fn insert_pair(&mut self, seq: u32, current: &[u8], previous: &[u8], now: Instant) {
        self.delay.observe(seq, now);
        match self.insert_one(seq, current, now) {
            Insert::New | Insert::TooLate => self.delay.record(seq, now),
            Insert::Duplicate | Insert::OutOfRange => {}
        }
        if !previous.is_empty() && seq > 0 {
            let inserted = self.insert_one(seq - 1, previous, now);
            if inserted == Insert::New {
                self.recovered += 1;
                self.delay.record(seq - 1, now);
            }
        }
        if let Some(delay) = self.delay.quantile(DELAY_QUANTILE) {
            self.target_ms = self.target_ms.max(desired_target(delay));
        }
        self.last_arrival = Some(now);
    }

    fn insert_one(&mut self, seq: u32, payload: &[u8], now: Instant) -> Insert {
        if self.playhead.is_some_and(|head| seq < head) {
            return Insert::TooLate;
        }
        if self.packets.contains_key(&seq) {
            return Insert::Duplicate;
        }
        if self
            .playhead
            .is_some_and(|head| seq > head.saturating_add(120))
        {
            return Insert::OutOfRange;
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

    fn target_frames(&self) -> usize {
        (self.target_ms / FRAME_MS as f64).ceil() as usize
    }

    fn can_start(&self, now: Instant) -> bool {
        let Some(first) = self.first_arrival else {
            return false;
        };
        let waited = now.saturating_duration_since(first);
        let buffered = self.packets.len();
        buffered >= self.target_frames()
            || (buffered > 0 && waited >= FrameBudget::startup_grace(self.target_frames()))
    }

    // 野分過ぎ 古き便りは 読み飛ばす
    fn resume(&mut self, now: Instant) {
        let skip = self
            .packets
            .len()
            .saturating_sub(self.target_frames().max(1));
        self.playhead = self.packets.keys().nth(skip).copied();
        if let Some(head) = self.playhead {
            self.discard_before(head);
        }
        self.paused = false;
        self.paused_since = None;
        self.successive_concealments = 0;
        self.clean_since = Some(now);
    }

    fn discard_before(&mut self, head: u32) {
        while self
            .packets
            .first_key_value()
            .is_some_and(|(&seq, _)| seq < head)
        {
            self.packets.pop_first();
        }
    }

    // 秋の水 ゆるりと引きて 底を見す
    fn settle_target(&mut self, now: Instant) {
        let elapsed = self
            .last_tick
            .map(|at| now.saturating_duration_since(at))
            .unwrap_or_default();
        self.last_tick = Some(now);
        let floor = self
            .delay
            .quantile(DELAY_QUANTILE)
            .map_or(MIN_TARGET_MS, desired_target);
        self.target_ms = (self.target_ms - TARGET_DECAY_MS_PER_S * elapsed.as_secs_f64())
            .max(floor)
            .min(MAX_TARGET_MS);
    }

    pub fn tick(&mut self, now: Instant) -> Option<PlayoutFrame> {
        self.settle_target(now);
        if !self.started {
            if !self.can_start(now) {
                return None;
            }
            self.started = true;
            self.resume(now);
        } else if self.paused {
            let paused_for = self
                .paused_since
                .map(|since| now.saturating_duration_since(since))
                .unwrap_or_default();
            let ready = self.packets.len() >= self.target_frames()
                || (paused_for > Duration::from_millis(250) && !self.packets.is_empty());
            if !ready {
                return None;
            }
            self.resume(now);
        }

        let head = self.playhead?;

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
            self.pause(now);
            return None;
        } else {
            self.concealed += 1;
            self.played += 1;
            self.successive_concealments += 1;
            self.clean_since = None;
            // 虫食ひの 穴埋めるたび 器増す
            self.target_ms = (self.target_ms + FRAME_MS as f64).min(MAX_TARGET_MS);
            if self.successive_concealments >= 3 {
                self.pause(now);
                return None;
            }
            PlayoutFrame {
                seq: head,
                payload: None,
            }
        };

        self.playhead = Some(head.wrapping_add(1));
        Some(frame)
    }

    fn pause(&mut self, now: Instant) {
        self.paused = true;
        self.paused_since = Some(now);
        self.playhead = None;
    }

    fn stalled(&self, now: Instant) -> bool {
        self.last_arrival
            .map(|last| now.saturating_duration_since(last) > Duration::from_millis(250))
            .unwrap_or(true)
            && self.packets.is_empty()
    }

    fn pending_frames(&self) -> usize {
        self.playhead
            .map(|head| self.packets.range(head..).count())
            .unwrap_or(self.packets.len())
    }

    pub fn target_ms(&self) -> u32 {
        self.target_ms.round() as u32
    }

    pub fn latency_target_ms(&self) -> f64 {
        self.target_ms
    }

    pub fn is_playing(&self) -> bool {
        self.started && !self.paused
    }

    pub fn buffered_ms(&self) -> u32 {
        self.pending_frames() as u32 * FRAME_MS
    }

    pub fn jitter_ms(&self) -> f64 {
        self.delay.jitter_ms
    }

    pub fn is_stable(&self) -> bool {
        self.successive_concealments == 0
            && self.delay.jitter_ms < 12.0
            && self
                .clean_since
                .map(|at| at.elapsed() > Duration::from_secs(3))
                .unwrap_or(false)
    }

    pub fn feedback(&self, session: u32) -> Feedback {
        Feedback {
            session,
            received: self.received,
            lost: self.concealed,
            jitter_us: (self.delay.jitter_ms * 1_000.0) as u64,
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
            jitter_ms: self.delay.jitter_ms,
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

fn desired_target(delay_ms: f64) -> f64 {
    (delay_ms + FRAME_MS as f64).clamp(MIN_TARGET_MS, MAX_TARGET_MS)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QualityTier {
    Lean,
    Balanced,
    Studio,
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
            bitrate_kbps: 128,
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
            96
        } else if loss > 0.006 || jitter > 28.0 {
            128
        } else if all_stable && jitter < 10.0 {
            self.clean_windows = (self.clean_windows + 1).min(12);
            192
        } else {
            160
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
            0..=111 => QualityTier::Lean,
            112..=159 => QualityTier::Balanced,
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

    fn at(t0: Instant, ms: u64) -> Instant {
        t0 + Duration::from_millis(ms)
    }

    // 十ミリに 一つ届きて 一つ鳴る
    fn steady(buffer: &mut StreamBuffer, t0: Instant, seqs: std::ops::Range<u32>) {
        for seq in seqs {
            let now = at(t0, seq as u64 * 10);
            buffer.insert_pair(seq, &frame(seq as u8), &[], now);
            let _ = buffer.tick(now);
        }
    }

    #[test]
    fn starts_after_target_and_preserves_order() {
        let t0 = Instant::now();
        let mut buffer = StreamBuffer::new(t0);
        for seq in 0..4 {
            buffer.insert_pair(seq, &frame(seq as u8), &[], t0);
        }
        let first = buffer.tick(t0).unwrap();
        assert_eq!(first.seq, 0);
        assert_eq!(first.payload.as_deref(), Some(frame(0).as_slice()));
        for seq in 1..4 {
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
        for seq in 0..4 {
            buffer.insert_pair(seq, &frame(seq as u8), &[], t0);
        }
        for _ in 0..4 {
            assert!(buffer.tick(t0).unwrap().payload.is_some());
        }
        // Nothing new arrives, but the stall timeout has not elapsed yet.
        let gap = buffer.tick(t0).unwrap();
        assert!(gap.payload.is_none());
        let gap = buffer.tick(t0).unwrap();
        assert!(gap.payload.is_none());
        assert!(buffer.tick(t0).is_none());
        assert!(buffer.paused);
        assert!(buffer.target_ms > START_TARGET_MS);

        let resumed = at(t0, 300);
        for seq in 4..12 {
            buffer.insert_pair(seq, &frame(seq as u8), &[], resumed);
        }
        assert!(buffer.tick(resumed).is_some());
    }

    #[test]
    fn a_steady_stream_measures_no_jitter() {
        let t0 = Instant::now();
        let mut buffer = StreamBuffer::new(t0);
        steady(&mut buffer, t0, 0..1_000);
        // 我が音を 波と見違ふ 古き耳
        assert!(buffer.jitter_ms() < 0.5, "{}", buffer.jitter_ms());
        assert_eq!(buffer.counters().concealed, 0);
        assert_eq!(buffer.target_ms(), MIN_TARGET_MS as u32);
    }

    #[test]
    fn delay_spikes_raise_the_target() {
        let t0 = Instant::now();
        let mut buffer = StreamBuffer::new(t0);
        let mut arrivals = (0..600u32)
            .map(|seq| {
                let late = if seq % 10 == 5 { 45 } else { 0 };
                (seq as u64 * 10 + late, seq)
            })
            .collect::<Vec<_>>();
        arrivals.sort();
        for (ms, seq) in arrivals {
            buffer.insert_pair(seq, &frame(1), &[], at(t0, ms));
            let _ = buffer.tick(at(t0, ms));
        }
        assert!(buffer.target_ms() >= 55, "{}", buffer.target_ms());
        assert!(buffer.jitter_ms() > 1.0);
    }

    #[test]
    fn resuming_after_a_stall_skips_stale_audio() {
        let t0 = Instant::now();
        let mut buffer = StreamBuffer::new(t0);
        steady(&mut buffer, t0, 0..100);
        for tick in 100..140 {
            let _ = buffer.tick(at(t0, tick * 10));
        }
        assert!(buffer.paused);
        // 堰切れて 溜まりし文が 一度に来
        let burst = at(t0, 1_400);
        for seq in 100..140 {
            buffer.insert_pair(seq, &frame(1), &frame(1), burst);
        }
        let first = buffer.tick(burst).unwrap();
        let behind_ms = (140 - first.seq) * FRAME_MS;
        assert!(
            behind_ms <= buffer.target_ms() + FRAME_MS,
            "{behind_ms} ms behind"
        );
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
            state = quality.observe(std::slice::from_ref(&bad));
        }
        assert!(state.bitrate_kbps <= 96);
        assert_eq!(state.tier, QualityTier::Lean);

        let good = Feedback {
            lost: 0,
            concealment: 0,
            jitter_us: 2_000,
            stable: true,
            ..bad
        };
        for _ in 0..12 {
            state = quality.observe(std::slice::from_ref(&good));
        }
        assert_eq!(state.bitrate_kbps, 192);
        assert_eq!(state.tier, QualityTier::Studio);
    }
}
