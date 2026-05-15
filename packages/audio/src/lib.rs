pub mod capture;
mod device;
mod error;
mod input;
pub mod preprocess;
pub mod ring_buffer;
pub mod vad;

pub use capture::{AudioCapture, CaptureConfig, CaptureEvent};
pub use device::{AudioDevice, DeviceType};
pub use error::AudioError;
pub use input::{AudioInput, BuiltinMicInput, MixerInput, UsbMicInput};
pub use ring_buffer::{RingBuffer, DEFAULT_CAPACITY};
pub use preprocess::{
    AmplitudeNormalizer, AudioPipeline, AudioPreprocessor, NoiseGate, NoiseSuppressor,
    CHUNK_100MS, RNNOISE_FRAME_SIZE,
};
pub use vad::{VadDecision, VadError, VoiceActivityDetector, CHUNK_SIZE, DEFAULT_THRESHOLD};

#[cfg(test)]
mod tests;
