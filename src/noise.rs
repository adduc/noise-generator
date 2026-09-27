// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 John Long

//! Equalizer-shaped noise. Each octave band gets its own independent white
//! noise source, band-pass filtered and scaled by that band's gain. Because
//! the bands are uncorrelated, their powers simply add: a slider's dB value
//! is exactly the level of that band, with no phase interaction between bands.

/// Center frequencies of the octave bands, in Hz.
pub const BAND_FREQS: [f32; 10] = [
    31.25, 62.5, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
];
pub const BAND_COUNT: usize = BAND_FREQS.len();

/// Index of the 1 kHz band, which presets pivot around (it stays at 0 dB).
const REFERENCE_BAND: usize = 5;

/// Slider range in dB. Wide enough for brown/violet presets (±6 dB/octave).
pub const MIN_DB: f32 = -30.0;
pub const MAX_DB: f32 = 30.0;

/// Output RMS after normalization, so the EQ changes tone but not loudness.
const TARGET_RMS: f32 = 0.15;

/// An octave-wide band has Q = √2.
const BAND_Q: f32 = std::f32::consts::SQRT_2;

/// Variance of a uniform [-1, 1) white noise sample.
const WHITE_VARIANCE: f32 = 1.0 / 3.0;

pub fn band_label(index: usize) -> String {
    let f = BAND_FREQS[index];
    if f >= 1000.0 {
        format!("{}k", f / 1000.0)
    } else {
        format!("{}", f.round())
    }
}

/// Classic noise colors, expressed as a constant EQ slope.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    White,
    Pink,
    Brown,
    Blue,
    Violet,
}

impl Preset {
    pub const ALL: [Preset; 5] = [
        Preset::White,
        Preset::Pink,
        Preset::Brown,
        Preset::Blue,
        Preset::Violet,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Preset::White => "White",
            Preset::Pink => "Pink",
            Preset::Brown => "Brown",
            Preset::Blue => "Blue",
            Preset::Violet => "Violet",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Preset::White => "Equal power at every frequency. Bright, hissy, like TV static.",
            Preset::Pink => "−3 dB/octave. Equal power per octave, like steady rain.",
            Preset::Brown => "−6 dB/octave. Deep, soft rumble, like a waterfall or strong wind.",
            Preset::Blue => "+3 dB/octave. Sharp and high-pitched, like a hissing spray.",
            Preset::Violet => "+6 dB/octave. Very high-pitched hiss, mostly treble.",
        }
    }

    /// Slope relative to white noise. Since each band is an octave, the
    /// slope in dB/octave is also the step between adjacent sliders.
    fn slope_db_per_octave(self) -> f32 {
        match self {
            Preset::White => 0.0,
            Preset::Pink => -3.0,
            Preset::Brown => -6.0,
            Preset::Blue => 3.0,
            Preset::Violet => 6.0,
        }
    }

    /// The preset with this label, as written to the settings file.
    pub fn from_label(label: &str) -> Option<Preset> {
        Preset::ALL.into_iter().find(|p| p.label() == label)
    }

    pub fn band_db(self) -> [f32; BAND_COUNT] {
        let slope = self.slope_db_per_octave();
        std::array::from_fn(|i| (slope * (i as f32 - REFERENCE_BAND as f32)).clamp(MIN_DB, MAX_DB))
    }
}

/// RBJ "constant 0 dB peak gain" band-pass biquad.
#[derive(Clone, Copy, Default)]
struct BandPass {
    b0: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
    /// Output variance for unit-variance white input: Σh[n]².
    noise_gain: f32,
}

impl BandPass {
    fn new(freq: f32, q: f32, sample_rate: f32) -> Self {
        // Bands at or near Nyquist can't be represented; leave them silent.
        if freq >= 0.45 * sample_rate {
            return Self::default();
        }
        let w0 = std::f32::consts::TAU * freq / sample_rate;
        let alpha = w0.sin() / (2.0 * q);
        let a0 = 1.0 + alpha;
        Self {
            b0: alpha / a0,
            a1: -2.0 * w0.cos() / a0,
            a2: (1.0 - alpha) / a0,
            z1: 0.0,
            z2: 0.0,
            // This filter equals (1 − allpass)/2, which makes Σh² = α / (1 + α).
            noise_gain: alpha / (1.0 + alpha),
        }
    }

    /// Transposed direct form II. The numerator is b0·(1 − z⁻²), so b1 = 0 and b2 = −b0.
    fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.z2 - self.a1 * y;
        self.z2 = -self.b0 * x - self.a2 * y;
        y
    }
}

/// Each band's output variance for white input, which depends only on the
/// sample rate. Compute once and pass to [`band_gains`].
pub fn band_variances(sample_rate: f32) -> [f32; BAND_COUNT] {
    BAND_FREQS.map(|f| WHITE_VARIANCE * BandPass::new(f, BAND_Q, sample_rate).noise_gain)
}

/// Converts slider positions (dB) into linear per-band amplitudes, scaled so
/// the summed output lands at `TARGET_RMS` whatever the EQ shape.
/// Allocation-free, so it can run on the audio thread.
pub fn band_gains(
    band_db: &[f32; BAND_COUNT],
    band_variances: &[f32; BAND_COUNT],
) -> [f32; BAND_COUNT] {
    let mut gains: [f32; BAND_COUNT] = std::array::from_fn(|i| 10f32.powf(band_db[i] / 20.0));
    let variance: f32 = gains
        .iter()
        .zip(band_variances)
        .map(|(g, v)| g * g * v)
        .sum();
    if variance > 0.0 {
        let scale = TARGET_RMS / variance.sqrt();
        gains.iter_mut().for_each(|g| *g *= scale);
    }
    gains
}

/// xorshift32: tiny, fast, allocation-free PRNG. Plenty random for audio,
/// and safe to call from the real-time audio thread.
struct XorShift32(u32);

impl XorShift32 {
    fn next_f32(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        // Map to [-1, 1)
        (x as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

pub struct NoiseGenerator {
    rng: XorShift32,
    bands: [BandPass; BAND_COUNT],
}

impl NoiseGenerator {
    pub fn new(seed: u32, sample_rate: f32) -> Self {
        Self {
            rng: XorShift32(seed.max(1)),
            bands: BAND_FREQS.map(|f| BandPass::new(f, BAND_Q, sample_rate)),
        }
    }

    /// `gains` are linear amplitudes, as produced by [`band_gains`].
    pub fn next(&mut self, gains: &[f32; BAND_COUNT]) -> f32 {
        let mut sum = 0.0;
        for (band, gain) in self.bands.iter_mut().zip(gains) {
            // Fresh random sample per band keeps the bands uncorrelated.
            sum += band.process(self.rng.next_f32()) * gain;
        }
        sum
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_are_normalized_to_target_loudness() {
        for sample_rate in [44_100.0, 48_000.0] {
            for preset in Preset::ALL {
                let gains = band_gains(&preset.band_db(), &band_variances(sample_rate));
                let mut g = NoiseGenerator::new(1234, sample_rate);
                let n = 480_000;
                let (mut sum_sq, mut peak) = (0.0f64, 0.0f32);
                for _ in 0..n {
                    let s = g.next(&gains);
                    sum_sq += (s as f64).powi(2);
                    peak = peak.max(s.abs());
                }
                let rms = (sum_sq / n as f64).sqrt() as f32;
                println!(
                    "{sample_rate} {:>6}: rms {rms:.3} peak {peak:.3}",
                    preset.label()
                );
                assert!(
                    (rms - TARGET_RMS).abs() < 0.1 * TARGET_RMS,
                    "{preset:?} rms {rms}"
                );
                assert!(peak <= 1.0, "{preset:?} peak {peak}");
            }
        }
    }

    #[test]
    fn pink_preset_steps_three_db_per_band() {
        let db = Preset::Pink.band_db();
        assert_eq!(db[REFERENCE_BAND], 0.0);
        assert!(db.windows(2).all(|w| (w[1] - w[0] + 3.0).abs() < 1e-6));
    }
}
