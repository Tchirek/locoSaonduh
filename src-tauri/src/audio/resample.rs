//! 波の端 帯を限りて 移し替ふ

use std::f64::consts::PI;

const HALF_TAPS: usize = 32;
const PHASES: usize = 256;
const KAISER_BETA: f64 = 8.0;
const PASSBAND: f64 = 0.95;

// 一歩ごと 入る調べを 量り取る
pub struct Sinc {
    taps: usize,
    rows: Vec<f32>,
    history: Vec<[f32; 2]>,
    write: usize,
    phase: f64,
    step: f64,
}

impl Sinc {
    pub fn new(step: f64) -> Self {
        // 間引くほど 窓を広げて 帯狭む
        let stretch = step.max(1.0);
        let half = (HALF_TAPS as f64 * stretch).ceil() as usize;
        let taps = half * 2;
        let cutoff = PASSBAND / stretch;
        let norm = bessel_i0(KAISER_BETA);
        let mut rows = vec![0.0f32; (PHASES + 1) * taps];
        let mut weights = vec![0.0f64; taps];
        for (phase, row) in rows.chunks_exact_mut(taps).enumerate() {
            let offset = phase as f64 / PHASES as f64;
            for (index, weight) in weights.iter_mut().enumerate() {
                let x = offset + (half - 1) as f64 - index as f64;
                let u = x / half as f64;
                let window = if u.abs() < 1.0 {
                    bessel_i0(KAISER_BETA * (1.0 - u * u).sqrt()) / norm
                } else {
                    0.0
                };
                *weight = cutoff * sinc(cutoff * x) * window;
            }
            // 一列の 重みを足して 一となす
            let sum = weights.iter().sum::<f64>();
            for (slot, weight) in row.iter_mut().zip(&weights) {
                *slot = (weight / sum) as f32;
            }
        }
        Self {
            taps,
            rows,
            history: vec![[0.0; 2]; taps * 2],
            write: 0,
            phase: 0.0,
            step,
        }
    }

    pub fn set_step(&mut self, step: f64) {
        self.step = step;
    }

    // ひとつ入れ 満ちたる分を 書き足しぬ
    pub fn push(&mut self, frame: [f32; 2], out: &mut Vec<f32>) {
        self.load(frame);
        while self.phase < 1.0 {
            let [left, right] = self.convolve();
            out.push(left);
            out.push(right);
            self.phase += self.step;
        }
        self.phase -= 1.0;
    }

    // 要るだけを 泉に汲みて 一つ出す
    pub fn pull<F>(&mut self, mut source: F) -> [f32; 2]
    where
        F: FnMut() -> [f32; 2],
    {
        while self.phase >= 1.0 {
            self.load(source());
            self.phase -= 1.0;
        }
        let frame = self.convolve();
        self.phase += self.step;
        frame
    }

    // 二重なる 輪に書き置けば 窓続く
    fn load(&mut self, frame: [f32; 2]) {
        self.history[self.write] = frame;
        self.history[self.write + self.taps] = frame;
        self.write = (self.write + 1) % self.taps;
    }

    fn convolve(&self) -> [f32; 2] {
        let position = self.phase * PHASES as f64;
        let row = (position as usize).min(PHASES - 1);
        let fraction = (position - row as f64) as f32;
        let taps = self.taps;
        let early = &self.rows[row * taps..(row + 1) * taps];
        let late = &self.rows[(row + 1) * taps..(row + 2) * taps];
        let window = &self.history[self.write..self.write + taps];
        let mut left = 0.0f32;
        let mut right = 0.0f32;
        for ((&a, &b), frame) in early.iter().zip(late).zip(window) {
            let weight = a + (b - a) * fraction;
            left += weight * frame[0];
            right += weight * frame[1];
        }
        [left, right]
    }
}

fn sinc(x: f64) -> f64 {
    if x.abs() < 1e-12 {
        1.0
    } else {
        (PI * x).sin() / (PI * x)
    }
}

fn bessel_i0(x: f64) -> f64 {
    let quarter = x * x / 4.0;
    let mut term = 1.0;
    let mut sum = 1.0;
    for k in 1..64 {
        term *= quarter / (k * k) as f64;
        sum += term;
        if term < sum * 1e-17 {
            break;
        }
    }
    sum
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(rate: f64, hz: f64, frames: usize) -> Vec<[f32; 2]> {
        (0..frames)
            .map(|n| {
                let value = (2.0 * PI * hz * n as f64 / rate).sin() as f32 * 0.5;
                [value, -value]
            })
            .collect()
    }

    fn resample(input: &[[f32; 2]], from: f64, to: f64) -> Vec<f32> {
        let mut sinc = Sinc::new(from / to);
        let mut out = Vec::new();
        for frame in input {
            sinc.push(*frame, &mut out);
        }
        out
    }

    // 正弦を 当てて残りの ねを量る
    fn snr_db(left: &[f32], rate: f64, hz: f64) -> f64 {
        let omega = 2.0 * PI * hz / rate;
        let (mut ss, mut sc, mut cc, mut ys, mut yc) = (0.0, 0.0, 0.0, 0.0, 0.0);
        for (n, &y) in left.iter().enumerate() {
            let (s, c) = (omega * n as f64).sin_cos();
            ss += s * s;
            sc += s * c;
            cc += c * c;
            ys += y as f64 * s;
            yc += y as f64 * c;
        }
        let det = ss * cc - sc * sc;
        let a = (ys * cc - yc * sc) / det;
        let b = (yc * ss - ys * sc) / det;
        let (mut signal, mut noise) = (0.0, 0.0);
        for (n, &y) in left.iter().enumerate() {
            let (s, c) = (omega * n as f64).sin_cos();
            let fit = a * s + b * c;
            signal += fit * fit;
            noise += (y as f64 - fit).powi(2);
        }
        10.0 * (signal / noise).log10()
    }

    fn left(interleaved: &[f32]) -> Vec<f32> {
        interleaved.chunks_exact(2).map(|frame| frame[0]).collect()
    }

    #[test]
    fn treble_survives_44k1_to_48k() {
        let out = resample(&tone(44_100.0, 10_000.0, 44_100), 44_100.0, 48_000.0);
        let steady = &left(&out)[1_000..40_000];
        let snr = snr_db(steady, 48_000.0, 10_000.0);
        assert!(snr > 70.0, "{snr:.1} dB");
    }

    #[test]
    fn treble_survives_48k_to_44k1() {
        let out = resample(&tone(48_000.0, 15_000.0, 48_000), 48_000.0, 44_100.0);
        let steady = &left(&out)[1_000..40_000];
        let snr = snr_db(steady, 44_100.0, 15_000.0);
        assert!(snr > 70.0, "{snr:.1} dB");
    }

    #[test]
    fn content_above_the_new_nyquist_is_removed() {
        let out = resample(&tone(48_000.0, 23_500.0, 48_000), 48_000.0, 44_100.0);
        let peak = left(&out)[1_000..40_000]
            .iter()
            .fold(0.0f32, |peak, s| peak.max(s.abs()));
        assert!(peak < 0.5e-3, "alias at {peak}");
    }

    #[test]
    fn a_slight_drift_stays_transparent() {
        let mut sinc = Sinc::new(1.0);
        sinc.set_step(1.002);
        let input = tone(48_000.0, 1_000.0, 48_000);
        let mut index = 0;
        let out = (0..40_000)
            .map(|_| {
                sinc.pull(|| {
                    index += 1;
                    input[index - 1]
                })[0]
            })
            .collect::<Vec<_>>();
        let snr = snr_db(&out[1_000..], 48_000.0 / 1.002, 1_000.0);
        assert!(snr > 90.0, "{snr:.1} dB");
    }

    #[test]
    fn decimating_by_four_keeps_the_band() {
        let out = resample(&tone(192_000.0, 18_000.0, 192_000), 192_000.0, 48_000.0);
        let steady = &left(&out)[1_000..40_000];
        let snr = snr_db(steady, 48_000.0, 18_000.0);
        assert!(snr > 60.0, "{snr:.1} dB");
    }
}
