use opus_pure::{Application, MAX_PACKET_BYTES, OpusDecoder, OpusEncoder, RateControl};
use thiserror::Error;

use crate::protocol::{CHANNELS, FRAME_SAMPLES, SAMPLE_RATE};

#[derive(Debug, Error)]
pub enum CodecError {
    #[error("opus: {0}")]
    Opus(#[from] opus_pure::Error),
    #[error("encoder produced {0} bytes, more than one datagram allows")]
    PacketTooLarge(usize),
}

pub struct Encoder {
    inner: OpusEncoder,
    bitrate_kbps: u32,
}

impl Encoder {
    pub fn new() -> Result<Self, CodecError> {
        let mut inner = OpusEncoder::new(SAMPLE_RATE as i32, CHANNELS, Application::Audio)?;
        inner.complexity = 8;
        inner.rate_control = RateControl::ConstrainedVbr;
        inner.use_inband_fec = true;
        inner.packet_loss_perc = 1;
        inner.bitrate_bps = 112_000;
        Ok(Self {
            inner,
            bitrate_kbps: 112,
        })
    }

    pub fn set_quality(&mut self, bitrate_kbps: u32, loss_pct: f64) {
        let bitrate = bitrate_kbps.clamp(32, 256);
        if bitrate != self.bitrate_kbps {
            self.inner.bitrate_bps = (bitrate * 1_000) as i32;
            self.bitrate_kbps = bitrate;
        }
        self.inner.packet_loss_perc = (loss_pct.ceil() as i32).clamp(0, 30);
    }

    pub fn encode(&mut self, pcm: &[f32], out: &mut Vec<u8>) -> Result<(), CodecError> {
        debug_assert_eq!(pcm.len(), FRAME_SAMPLES * CHANNELS);
        out.resize(MAX_PACKET_BYTES, 0);
        let written = self.inner.encode(pcm, FRAME_SAMPLES, out)?;
        if written > 1_200 {
            return Err(CodecError::PacketTooLarge(written));
        }
        out.truncate(written);
        Ok(())
    }
}

pub struct Decoder {
    inner: OpusDecoder,
}

impl Decoder {
    pub fn new() -> Result<Self, CodecError> {
        Ok(Self {
            inner: OpusDecoder::new(SAMPLE_RATE as i32, CHANNELS)?,
        })
    }

    /// Decode one frame. An empty payload asks Opus to conceal the gap.
    pub fn decode(&mut self, payload: Option<&[u8]>, pcm: &mut [f32]) -> Result<usize, CodecError> {
        debug_assert_eq!(pcm.len(), FRAME_SAMPLES * CHANNELS);
        let empty = [];
        let input = payload.unwrap_or(&empty);
        Ok(self.inner.decode(input, FRAME_SAMPLES, pcm)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sine_survives_the_round_trip() {
        let mut encoder = Encoder::new().unwrap();
        let mut decoder = Decoder::new().unwrap();
        let mut pcm = vec![0.0f32; FRAME_SAMPLES * CHANNELS];
        for (i, frame) in pcm.as_chunks_mut::<CHANNELS>().0.iter_mut().enumerate() {
            let sample =
                (i as f32 * 440.0 * std::f32::consts::TAU / SAMPLE_RATE as f32).sin() * 0.2;
            frame[0] = sample;
            frame[1] = sample;
        }

        // Warm the codec's analysis and framing state.
        for _ in 0..4 {
            let mut packet = Vec::new();
            encoder.encode(&pcm, &mut packet).unwrap();
            let mut out = vec![0.0; FRAME_SAMPLES * CHANNELS];
            let samples = decoder.decode(Some(&packet), &mut out).unwrap();
            assert_eq!(samples, FRAME_SAMPLES);
        }

        let mut packet = Vec::new();
        encoder.encode(&pcm, &mut packet).unwrap();
        assert!(!packet.is_empty());
        assert!(packet.len() < 600, "{} bytes", packet.len());

        let mut concealed = vec![0.0; FRAME_SAMPLES * CHANNELS];
        decoder.decode(None, &mut concealed).unwrap();
        assert!(concealed.iter().all(|s| s.is_finite()));
    }
}
