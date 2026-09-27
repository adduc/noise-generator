// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 John Long

//! Audio output. The GTK thread and the real-time audio thread share state
//! only through atomics, so the audio callback never blocks on a lock.

use std::error::Error;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, Stream, StreamConfig};

use crate::noise::{self, BAND_COUNT, NoiseGenerator};

/// Controls shared between the UI and the audio callback.
pub struct Controls {
    /// Per-band EQ levels in dB. f32s are stored as raw bits, since there
    /// is no AtomicF32.
    band_db: [AtomicU32; BAND_COUNT],
    volume: AtomicU32,
    playing: AtomicBool,
}

impl Controls {
    /// Starts paused, with the given EQ and volume.
    pub fn new(band_db: &[f32; BAND_COUNT], volume: f32) -> Arc<Self> {
        Arc::new(Self {
            band_db: band_db.map(|db| AtomicU32::new(db.to_bits())),
            volume: AtomicU32::new(volume.clamp(0.0, 1.0).to_bits()),
            playing: AtomicBool::new(false),
        })
    }

    pub fn set_band_db(&self, band: usize, db: f32) {
        self.band_db[band].store(db.to_bits(), Ordering::Relaxed);
    }

    pub fn set_volume(&self, volume: f32) {
        self.volume
            .store(volume.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    pub fn set_playing(&self, playing: bool) {
        self.playing.store(playing, Ordering::Relaxed);
    }

    fn band_db(&self) -> [f32; BAND_COUNT] {
        std::array::from_fn(|i| f32::from_bits(self.band_db[i].load(Ordering::Relaxed)))
    }

    fn volume(&self) -> f32 {
        f32::from_bits(self.volume.load(Ordering::Relaxed))
    }

    fn playing(&self) -> bool {
        self.playing.load(Ordering::Relaxed)
    }
}

/// Opens the default output device and starts a stream that renders noise
/// according to `controls`. The stream plays until it is dropped; "pause"
/// is handled by fading the gain to zero rather than stopping the device.
pub fn start(controls: Arc<Controls>) -> Result<Stream, Box<dyn Error>> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or("no audio output device found")?;
    let supported = device.default_output_config()?;
    let format = supported.sample_format();
    let config: StreamConfig = supported.into();

    let stream = match format {
        SampleFormat::F32 => build::<f32>(&device, config, controls),
        SampleFormat::F64 => build::<f64>(&device, config, controls),
        SampleFormat::I16 => build::<i16>(&device, config, controls),
        SampleFormat::I32 => build::<i32>(&device, config, controls),
        SampleFormat::U16 => build::<u16>(&device, config, controls),
        other => return Err(format!("unsupported sample format: {other}").into()),
    }?;
    stream.play()?;
    Ok(stream)
}

fn build<T>(
    device: &cpal::Device,
    config: StreamConfig,
    controls: Arc<Controls>,
) -> Result<Stream, Box<dyn Error>>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = config.channels as usize;
    let sample_rate = config.sample_rate as f32;

    // One generator per channel with different seeds: uncorrelated left/right
    // noise sounds wide and enveloping instead of sitting in the middle.
    let mut generators: Vec<NoiseGenerator> = (0..channels)
        .map(|ch| NoiseGenerator::new(0x9E37_79B9u32.wrapping_mul(ch as u32 + 1), sample_rate))
        .collect();

    // Gains are smoothed toward their targets with a one-pole filter (~20 ms
    // time constant) so slider moves and play/pause don't click or "zipper".
    let smoothing = 1.0 - (-1.0 / (0.02 * sample_rate)).exp();
    let mut master = 0.0f32;
    let band_variances = noise::band_variances(sample_rate);
    let mut band_db = controls.band_db();
    let mut band_targets = noise::band_gains(&band_db, &band_variances);
    let mut band_gains = band_targets;

    let stream = device.build_output_stream(
        config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            // Recompute band gains only when a slider actually moved.
            let wanted_db = controls.band_db();
            if wanted_db != band_db {
                band_db = wanted_db;
                band_targets = noise::band_gains(&band_db, &band_variances);
            }
            let volume = controls.volume();
            // Squared for a more natural-feeling volume slider.
            let master_target = if controls.playing() {
                volume * volume
            } else {
                0.0
            };

            for frame in data.chunks_mut(channels) {
                master += (master_target - master) * smoothing;
                for (gain, target) in band_gains.iter_mut().zip(&band_targets) {
                    *gain += (target - *gain) * smoothing;
                }

                for (sample, generator) in frame.iter_mut().zip(generators.iter_mut()) {
                    let value = generator.next(&band_gains) * master;
                    *sample = T::from_sample(value.clamp(-1.0, 1.0));
                }
            }
        },
        |err| eprintln!("audio stream error: {err}"),
        None,
    )?;
    Ok(stream)
}
