pub mod capture;
mod device;
mod error;
mod input;
pub mod preprocess;
pub mod ring_buffer;
pub mod sliding_window;
pub mod system;
pub mod vad;

pub use capture::{AudioCapture, CaptureConfig, CaptureEvent};
pub use device::{AudioDevice, DeviceType};
pub use error::AudioError;
pub use input::{AudioInput, BuiltinMicInput, MixerInput, UsbMicInput};
pub use ring_buffer::{RingBuffer, DEFAULT_CAPACITY};
pub use sliding_window::{AudioWindow, SlidingWindow, SAMPLE_RATE, WINDOW_CAPACITY, WINDOW_SECS};
pub use preprocess::{
    AmplitudeNormalizer, AudioPipeline, AudioPreprocessor, NoiseGate, NoiseSuppressor,
    CHUNK_100MS, RNNOISE_FRAME_SIZE,
};
pub use system::{AudioSystem, SystemConfig};
pub use vad::{VadDecision, VadError, VoiceActivityDetector, CHUNK_SIZE, DEFAULT_THRESHOLD};

#[cfg(test)]
mod tests;
