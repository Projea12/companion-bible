use std::sync::{mpsc, Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;

use crate::capture::{AudioCapture, CaptureConfig, CaptureEvent};
use crate::error::AudioError;
use crate::input::{AudioInput, downsample};
use crate::preprocess::AudioPipeline;
use crate::ring_buffer::RingBuffer;
use crate::sliding_window::SlidingWindow;

/// Output sample rate for the sliding window and transcription backends.
const TRANSCRIPTION_RATE: u32 = 16_000;

// ─── SystemConfig ─────────────────────────────────────────────────────────────

/// Configuration for the full audio processing system.
pub struct SystemConfig {
    /// RMS threshold below which chunks are zeroed by the noise gate.
    pub gate_threshold: f32,
    /// Desired output RMS after normalisation.
    pub target_rms: f32,
    /// Capture-layer tuning (monitor interval, disconnect thresholds, etc.).
    pub capture: CaptureConfig,
}

impl Default for SystemConfig {
    fn default() -> Self {
        Self {
            gate_threshold: 0.02,
            target_rms: 0.1,
            capture: CaptureConfig::default(),
        }
    }
}

// ─── AudioSystem ──────────────────────────────────────────────────────────────

/// End-to-end audio processing system.
///
/// ## Data flow
/// ```text
/// cpal callback
///     │  Vec<f32>
///     ▼
/// RingBuffer<f32>        (lock-free, drop-oldest)
///     │  100 ms chunks
///     ▼
/// AudioPipeline          (gate → RNNoise → normalise)
///     │  clean samples
///     ▼
/// SlidingWindow          (last 30 s of clean audio)
/// ```
///
/// The capture layer runs on cpal's audio thread; the processor thread pulls
/// chunks from the ring buffer and feeds them into the pipeline and window.
pub struct AudioSystem {
    capture: AudioCapture,
    /// Pipeline-processed window: gate → RNNoise → normalize → 16 kHz.
    /// Use this for Whisper (local model needs clean, normalised audio).
    window: Arc<Mutex<SlidingWindow>>,
    /// Raw window: downsample-only → 16 kHz, zero extra processing.
    /// Use this for cloud APIs (AssemblyAI, Deepgram) which have their own
    /// internal noise handling and perform better without pre-processing.
    raw_window: Arc<Mutex<SlidingWindow>>,
    stop_flag: Arc<AtomicBool>,
    processor_handle: Option<JoinHandle<()>>,
}

impl AudioSystem {
    /// Create a system with the default [`SystemConfig`].
    pub fn new(input: Box<dyn AudioInput>, buffer: Arc<RingBuffer<f32>>) -> Self {
        Self::with_config(input, buffer, SystemConfig::default())
    }

    /// Create a system with a custom [`SystemConfig`].
    pub fn with_config(
        input: Box<dyn AudioInput>,
        buffer: Arc<RingBuffer<f32>>,
        config: SystemConfig,
    ) -> Self {
        let capture = AudioCapture::with_config(input, Arc::clone(&buffer), config.capture);
        let window = Arc::new(Mutex::new(SlidingWindow::new()));
        let raw_window = Arc::new(Mutex::new(SlidingWindow::new()));
        let stop_flag = Arc::new(AtomicBool::new(true));

        Self {
            capture,
            window,
            raw_window,
            stop_flag,
            processor_handle: None,
        }
    }

    // ── Observability ─────────────────────────────────────────────────────────

    /// Smoothed peak level in [0.0, 1.0] for a UI level meter.
    pub fn current_level(&self) -> f32 {
        self.capture.current_level()
    }

    /// `true` while the capture stream is running and the device is responding.
    pub fn is_connected(&self) -> bool {
        self.capture.is_connected()
    }

    /// Take the capture-event receiver (can only be called once).
    pub fn subscribe(&mut self) -> Option<mpsc::Receiver<CaptureEvent>> {
        self.capture.subscribe()
    }

    /// Pipeline-processed window (gate → RNNoise → normalize → 16 kHz).
    /// Pass this to Whisper; local models need clean, normalised audio.
    pub fn window(&self) -> Arc<Mutex<SlidingWindow>> {
        Arc::clone(&self.window)
    }

    /// Raw window (downsample-only → 16 kHz, no other processing).
    /// Pass this to cloud APIs such as AssemblyAI and Deepgram — they have
    /// their own internal noise suppression and work best on unmodified audio.
    pub fn raw_window(&self) -> Arc<Mutex<SlidingWindow>> {
        Arc::clone(&self.raw_window)
    }

    // ── Lifecycle ─────────────────────────────────────────────────────────────

    /// Start capturing and processing audio.
    ///
    /// Starts the cpal stream (via `AudioCapture::start`) then spawns a
    /// processor thread that drains `CHUNK_100MS`-sized blocks from the ring
    /// buffer, passes them through `AudioPipeline`, and pushes clean audio into
    /// the `SlidingWindow`.
    pub fn start(
        &mut self,
        buffer: Arc<RingBuffer<f32>>,
        gate_threshold: f32,
        target_rms: f32,
    ) -> Result<(), AudioError> {
        self.stop_join();

        self.stop_flag.store(false, Ordering::Release);
        self.capture.start()?;

        // Read the native device rate now that the stream is open.
        // Fall back to TRANSCRIPTION_RATE if somehow unavailable.
        let native_rate = {
            let r = self.capture.native_rate();
            if r == 0 { TRANSCRIPTION_RATE } else { r }
        };

        let window = Arc::clone(&self.window);
        let raw_window = Arc::clone(&self.raw_window);
        let stop_flag = Arc::clone(&self.stop_flag);

        self.processor_handle = Some(
            std::thread::Builder::new()
                .name("audio-processor".into())
                .spawn(move || {
                    processor_loop(buffer, window, raw_window, stop_flag, gate_threshold, target_rms, native_rate);
                })
                .expect("failed to spawn audio-processor"),
        );

        Ok(())
    }

    /// Stop capturing and processing, waiting for the processor thread to exit.
    pub fn stop(&mut self) {
        self.stop_join();
    }

    // ── Internal ──────────────────────────────────────────────────────────────

    fn stop_join(&mut self) {
        self.stop_flag.store(true, Ordering::Release);
        self.capture.stop();
        if let Some(h) = self.processor_handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for AudioSystem {
    fn drop(&mut self) {
        self.stop_flag.store(true, Ordering::Release);
        self.processor_handle.take();
    }
}

// ─── Processor loop ───────────────────────────────────────────────────────────

fn processor_loop(
    buffer: Arc<RingBuffer<f32>>,
    window: Arc<Mutex<SlidingWindow>>,
    raw_window: Arc<Mutex<SlidingWindow>>,
    stop_flag: Arc<AtomicBool>,
    gate_threshold: f32,
    target_rms: f32,
    native_rate: u32,
) {
    let chunk_size = (native_rate / 10) as usize; // 100 ms at native rate
    let mut pipeline = AudioPipeline::new(gate_threshold, target_rms);

    while !stop_flag.load(Ordering::Acquire) {
        let chunk = buffer.read(chunk_size);
        if chunk.is_empty() {
            std::thread::sleep(std::time::Duration::from_millis(5));
            continue;
        }

        // ── Raw window: downsample only, no pipeline processing ───────────────
        // Cloud APIs (AssemblyAI u3-rt-pro, Deepgram Nova-2) have their own
        // noise suppression models trained on real-world mic audio.  Sending
        // them pre-processed audio consistently degrades their accuracy.
        let raw_16k = if native_rate != TRANSCRIPTION_RATE {
            downsample(&chunk, native_rate, TRANSCRIPTION_RATE)
        } else {
            chunk.clone()
        };
        if !raw_16k.is_empty() {
            if let Ok(mut w) = raw_window.lock() {
                w.push(&raw_16k);
            }
        }

        // ── Processed window: gate → RNNoise → normalize → 16 kHz ────────────
        // Local Whisper needs clean, normalised audio; the pipeline helps here.
        let clean_native = pipeline.process(&chunk);
        if clean_native.is_empty() {
            continue;
        }
        let clean = if native_rate != TRANSCRIPTION_RATE {
            downsample(&clean_native, native_rate, TRANSCRIPTION_RATE)
        } else {
            clean_native
        };
        if !clean.is_empty() {
            if let Ok(mut w) = window.lock() {
                w.push(&clean);
            }
        }
    }

    // Drain RNNoise staging buffer into the processed window.
    let tail_native = pipeline.flush();
    if !tail_native.is_empty() {
        let tail = if native_rate != TRANSCRIPTION_RATE {
            downsample(&tail_native, native_rate, TRANSCRIPTION_RATE)
        } else {
            tail_native
        };
        if !tail.is_empty() {
            if let Ok(mut w) = window.lock() {
                w.push(&tail);
            }
        }
    }
}
