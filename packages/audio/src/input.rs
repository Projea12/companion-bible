use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleFormat, Stream};

use crate::device::{infer_device_type, AudioDevice, DeviceType};
use crate::error::AudioError;

// ─── Trait ────────────────────────────────────────────────────────────────────

pub trait AudioInput: Send + Sync {
    fn available_devices(&self) -> Result<Vec<AudioDevice>, AudioError>;
    fn select_device(&mut self, device_id: &str) -> Result<(), AudioError>;
    fn start(&mut self, callback: Box<dyn Fn(Vec<f32>) + Send + 'static>) -> Result<(), AudioError>;
    fn stop(&mut self);
    /// Peak level in [0.0, 1.0] from the most recent buffer.
    fn current_level(&self) -> f32;
}

// ─── Shared stream handle ─────────────────────────────────────────────────────

struct StreamHandle {
    _stream: Stream,
}

// cpal Stream is !Send + !Sync on some platforms (e.g. CoreAudio uses raw
// pointers internally).  We can soundly opt in to both because:
//   - start() / stop() both require &mut self, so there is never concurrent
//     access to the handle from multiple threads simultaneously.
//   - The stream callbacks are already bound as Send ('static Fn).
unsafe impl Send for StreamHandle {}
unsafe impl Sync for StreamHandle {}

// ─── CpalInput (shared implementation) ───────────────────────────────────────

pub(crate) struct CpalInput {
    selected_id: Option<String>,
    handle: Option<StreamHandle>,
    level: Arc<AtomicU32>,
    filter: Option<DeviceType>,
}

impl CpalInput {
    pub(crate) fn new(filter: Option<DeviceType>) -> Self {
        Self {
            selected_id: None,
            handle: None,
            level: Arc::new(AtomicU32::new(0)),
            filter,
        }
    }

    fn host_devices() -> Result<Vec<(cpal::Device, bool)>, AudioError> {
        let host = cpal::default_host();
        let default_name = host
            .default_input_device()
            .and_then(|d| d.name().ok())
            .unwrap_or_default();

        let devices = host
            .input_devices()
            .map_err(|e| AudioError::CpalDevice(e.to_string()))?;

        Ok(devices
            .filter_map(|d| {
                let is_default = d.name().ok().as_deref() == Some(&default_name);
                Some((d, is_default))
            })
            .collect())
    }
}

impl AudioInput for CpalInput {
    fn available_devices(&self) -> Result<Vec<AudioDevice>, AudioError> {
        let devs = Self::host_devices()?;
        let mut out = Vec::new();
        for (dev, is_default) in devs {
            let name = dev.name().map_err(|e| AudioError::CpalDevice(e.to_string()))?;
            let device_type = infer_device_type(&name);
            if let Some(ref f) = self.filter {
                if &device_type != f {
                    continue;
                }
            }
            out.push(AudioDevice {
                id: name.clone(),
                name,
                device_type,
                is_default,
            });
        }
        if out.is_empty() {
            return Err(AudioError::NoDevices);
        }
        Ok(out)
    }

    fn select_device(&mut self, device_id: &str) -> Result<(), AudioError> {
        let devs = Self::host_devices()?;
        let found = devs
            .iter()
            .any(|(d, _)| d.name().ok().as_deref() == Some(device_id));
        if !found {
            return Err(AudioError::DeviceNotFound(device_id.to_string()));
        }
        self.selected_id = Some(device_id.to_string());
        Ok(())
    }

    fn start(&mut self, callback: Box<dyn Fn(Vec<f32>) + Send + 'static>) -> Result<(), AudioError> {
        self.stop();

        let host = cpal::default_host();
        let device = if let Some(ref id) = self.selected_id {
            Self::host_devices()?
                .into_iter()
                .find(|(d, _)| d.name().ok().as_deref() == Some(id.as_str()))
                .map(|(d, _)| d)
                .ok_or_else(|| AudioError::DeviceNotFound(id.clone()))?
        } else {
            host.default_input_device().ok_or(AudioError::NoDevices)?
        };

        let config = device
            .default_input_config()
            .map_err(|e| AudioError::CpalConfig(e.to_string()))?;

        let native_rate = config.sample_rate().0;
        let native_channels = config.channels() as usize;
        eprintln!(
            "[audio] device='{}' native={}Hz/{}ch/{:?}",
            device.name().unwrap_or_default(),
            native_rate, native_channels, config.sample_format()
        );

        let level = Arc::clone(&self.level);
        let err_fn = |e: cpal::StreamError| eprintln!("audio stream error: {e}");

        // Build the stream at the device's native rate, then downsample to
        // 16 kHz in the callback so the SlidingWindow and Deepgram always
        // receive 16 kHz mono audio regardless of the device's native rate.
        let stream = match config.sample_format() {
            SampleFormat::F32 => device.build_input_stream(
                &config.into(),
                move |data: &[f32], _| {
                    let mono = to_mono_f32(data, native_channels);
                    let resampled = downsample(&mono, native_rate, 16_000);
                    let peak = resampled.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
                    level.store(peak.to_bits(), Ordering::Relaxed);
                    callback(resampled);
                },
                err_fn,
                None,
            ),
            SampleFormat::I16 => device.build_input_stream(
                &config.into(),
                move |data: &[i16], _| {
                    let f32s: Vec<f32> = data.iter().map(|&s| s as f32 / i16::MAX as f32).collect();
                    let mono = to_mono_f32(&f32s, native_channels);
                    let resampled = downsample(&mono, native_rate, 16_000);
                    let peak = resampled.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
                    level.store(peak.to_bits(), Ordering::Relaxed);
                    callback(resampled);
                },
                err_fn,
                None,
            ),
            SampleFormat::U16 => device.build_input_stream(
                &config.into(),
                move |data: &[u16], _| {
                    let f32s: Vec<f32> = data.iter().map(|&s| (s as f32 / u16::MAX as f32) * 2.0 - 1.0).collect();
                    let mono = to_mono_f32(&f32s, native_channels);
                    let resampled = downsample(&mono, native_rate, 16_000);
                    let peak = resampled.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
                    level.store(peak.to_bits(), Ordering::Relaxed);
                    callback(resampled);
                },
                err_fn,
                None,
            ),
            fmt => return Err(AudioError::StreamBuild(format!("unsupported sample format: {fmt:?}"))),
        }
        .map_err(|e| AudioError::StreamBuild(e.to_string()))?;

        stream
            .play()
            .map_err(|e| AudioError::StreamPlay(e.to_string()))?;
        self.handle = Some(StreamHandle { _stream: stream });
        Ok(())
    }

    fn stop(&mut self) {
        self.handle.take();
        self.level.store(0u32, Ordering::Relaxed);
    }

    fn current_level(&self) -> f32 {
        f32::from_bits(self.level.load(Ordering::Relaxed))
    }
}

// ─── Concrete types ───────────────────────────────────────────────────────────

/// Captures audio from a line-in / sound board mixer.
pub struct MixerInput(CpalInput);

impl MixerInput {
    pub fn new() -> Self {
        Self(CpalInput::new(Some(DeviceType::Mixer)))
    }
}

impl Default for MixerInput {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioInput for MixerInput {
    fn available_devices(&self) -> Result<Vec<AudioDevice>, AudioError> {
        self.0.available_devices()
    }
    fn select_device(&mut self, id: &str) -> Result<(), AudioError> {
        self.0.select_device(id)
    }
    fn start(&mut self, cb: Box<dyn Fn(Vec<f32>) + Send + 'static>) -> Result<(), AudioError> {
        self.0.start(cb)
    }
    fn stop(&mut self) {
        self.0.stop()
    }
    fn current_level(&self) -> f32 {
        self.0.current_level()
    }
}

/// Captures audio from a USB microphone.
pub struct UsbMicInput(CpalInput);

impl UsbMicInput {
    pub fn new() -> Self {
        Self(CpalInput::new(Some(DeviceType::UsbMic)))
    }
}

impl Default for UsbMicInput {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioInput for UsbMicInput {
    fn available_devices(&self) -> Result<Vec<AudioDevice>, AudioError> {
        self.0.available_devices()
    }
    fn select_device(&mut self, id: &str) -> Result<(), AudioError> {
        self.0.select_device(id)
    }
    fn start(&mut self, cb: Box<dyn Fn(Vec<f32>) + Send + 'static>) -> Result<(), AudioError> {
        self.0.start(cb)
    }
    fn stop(&mut self) {
        self.0.stop()
    }
    fn current_level(&self) -> f32 {
        self.0.current_level()
    }
}

/// Captures audio from the built-in / on-board microphone.
pub struct BuiltinMicInput(CpalInput);

impl BuiltinMicInput {
    pub fn new() -> Self {
        Self(CpalInput::new(Some(DeviceType::Builtin)))
    }
}

impl Default for BuiltinMicInput {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioInput for BuiltinMicInput {
    fn available_devices(&self) -> Result<Vec<AudioDevice>, AudioError> {
        self.0.available_devices()
    }
    fn select_device(&mut self, id: &str) -> Result<(), AudioError> {
        self.0.select_device(id)
    }
    fn start(&mut self, cb: Box<dyn Fn(Vec<f32>) + Send + 'static>) -> Result<(), AudioError> {
        self.0.start(cb)
    }
    fn stop(&mut self) {
        self.0.stop()
    }
    fn current_level(&self) -> f32 {
        self.0.current_level()
    }
}

// ─── Audio helpers ────────────────────────────────────────────────────────────

/// Average multi-channel interleaved samples down to mono.
fn to_mono_f32(samples: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return samples.to_vec();
    }
    samples
        .chunks_exact(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect()
}

/// Linear-interpolation downsample from `from_hz` to `to_hz`.
fn downsample(samples: &[f32], from_hz: u32, to_hz: u32) -> Vec<f32> {
    if from_hz == to_hz || samples.is_empty() {
        return samples.to_vec();
    }
    let ratio = from_hz as f64 / to_hz as f64;
    let out_len = ((samples.len() as f64) / ratio).ceil() as usize;
    let mut out = Vec::with_capacity(out_len);
    let mut pos = 0.0f64;
    while pos < samples.len() as f64 {
        let i = pos as usize;
        let frac = (pos - i as f64) as f32;
        let s0 = samples[i];
        let s1 = samples.get(i + 1).copied().unwrap_or(s0);
        out.push(s0 + (s1 - s0) * frac);
        pos += ratio;
    }
    out
}
