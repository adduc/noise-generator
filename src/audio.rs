//! Audio output. The GTK thread and the real-time audio thread share state
//! only through atomics, so the audio callback never blocks on a lock.

use std::error::Error;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, Ordering};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, Stream, StreamConfig};

use crate::noise::{NoiseColor, NoiseGenerator};

/// Controls shared between the UI and the audio callback.
pub struct Controls {
    color: AtomicU8,
    /// f32 volume stored as raw bits, since there is no AtomicF32.
    volume: AtomicU32,
    playing: AtomicBool,
}

impl Controls {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            color: AtomicU8::new(NoiseColor::White as u8),
            volume: AtomicU32::new(0.5f32.to_bits()),
            playing: AtomicBool::new(false),
        })
    }

    pub fn set_color(&self, color: NoiseColor) {
        self.color.store(color as u8, Ordering::Relaxed);
    }

    pub fn set_volume(&self, volume: f32) {
        self.volume.store(volume.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    pub fn set_playing(&self, playing: bool) {
        self.playing.store(playing, Ordering::Relaxed);
    }

    fn color(&self) -> NoiseColor {
        NoiseColor::from_u8(self.color.load(Ordering::Relaxed))
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
        .map(|ch| NoiseGenerator::new(0x9E37_79B9u32.wrapping_mul(ch as u32 + 1)))
        .collect();

    // Gain is smoothed toward its target with a one-pole filter (~20 ms time
    // constant) so volume changes, play/pause and color switches don't click.
    let smoothing = 1.0 - (-1.0 / (0.02 * sample_rate)).exp();
    let mut gain = 0.0f32;
    let mut active_color = controls.color();

    let stream = device.build_output_stream(
        config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            let wanted_color = controls.color();
            let volume = controls.volume();
            let playing = controls.playing();

            for frame in data.chunks_mut(channels) {
                // On a color change, fade out, swap once silent, then fade back in.
                let switching = wanted_color != active_color;
                if switching && gain < 1e-3 {
                    active_color = wanted_color;
                }
                let target = if playing && !switching {
                    // Squared for a more natural-feeling volume slider.
                    volume * volume
                } else {
                    0.0
                };
                gain += (target - gain) * smoothing;

                for (sample, generator) in frame.iter_mut().zip(generators.iter_mut()) {
                    let value = generator.next(active_color) * gain;
                    *sample = T::from_sample(value.clamp(-1.0, 1.0));
                }
            }
        },
        |err| eprintln!("audio stream error: {err}"),
        None,
    )?;
    Ok(stream)
}
