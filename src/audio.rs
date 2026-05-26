use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, Mutex};

/// Captures audio from the default microphone, resamples to 16kHz mono for Whisper.
pub struct Recorder {
    device: cpal::Device,
    config: cpal::SupportedStreamConfig,
    buffer: Arc<Mutex<Vec<f32>>>,
    stream: Option<cpal::Stream>,
}

impl Recorder {
    /// Create a new Recorder using the system's default input device.
    pub fn new() -> Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| anyhow!("No microphone found. Check your audio input settings."))?;

        let config = device.default_input_config()?;
        eprintln!(
            "   🎤 Microphone: {} ({}Hz, {} ch, {:?})",
            device.name().unwrap_or_else(|_| "Unknown".into()),
            config.sample_rate().0,
            config.channels(),
            config.sample_format()
        );

        Ok(Self {
            device,
            config,
            buffer: Arc::new(Mutex::new(Vec::new())),
            stream: None,
        })
    }

    /// Start recording audio from the microphone.
    pub fn start(&mut self) -> Result<()> {
        // Clear any previous recording
        self.buffer.lock().unwrap().clear();

        let buffer = self.buffer.clone();
        let config: cpal::StreamConfig = self.config.clone().into();
        let sample_format = self.config.sample_format();

        let err_fn = |err: cpal::StreamError| {
            eprintln!("   ❌ Audio stream error: {}", err);
        };

        let stream = match sample_format {
            cpal::SampleFormat::F32 => self.device.build_input_stream(
                &config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    buffer.lock().unwrap().extend_from_slice(data);
                },
                err_fn,
                None,
            )?,
            cpal::SampleFormat::I16 => {
                let buffer = self.buffer.clone();
                self.device.build_input_stream(
                    &config,
                    move |data: &[i16], _: &cpal::InputCallbackInfo| {
                        let floats: Vec<f32> =
                            data.iter().map(|&s| s as f32 / 32768.0).collect();
                        buffer.lock().unwrap().extend_from_slice(&floats);
                    },
                    err_fn,
                    None,
                )?
            }
            cpal::SampleFormat::U16 => {
                let buffer = self.buffer.clone();
                self.device.build_input_stream(
                    &config,
                    move |data: &[u16], _: &cpal::InputCallbackInfo| {
                        let floats: Vec<f32> = data
                            .iter()
                            .map(|&s| (s as f32 / 32768.0) - 1.0)
                            .collect();
                        buffer.lock().unwrap().extend_from_slice(&floats);
                    },
                    err_fn,
                    None,
                )?
            }
            _ => return Err(anyhow!("Unsupported sample format: {:?}", sample_format)),
        };

        stream.play()?;
        self.stream = Some(stream);
        Ok(())
    }

    /// Returns a clone of the shared audio buffer Arc so callers can monitor
    /// amplitude without holding a reference to the Recorder (which is !Send).
    pub fn buffer_arc(&self) -> Arc<Mutex<Vec<f32>>> {
        self.buffer.clone()
    }

    /// Stop recording and return the audio data as 16kHz mono f32 samples.
    pub fn stop(&mut self) -> Vec<f32> {
        // Drop the stream to stop recording
        self.stream = None;

        let raw_samples = self.buffer.lock().unwrap().clone();
        let channels = self.config.channels();
        let sample_rate = self.config.sample_rate().0;

        // Convert to mono
        let mono = to_mono(&raw_samples, channels);

        // Resample to 16kHz
        resample(&mono, sample_rate, 16_000)
    }
}

/// Convert multi-channel audio to mono by averaging channels.
fn to_mono(samples: &[f32], channels: u16) -> Vec<f32> {
    if channels == 1 {
        return samples.to_vec();
    }
    let ch = channels as usize;
    samples
        .chunks(ch)
        .map(|frame| frame.iter().sum::<f32>() / ch as f32)
        .collect()
}

/// Resample audio using linear interpolation.
/// Good enough quality for speech recognition — Whisper is very robust.
fn resample(input: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate {
        return input.to_vec();
    }

    let ratio = from_rate as f64 / to_rate as f64;
    let output_len = (input.len() as f64 / ratio) as usize;

    (0..output_len)
        .map(|i| {
            let src = i as f64 * ratio;
            let idx = src as usize;
            let frac = (src - idx as f64) as f32;

            if idx + 1 < input.len() {
                input[idx] * (1.0 - frac) + input[idx + 1] * frac
            } else if idx < input.len() {
                input[idx]
            } else {
                0.0
            }
        })
        .collect()
}
