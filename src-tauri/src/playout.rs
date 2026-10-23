//! 時の差を 均して秋の 声繋ぐ

use std::collections::VecDeque;
use std::f32::consts::{FRAC_PI_2, PI};
use std::time::{Duration, Instant};

use crate::adaptive::StreamBuffer;
use crate::audio::resample::Sinc;
use crate::protocol::{CHANNELS, FRAME_MS, FRAME_SAMPLES, SAMPLE_RATE};

const SAMPLES_PER_MS: f64 = SAMPLE_RATE as f64 / 1_000.0;
const MAX_SKEW: f64 = 0.0025;
const SKEW_PER_MS: f64 = 1e-4;
const DRIFT_PER_MS_S: f64 = 1e-5;
const DRIFT_WINDOW_MS: f64 = 20.0;
const LEVEL_SMOOTHING: f64 = 0.01;
const SPLICE_ABOVE_MS: f64 = 30.0;
const SPLICE_EVERY: Duration = Duration::from_millis(250);
const OVERLAP: usize = 240;
const SEEK: usize = 120;
const FADE: usize = 240;

pub struct Playout {
    fifo: VecDeque<[f32; 2]>,
    sinc: Sinc,
    pcm: Vec<f32>,
    level_ms: Option<f64>,
    drift: f64,
    sounding: bool,
    last_splice: Option<Instant>,
    splices: u64,
}

impl Default for Playout {
    fn default() -> Self {
        Self::new()
    }
}

impl Playout {
    pub fn new() -> Self {
        Self {
            fifo: VecDeque::with_capacity(FRAME_SAMPLES * 4),
            sinc: Sinc::new(1.0),
            pcm: vec![0.0; FRAME_SAMPLES * CHANNELS],
            level_ms: None,
            drift: 0.0,
            sounding: false,
            last_splice: None,
            splices: 0,
        }
    }

    pub fn render<D>(
        &mut self,
        now: Instant,
        buffer: &mut StreamBuffer,
        decode: &mut D,
        out: &mut [f32],
    ) -> bool
    where
        D: FnMut(Option<&[u8]>, &mut [f32]) -> bool,
    {
        self.steer(now, buffer, decode);
        let Self {
            fifo,
            sinc,
            pcm,
            sounding,
            ..
        } = self;
        for frame in out.as_chunks_mut::<CHANNELS>().0 {
            *frame = sinc.pull(|| {
                if fifo.is_empty() {
                    refill(fifo, pcm, sounding, now, buffer, decode);
                }
                fifo.pop_front().unwrap_or([0.0; 2])
            });
        }
        *sounding
    }

    fn steer<D>(&mut self, now: Instant, buffer: &mut StreamBuffer, decode: &mut D)
    where
        D: FnMut(Option<&[u8]>, &mut [f32]) -> bool,
    {
        if !buffer.is_playing() {
            self.level_ms = None;
            self.sinc.set_step(1.0 + self.drift);
            return;
        }
        let level = buffer.buffered_ms() as f64 + self.fifo.len() as f64 / SAMPLES_PER_MS;
        let target = buffer.latency_target_ms();
        let smooth = self.level_ms.get_or_insert(level);
        *smooth += (level - *smooth) * LEVEL_SMOOTHING;
        let error = *smooth - target;
        // 移ろひを 積みて時計の 癖を知る
        if error.abs() < DRIFT_WINDOW_MS {
            let seconds = FRAME_MS as f64 / 1_000.0;
            self.drift = (self.drift + error * DRIFT_PER_MS_S * seconds).clamp(-MAX_SKEW, MAX_SKEW);
        }
        let skew = (error * SKEW_PER_MS + self.drift).clamp(-MAX_SKEW, MAX_SKEW);
        self.sinc.set_step(1.0 + skew);

        let due = self
            .last_splice
            .is_none_or(|at| now.saturating_duration_since(at) >= SPLICE_EVERY);
        if due
            && level - target > SPLICE_ABOVE_MS
            && error > SPLICE_ABOVE_MS / 2.0
            && let Some(removed) = self.splice(now, buffer, decode)
        {
            self.last_splice = Some(now);
            self.splices += 1;
            if let Some(smooth) = self.level_ms.as_mut() {
                *smooth -= removed as f64 / SAMPLES_PER_MS;
            }
        }
    }

    // 継ぎ目をば 波の山にて 結び合ふ
    fn splice<D>(
        &mut self,
        now: Instant,
        buffer: &mut StreamBuffer,
        decode: &mut D,
    ) -> Option<usize>
    where
        D: FnMut(Option<&[u8]>, &mut [f32]) -> bool,
    {
        let need = FRAME_SAMPLES + SEEK + OVERLAP * 2;
        while self.fifo.len() < need {
            if !refill(
                &mut self.fifo,
                &mut self.pcm,
                &mut self.sounding,
                now,
                buffer,
                decode,
            ) {
                return None;
            }
        }
        let fifo = self.fifo.make_contiguous();
        let mono = |frame: &[f32; 2]| frame[0] + frame[1];
        let template = &fifo[..OVERLAP];
        let template_energy = template.iter().map(|f| mono(f).powi(2)).sum::<f32>();
        let mut best = (FRAME_SAMPLES, f32::MIN);
        for lag in FRAME_SAMPLES - SEEK..=FRAME_SAMPLES + SEEK {
            let (dot, energy) = template.iter().zip(&fifo[lag..lag + OVERLAP]).fold(
                (0.0f32, 0.0f32),
                |(dot, energy), (early, late)| {
                    (dot + mono(early) * mono(late), energy + mono(late).powi(2))
                },
            );
            let score = dot / (template_energy * energy).sqrt().max(1e-9);
            if score > best.1 {
                best = (lag, score);
            }
        }
        let (lag, score) = best;
        let equal_power = score < 0.5;
        for index in 0..OVERLAP {
            let t = (index as f32 + 0.5) / OVERLAP as f32;
            let (rise, fall) = if equal_power {
                ((t * FRAC_PI_2).sin(), (t * FRAC_PI_2).cos())
            } else {
                let rise = 0.5 - 0.5 * (PI * t).cos();
                (rise, 1.0 - rise)
            };
            let early = fifo[index];
            let late = &mut fifo[lag + index];
            late[0] = early[0] * fall + late[0] * rise;
            late[1] = early[1] * fall + late[1] * rise;
        }
        self.fifo.drain(..lag);
        Some(lag)
    }
}

fn refill<D>(
    fifo: &mut VecDeque<[f32; 2]>,
    pcm: &mut [f32],
    sounding: &mut bool,
    now: Instant,
    buffer: &mut StreamBuffer,
    decode: &mut D,
) -> bool
where
    D: FnMut(Option<&[u8]>, &mut [f32]) -> bool,
{
    let fresh = match buffer.tick(now) {
        Some(frame) => {
            if !decode(frame.payload.as_deref(), pcm) && !decode(None, pcm) {
                pcm.fill(0.0);
            }
            if !*sounding {
                fade_in(pcm);
            }
            *sounding = true;
            true
        }
        None if *sounding => {
            if !decode(None, pcm) {
                pcm.fill(0.0);
            }
            fade_out(pcm);
            *sounding = false;
            false
        }
        None => {
            pcm.fill(0.0);
            false
        }
    };
    fifo.extend(pcm.as_chunks::<CHANNELS>().0.iter().copied());
    fresh
}

fn raised_cosine(index: usize, span: usize) -> f32 {
    0.5 - 0.5 * (PI * (index as f32 + 0.5) / span as f32).cos()
}

// 目覚めには 小声で始む 朝の鳥
fn fade_in(pcm: &mut [f32]) {
    for (index, frame) in pcm
        .as_chunks_mut::<CHANNELS>()
        .0
        .iter_mut()
        .take(FADE)
        .enumerate()
    {
        let gain = raised_cosine(index, FADE);
        frame[0] *= gain;
        frame[1] *= gain;
    }
}

// 散る際は 声を細めて 闇に入る
fn fade_out(pcm: &mut [f32]) {
    let frames = pcm.as_chunks_mut::<CHANNELS>().0;
    let span = frames.len();
    for (index, frame) in frames.iter_mut().enumerate() {
        let gain = 1.0 - raised_cosine(index, span);
        frame[0] *= gain;
        frame[1] *= gain;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adaptive::StreamBuffer;

    const AMPLITUDE: f32 = 0.5;
    const OMEGA: f64 = 2.0 * std::f64::consts::PI * 440.0 / SAMPLE_RATE as f64;

    fn payload(seq: u32) -> Vec<u8> {
        (0..FRAME_SAMPLES)
            .flat_map(|index| {
                let n = seq as f64 * FRAME_SAMPLES as f64 + index as f64;
                (((OMEGA * n).sin() as f32) * AMPLITUDE).to_le_bytes()
            })
            .collect()
    }

    // 仮の喉 正弦ひとつ 歌はせて
    #[derive(Default)]
    struct ToneCodec {
        history: [f32; 2],
    }

    impl ToneCodec {
        fn decode(&mut self, payload: Option<&[u8]>, pcm: &mut [f32]) -> bool {
            let pole = 2.0 * 0.9995 * OMEGA.cos() as f32;
            for (index, frame) in pcm.as_chunks_mut::<CHANNELS>().0.iter_mut().enumerate() {
                let value = match payload {
                    Some(bytes) => {
                        f32::from_le_bytes(bytes[index * 4..index * 4 + 4].try_into().unwrap())
                    }
                    None => pole * self.history[1] - 0.999 * self.history[0],
                };
                self.history = [self.history[1], value];
                *frame = [value, value];
            }
            true
        }
    }

    struct Run {
        output: Vec<f32>,
        buffer: StreamBuffer,
        playout: Playout,
        final_level_ms: f64,
    }

    // 遠き野の 時計を少し 急がせて
    fn simulate(seconds: u64, skew: f64, delay_ms: impl Fn(u32) -> Option<f64>) -> Run {
        let t0 = Instant::now();
        let mut buffer = StreamBuffer::new(t0);
        let mut playout = Playout::new();
        let mut codec = ToneCodec::default();
        let mut decode = |payload: Option<&[u8]>, pcm: &mut [f32]| codec.decode(payload, pcm);
        let mut in_flight = Vec::<(f64, u32)>::new();
        let mut next_seq = 0u32;
        let mut output = Vec::new();
        let mut block = vec![0.0f32; FRAME_SAMPLES * CHANNELS];
        let period_ms = FRAME_MS as f64 / (1.0 + skew);
        for ms in 0..seconds * 1_000 {
            let now_ms = ms as f64;
            while next_seq as f64 * period_ms <= now_ms {
                if let Some(delay) = delay_ms(next_seq) {
                    in_flight.push((next_seq as f64 * period_ms + delay, next_seq));
                }
                next_seq += 1;
            }
            in_flight.sort_by(|a, b| a.0.total_cmp(&b.0));
            let landed = in_flight.partition_point(|&(at, _)| at <= now_ms);
            for (at, seq) in in_flight.drain(..landed) {
                let previous = if seq == 0 {
                    Vec::new()
                } else {
                    payload(seq - 1)
                };
                let arrival = t0 + Duration::from_secs_f64(at / 1_000.0);
                buffer.insert_pair(seq, &payload(seq), &previous, arrival);
            }
            if ms % FRAME_MS as u64 == 0 {
                let now = t0 + Duration::from_millis(ms);
                playout.render(now, &mut buffer, &mut decode, &mut block);
                output.extend(block.as_chunks::<CHANNELS>().0.iter().map(|f| f[0]));
            }
        }
        let final_level_ms =
            buffer.buffered_ms() as f64 + playout.fifo.len() as f64 / SAMPLES_PER_MS;
        Run {
            output,
            buffer,
            playout,
            final_level_ms,
        }
    }

    fn jitter(seq: u32) -> f64 {
        let hash = seq.wrapping_mul(2_654_435_761) >> 24;
        2.0 + hash as f64 / 255.0 * 6.0
    }

    fn worst_bend(output: &[f32]) -> f32 {
        output
            .windows(3)
            .skip(SAMPLE_RATE as usize / 10)
            .map(|w| (w[2] - 2.0 * w[1] + w[0]).abs())
            .fold(0.0, f32::max)
    }

    fn sine_bend() -> f32 {
        AMPLITUDE * (OMEGA * OMEGA) as f32
    }

    #[test]
    fn clock_skew_is_absorbed_without_gaps() {
        for skew in [1e-3, -1e-3] {
            let run = simulate(20, skew, |seq| Some(jitter(seq)));
            let counters = run.buffer.counters();
            assert_eq!(counters.concealed, 0, "skew {skew}");
            assert_eq!(run.playout.splices, 0, "skew {skew}");
            let error = run.final_level_ms - run.buffer.latency_target_ms();
            assert!(error.abs() < 15.0, "skew {skew}: {error:.1} ms off target");
            assert!(worst_bend(&run.output) < sine_bend() * 3.0, "skew {skew}");
        }
    }

    #[test]
    fn a_sudden_shortcut_is_spliced_away_seamlessly() {
        let run = simulate(12, 0.0, |seq| Some(if seq < 500 { 60.0 } else { 2.0 }));
        assert!(run.playout.splices >= 3, "{} splices", run.playout.splices);
        assert_eq!(run.buffer.counters().concealed, 0);
        let excess = run.final_level_ms - run.buffer.latency_target_ms();
        assert!(excess < SPLICE_ABOVE_MS + 1.0, "{excess:.1} ms excess left");
        let bend = worst_bend(&run.output);
        assert!(bend < sine_bend() * 3.0, "{bend} vs {}", sine_bend());
    }

    #[test]
    fn an_outage_fades_out_and_back_in() {
        let run = simulate(6, 0.0, |seq| {
            (!(300..340).contains(&seq)).then(|| jitter(seq))
        });
        let bend = worst_bend(&run.output);
        assert!(bend < sine_bend() * 3.0, "{bend} vs {}", sine_bend());
        let tail = &run.output[run.output.len() - SAMPLE_RATE as usize..];
        let peak = tail.iter().fold(0.0f32, |peak, s| peak.max(s.abs()));
        assert!(peak > AMPLITUDE * 0.9, "audio did not come back: {peak}");
    }
}
