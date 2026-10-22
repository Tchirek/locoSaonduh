//! Small, deterministic sample adapters.
//!
//! Capture devices can present anything from a mono headset at 44.1 kHz to a
//! surround receiver at 192 kHz. The network always carries 48 kHz stereo, so
//! this module owns the (single) channel downmix and the streaming linear
//! resampler used on both ends. It is intentionally allocation-free once warm.

use std::collections::VecDeque;

use crate::protocol::{CHANNELS, SAMPLE_RATE};

pub struct CaptureAdapter {
    channels: usize,
    source_rate: u32,
    pending: VecDeque<[f32; 2]>,
    phase: f64,
}

impl CaptureAdapter {
    pub fn new(channels: usize, source_rate: u32) -> Self {
        Self {
            channels: channels.max(1),
            source_rate,
            pending: VecDeque::with_capacity(4_096),
            phase: 0.0,
        }
    }

    /// Turn an arbitrary interleaved capture block into interleaved 48 kHz
    /// stereo. The returned buffer is reused by the caller.
    pub fn push(&mut self, input: &[f32], out: &mut Vec<f32>) {
        match self.channels {
            1 => {
                for sample in input {
                    self.pending.push_back([*sample, *sample]);
                }
            }
            n if n >= 2 => {
                for frame in input.chunks_exact(n) {
                    self.pending.push_back([frame[0], frame[1]]);
                }
            }
            _ => {}
        }
        self.drain_to(out);
    }

    fn drain_to(&mut self, out: &mut Vec<f32>) {
        let step = self.source_rate as f64 / SAMPLE_RATE as f64;
        if (step - 1.0).abs() < f64::EPSILON {
            while let Some([l, r]) = self.pending.pop_front() {
                out.push(l);
                out.push(r);
            }
            self.phase = 0.0;
            return;
        }

        while self.phase + 1.0 < self.pending.len() as f64 {
            let index = self.phase.floor() as usize;
            let fraction = (self.phase - index as f64) as f32;
            let a = self.pending[index];
            let b = self.pending[index + 1];
            out.push(a[0] + (b[0] - a[0]) * fraction);
            out.push(a[1] + (b[1] - a[1]) * fraction);
            self.phase += step;
        }

        let consumed = self.phase.floor() as usize;
        if consumed > 0 {
            self.pending.drain(..consumed);
            self.phase -= consumed as f64;
        }
    }
}

/// A pull-side resampler for the output callback. It interpolates 48 kHz
/// stereo blocks from the mixer into whatever rate the device runs at.
pub struct PlaybackAdapter {
    output_rate: u32,
    previous: Option<[f32; 2]>,
    current: [f32; 2],
    phase: f64,
    step: f64,
}

impl PlaybackAdapter {
    pub fn new(output_rate: u32) -> Self {
        Self {
            output_rate,
            previous: None,
            current: [0.0, 0.0],
            phase: 0.0,
            step: if output_rate == SAMPLE_RATE {
                1.0
            } else {
                SAMPLE_RATE as f64 / output_rate as f64
            },
        }
    }

    /// Produce one stereo frame by pulling at most one 48 kHz frame from the
    /// source. This is the allocation-free form the output callback uses.
    pub fn next<F>(&mut self, mut source: F) -> [f32; 2]
    where
        F: FnMut() -> [f32; 2],
    {
        if (self.step - 1.0).abs() < f64::EPSILON {
            return source();
        }
        while self.phase >= 1.0 {
            self.previous = Some(self.current);
            self.current = source();
            self.phase -= 1.0;
        }
        let (a, b) = match self.previous {
            Some(previous) => (previous, self.current),
            None => {
                self.previous = Some(self.current);
                (self.current, self.current)
            }
        };
        let fraction = self.phase as f32;
        let frame = [
            a[0] + (b[0] - a[0]) * fraction,
            a[1] + (b[1] - a[1]) * fraction,
        ];
        self.phase += self.step;
        frame
    }

    /// Produce exactly `frames` stereo frames by pulling from `source`.
    /// Missing source samples are silence, which is what a starved jitter
    /// buffer should sound like: a clean gap, not a stuck sample.
    pub fn pull<F>(&mut self, frames: usize, mut source: F, out: &mut [[f32; 2]])
    where
        F: FnMut() -> [f32; 2],
    {
        for frame in out.iter_mut().take(frames) {
            *frame = self.next(&mut source);
        }
    }

    pub fn output_rate(&self) -> u32 {
        self.output_rate
    }
}

pub fn downmix_to_mono(stereo: &[f32], mono: &mut [f32]) {
    for (frame, slot) in stereo.as_chunks::<CHANNELS>().0.iter().zip(mono.iter_mut()) {
        *slot = (frame[0] + frame[1]) * 0.5;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mono_capture_becomes_stereo() {
        let mut adapter = CaptureAdapter::new(1, SAMPLE_RATE);
        let mut out = Vec::new();
        adapter.push(&[0.25, -0.5], &mut out);
        assert_eq!(out, vec![0.25, 0.25, -0.5, -0.5]);
    }

    #[test]
    fn stereo_capture_is_bit_exact_at_48k() {
        let mut adapter = CaptureAdapter::new(2, SAMPLE_RATE);
        let mut out = Vec::new();
        adapter.push(&[0.1, 0.2, 0.3, 0.4], &mut out);
        assert_eq!(out, vec![0.1, 0.2, 0.3, 0.4]);
    }

    #[test]
    fn resampling_produces_expected_frame_count() {
        let mut adapter = CaptureAdapter::new(2, 44_100);
        let mut out = Vec::new();
        let input = vec![0.0f32; 2 * 4_410];
        adapter.push(&input, &mut out);
        let output_frames = out.len() / 2;
        assert!((output_frames as i64 - 4_800).abs() < 2);
    }

    #[test]
    fn pull_resampler_is_finite_and_sized() {
        let mut adapter = PlaybackAdapter::new(44_100);
        let mut out = [[0.0f32; 2]; 441];
        adapter.pull(441, || [0.5, -0.5], &mut out);
        assert!(out.iter().flatten().all(|s| s.is_finite()));
    }
}
