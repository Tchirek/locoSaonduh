//! Small, deterministic sample adapters.
//!
//! 片耳も 遠き調べも 四十八

use super::resample::Sinc;
use crate::protocol::SAMPLE_RATE;

pub struct CaptureAdapter {
    channels: usize,
    resampler: Option<Sinc>,
}

impl CaptureAdapter {
    pub fn new(channels: usize, source_rate: u32) -> Self {
        Self {
            channels: channels.max(1),
            resampler: (source_rate != SAMPLE_RATE)
                .then(|| Sinc::new(source_rate as f64 / SAMPLE_RATE as f64)),
        }
    }

    /// Turn an arbitrary interleaved capture block into interleaved 48 kHz
    /// stereo. The returned buffer is reused by the caller.
    pub fn push(&mut self, input: &[f32], out: &mut Vec<f32>) {
        let right = self.channels.min(2) - 1;
        for frame in input.chunks_exact(self.channels) {
            let pair = [frame[0], frame[right]];
            match &mut self.resampler {
                Some(sinc) => sinc.push(pair, out),
                None => out.extend_from_slice(&pair),
            }
        }
    }
}

/// A pull-side resampler for the output callback. It interpolates 48 kHz
/// stereo blocks from the mixer into whatever rate the device runs at.
pub struct PlaybackAdapter {
    resampler: Option<Sinc>,
}

impl PlaybackAdapter {
    pub fn new(output_rate: u32) -> Self {
        Self {
            resampler: (output_rate != SAMPLE_RATE)
                .then(|| Sinc::new(SAMPLE_RATE as f64 / output_rate as f64)),
        }
    }

    /// Produce one stereo frame by pulling at most one 48 kHz frame from the
    /// source. This is the allocation-free form the output callback uses.
    pub fn next<F>(&mut self, mut source: F) -> [f32; 2]
    where
        F: FnMut() -> [f32; 2],
    {
        match &mut self.resampler {
            Some(sinc) => sinc.pull(source),
            None => source(),
        }
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
    fn pull_resampler_settles_on_dc() {
        let mut adapter = PlaybackAdapter::new(44_100);
        let out = (0..441)
            .map(|_| adapter.next(|| [0.5, -0.5]))
            .collect::<Vec<_>>();
        let [left, right] = out[440];
        assert!((left - 0.5).abs() < 1e-4 && (right + 0.5).abs() < 1e-4);
    }
}
