//! Noise generators. Every color is derived from a single white noise source
//! by filtering, so they all share the same random number generator.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum NoiseColor {
    White,
    Pink,
    Brown,
    Blue,
    Violet,
}

impl NoiseColor {
    pub const ALL: [NoiseColor; 5] = [
        NoiseColor::White,
        NoiseColor::Pink,
        NoiseColor::Brown,
        NoiseColor::Blue,
        NoiseColor::Violet,
    ];

    pub fn from_u8(v: u8) -> Self {
        Self::ALL.get(v as usize).copied().unwrap_or(NoiseColor::White)
    }

    pub fn label(self) -> &'static str {
        match self {
            NoiseColor::White => "White",
            NoiseColor::Pink => "Pink",
            NoiseColor::Brown => "Brown",
            NoiseColor::Blue => "Blue",
            NoiseColor::Violet => "Violet",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            NoiseColor::White => "Equal power at every frequency. Bright, hissy, like TV static.",
            NoiseColor::Pink => "−3 dB/octave. Equal power per octave, like steady rain.",
            NoiseColor::Brown => "−6 dB/octave. Deep, soft rumble, like a waterfall or strong wind.",
            NoiseColor::Blue => "+3 dB/octave. Sharp and high-pitched, like a hissing spray.",
            NoiseColor::Violet => "+6 dB/octave. Very high-pitched hiss, mostly treble.",
        }
    }
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

/// Holds all filter state so switching colors is instant and click-free-ish.
/// All filters run every sample, so their state stays "warm" when you switch.
pub struct NoiseGenerator {
    rng: XorShift32,
    pink: [f32; 7],
    brown: f32,
    prev_white: f32,
    prev_pink: f32,
}

impl NoiseGenerator {
    pub fn new(seed: u32) -> Self {
        Self {
            rng: XorShift32(seed.max(1)),
            pink: [0.0; 7],
            brown: 0.0,
            prev_white: 0.0,
            prev_pink: 0.0,
        }
    }

    pub fn next(&mut self, color: NoiseColor) -> f32 {
        let white = self.rng.next_f32();

        // Paul Kellet's refined pink noise filter (accurate to ±0.05 dB above 9.2 Hz).
        let b = &mut self.pink;
        b[0] = 0.99886 * b[0] + white * 0.0555179;
        b[1] = 0.99332 * b[1] + white * 0.0750759;
        b[2] = 0.96900 * b[2] + white * 0.1538520;
        b[3] = 0.86650 * b[3] + white * 0.3104856;
        b[4] = 0.55000 * b[4] + white * 0.5329522;
        b[5] = -0.7616 * b[5] - white * 0.0168980;
        let pink = (b[0] + b[1] + b[2] + b[3] + b[4] + b[5] + b[6] + white * 0.5362) * 0.11;
        b[6] = white * 0.115926;

        // Leaky integrator: a random walk that slowly decays back toward zero.
        self.brown = (self.brown + 0.02 * white) / 1.02;
        let brown = self.brown * 3.5;

        // First difference tilts the spectrum up by +6 dB/octave.
        let blue = (pink - self.prev_pink) * 1.7;
        let violet = (white - self.prev_white) * 0.25;
        self.prev_pink = pink;
        self.prev_white = white;

        // Gains are hand-tuned so the colors have roughly similar loudness.
        match color {
            NoiseColor::White => white * 0.35,
            NoiseColor::Pink => pink,
            NoiseColor::Brown => brown,
            NoiseColor::Blue => blue,
            NoiseColor::Violet => violet,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_have_similar_loudness_and_stay_in_range() {
        for color in NoiseColor::ALL {
            let mut g = NoiseGenerator::new(1234);
            let n = 480_000;
            let (mut sum_sq, mut peak) = (0.0f64, 0.0f32);
            for _ in 0..n {
                let s = g.next(color);
                sum_sq += (s as f64).powi(2);
                peak = peak.max(s.abs());
            }
            let rms = (sum_sq / n as f64).sqrt();
            println!("{:>6}: rms {rms:.3} peak {peak:.3}", color.label());
            assert!((0.15..0.25).contains(&rms), "{color:?} rms {rms}");
            assert!(peak <= 1.0, "{color:?} peak {peak}");
        }
    }
}
