use std::collections::VecDeque;

use thiserror::Error;

// ─── Constants ────────────────────────────────────────────────────────────────

/// Rolling-window size (frames).
const WINDOW_SIZE: usize = 5;

/// Minimum speech frames in the window to declare `Speech`.
const SPEECH_FRAMES_NEEDED: usize = 3;

/// Default probability / RMS threshold for classifying a frame as speech.
pub const DEFAULT_THRESHOLD: f32 = 0.5;

/// Expected chunk size for 16 kHz audio (32 ms).
pub const CHUNK_SIZE: usize = 512;

// Neural-backend constants (unused unless feature enabled).
#[cfg(feature = "neural-vad")]
const HIDDEN_DIM: usize = 64;
#[cfg(feature = "neural-vad")]
const LSTM_LAYERS: usize = 2;
#[cfg(feature = "neural-vad")]
const SAMPLE_RATE: i64 = 16_000;

// ─── Public types ─────────────────────────────────────────────────────────────

/// The outcome of a single `detect()` call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VadDecision {
    Speech,
    Silence,
}

#[derive(Debug, Error)]
pub enum VadError {
    #[error("model file not found: {0}")]
    ModelNotFound(String),
    #[error("failed to load model: {0}")]
    ModelLoad(String),
    #[error("inference error: {0}")]
    Inference(String),
}

// ─── VoiceActivityDetector ────────────────────────────────────────────────────

/// Stateful VAD that combines per-frame probability with a rolling majority vote.
///
/// ## Decision algorithm
/// 1. Each `detect()` call produces a frame probability in [0, 1].
/// 2. A frame is labelled **speech** if `prob ≥ threshold`.
/// 3. The last `WINDOW_SIZE` (5) frame labels are kept in a rolling window.
/// 4. The window vote is `Speech` when **≥ 3 of 5** frames are speech.
///
/// ## Backends
/// * **Energy** (default): root-mean-square amplitude.  No model required;
///   always available; suitable for unit tests and simple environments.
/// * **Neural** (`neural-vad` feature): Silero VAD ONNX model via
///   `tract-onnx`.  Construct with `VoiceActivityDetector::from_model(path)`.
pub struct VoiceActivityDetector {
    backend: VadBackend,
    /// Rolling window of per-frame speech labels.
    window: VecDeque<bool>,
    /// Per-frame classification threshold (default `DEFAULT_THRESHOLD = 0.5`).
    prob_threshold: f32,
}

// ─── Backend ──────────────────────────────────────────────────────────────────

enum VadBackend {
    Energy,
    #[cfg(feature = "neural-vad")]
    Neural(NeuralState),
}

// ─── Constructors ─────────────────────────────────────────────────────────────

impl VoiceActivityDetector {
    /// Create a VAD using energy-based (RMS) detection.
    ///
    /// No model file required; always available.
    pub fn new_energy() -> Self {
        Self {
            backend: VadBackend::Energy,
            window: VecDeque::with_capacity(WINDOW_SIZE),
            prob_threshold: DEFAULT_THRESHOLD,
        }
    }

    /// Create a VAD using the Silero VAD ONNX neural model.
    ///
    /// `model_path` should point to `silero_vad.onnx` (~1 MB).
    /// The file is downloaded automatically at build time when the
    /// `neural-vad` feature is enabled and `curl` / `wget` is available.
    ///
    /// Falls back to energy detection if the `neural-vad` feature is disabled.
    #[cfg(feature = "neural-vad")]
    pub fn from_model(model_path: impl AsRef<std::path::Path>) -> Result<Self, VadError> {
        let path = model_path.as_ref();
        if !path.exists() {
            return Err(VadError::ModelNotFound(path.display().to_string()));
        }
        Ok(Self {
            backend: VadBackend::Neural(NeuralState::load(path)?),
            window: VecDeque::with_capacity(WINDOW_SIZE),
            prob_threshold: DEFAULT_THRESHOLD,
        })
    }

    /// Fallback constructor that matches the `from_model` signature even when
    /// `neural-vad` is disabled, returning an energy-based detector.
    #[cfg(not(feature = "neural-vad"))]
    pub fn from_model(_model_path: impl AsRef<std::path::Path>) -> Result<Self, VadError> {
        Ok(Self::new_energy())
    }
}

// ─── Core API ─────────────────────────────────────────────────────────────────

impl VoiceActivityDetector {
    /// Analyse one audio chunk and return a `Speech` / `Silence` decision.
    ///
    /// The chunk should contain `CHUNK_SIZE` (512) mono f32 samples at 16 kHz.
    /// Shorter chunks are accepted; longer chunks are truncated to `CHUNK_SIZE`.
    /// The decision is the **majority vote** over the last 5 frames:
    /// `Speech` when ≥ 3 of 5 frames cross the probability threshold.
    pub fn detect(&mut self, chunk: &[f32]) -> VadDecision {
        let chunk = &chunk[..chunk.len().min(CHUNK_SIZE)];

        let prob = match &mut self.backend {
            VadBackend::Energy => rms(chunk),
            #[cfg(feature = "neural-vad")]
            VadBackend::Neural(state) => {
                state.infer(chunk).unwrap_or_else(|_| rms(chunk))
            }
        };

        let is_speech = prob >= self.prob_threshold;

        if self.window.len() == WINDOW_SIZE {
            self.window.pop_front();
        }
        self.window.push_back(is_speech);

        let speech_count = self.window.iter().filter(|&&s| s).count();
        if speech_count >= SPEECH_FRAMES_NEEDED {
            VadDecision::Speech
        } else {
            VadDecision::Silence
        }
    }

    /// Set the per-frame classification threshold (clamped to [0, 1]).
    ///
    /// Higher values demand greater confidence before labelling a frame
    /// as speech — useful in noisy church environments.  Default: `0.5`.
    pub fn set_threshold(&mut self, threshold: f32) {
        self.prob_threshold = threshold.clamp(0.0, 1.0);
    }

    /// Current per-frame threshold.
    pub fn threshold(&self) -> f32 {
        self.prob_threshold
    }

    /// Calibrate the threshold from a sample of ambient noise.
    ///
    /// Splits `noise_samples` into `CHUNK_SIZE` frames, runs each through the
    /// backend, and sets the threshold to `baseline_mean × scale_factor`.
    ///
    /// * A `scale_factor` of **2.0** is a good starting point for typical
    ///   church acoustics (threshold lands 2× above the ambient noise floor).
    /// * A higher factor increases specificity (fewer false positives from
    ///   applause, music, etc.) at the cost of sensitivity.
    pub fn calibrate(&mut self, noise_samples: &[f32], scale_factor: f32) {
        if noise_samples.is_empty() {
            return;
        }

        let probs: Vec<f32> = noise_samples
            .chunks(CHUNK_SIZE)
            .filter(|c| !c.is_empty())
            .map(|chunk| match &mut self.backend {
                VadBackend::Energy => rms(chunk),
                #[cfg(feature = "neural-vad")]
                VadBackend::Neural(state) => state.infer(chunk).unwrap_or_else(|_| rms(chunk)),
            })
            .collect();

        if probs.is_empty() {
            return;
        }

        let mean = probs.iter().sum::<f32>() / probs.len() as f32;
        self.prob_threshold = (mean * scale_factor).clamp(0.0, 1.0);
    }

    /// Reset LSTM hidden state (neural backend) and clear the rolling window.
    ///
    /// Call at the start of each new sermon so previous session state does
    /// not bleed into the new session.
    pub fn reset(&mut self) {
        self.window.clear();
        #[cfg(feature = "neural-vad")]
        if let VadBackend::Neural(state) = &mut self.backend {
            state.reset();
        }
    }

    /// Number of frames currently in the rolling window.
    pub fn window_len(&self) -> usize {
        self.window.len()
    }

    /// A snapshot of the current rolling window (oldest → newest).
    pub fn window_snapshot(&self) -> Vec<bool> {
        self.window.iter().copied().collect()
    }
}

// ─── Energy helper ────────────────────────────────────────────────────────────

/// Root-mean-square amplitude, normalised to [0, 1] for f32 samples in [-1, 1].
fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let mean_sq = samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32;
    mean_sq.sqrt()
}

// ─── Neural backend (neural-vad feature) ─────────────────────────────────────

#[cfg(feature = "neural-vad")]
use tract_onnx::prelude::*;

#[cfg(feature = "neural-vad")]
type OnnxPlan = SimplePlan<TypedFact, Box<dyn TypedOp>, Graph<TypedFact, Box<dyn TypedOp>>>;

#[cfg(feature = "neural-vad")]
struct NeuralState {
    model: OnnxPlan,
    h: Tensor,
    c: Tensor,
}

#[cfg(feature = "neural-vad")]
impl NeuralState {
    fn load(path: &std::path::Path) -> Result<Self, VadError> {
        let model = tract_onnx::onnx()
            .model_for_path(path)
            .map_err(|e| VadError::ModelLoad(e.to_string()))?
            .into_optimized()
            .map_err(|e| VadError::ModelLoad(e.to_string()))?
            .into_runnable()
            .map_err(|e| VadError::ModelLoad(e.to_string()))?;

        Ok(Self {
            model,
            h: Self::zero_state(),
            c: Self::zero_state(),
        })
    }

    fn zero_state() -> Tensor {
        tract_ndarray::Array3::<f32>::zeros((LSTM_LAYERS, 1, HIDDEN_DIM)).into()
    }

    fn infer(&mut self, chunk: &[f32]) -> Result<f32, VadError> {
        // Pad to CHUNK_SIZE if the chunk is shorter.
        let mut padded = [0.0f32; CHUNK_SIZE];
        let n = chunk.len().min(CHUNK_SIZE);
        padded[..n].copy_from_slice(&chunk[..n]);

        let audio: Tensor =
            tract_ndarray::Array2::<f32>::from_shape_fn((1, CHUNK_SIZE), |(_, i)| padded[i])
                .into();
        let sr: Tensor = tract_ndarray::arr1(&[SAMPLE_RATE]).into();

        let mut outputs = self
            .model
            .run(tvec![audio, sr, self.h.clone(), self.c.clone()])
            .map_err(|e| VadError::Inference(e.to_string()))?;

        // Extract in reverse index order to avoid index shifting after remove().
        // outputs: [probability(0), hn(1), cn(2)]
        let cn = outputs.remove(2).into_tensor();
        let hn = outputs.remove(1).into_tensor();
        let prob_val = outputs.remove(0);

        let prob = prob_val
            .to_array_view::<f32>()
            .map_err(|e| VadError::Inference(e.to_string()))?
            .iter()
            .next()
            .copied()
            .unwrap_or(0.0);

        self.h = hn;
        self.c = cn;

        Ok(prob)
    }

    fn reset(&mut self) {
        self.h = Self::zero_state();
        self.c = Self::zero_state();
    }
}
