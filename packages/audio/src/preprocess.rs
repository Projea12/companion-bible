use nnnoiseless::DenoiseState;

// ─── Constants ────────────────────────────────────────────────────────────────

/// RNNoise processes audio in frames of exactly this many samples.
pub const RNNOISE_FRAME_SIZE: usize = DenoiseState::FRAME_SIZE; // 480

/// RNNoise expects samples in the 16-bit PCM amplitude range.
/// Our pipeline uses normalised f32 in [-1, 1], so we scale on the way in
/// and back on the way out.
const PCM_SCALE: f32 = 32_768.0;

// ─── NoiseGate ────────────────────────────────────────────────────────────────

/// Hard amplitude gate: zeroes an audio chunk whose RMS falls below a
/// configurable threshold.
///
/// Applied **after** RNNoise in the `AudioPreprocessor` pipeline, the gate
/// removes quiet pauses and residual low-level noise that RNNoise leaves
/// behind.  It is also useful standalone for very noisy environments where
/// the pastor's microphone is only active when speaking.
///
/// # Threshold guidance
/// * `0.02` – gentle gate, only removes near-silence
/// * `0.05` – recommended starting point for typical church environments
/// * `0.10` – aggressive gate; ensure the speaker's normal level is well above
pub struct NoiseGate {
    threshold: f32,
}

impl NoiseGate {
    /// Create a gate with the given RMS `threshold` (normalised, [0, 1]).
    pub fn new(threshold: f32) -> Self {
        Self { threshold: threshold.clamp(0.0, 1.0) }
    }

    /// Threshold below which a chunk is zeroed.
    pub fn threshold(&self) -> f32 {
        self.threshold
    }

    /// Adjust the threshold (clamped to [0, 1]).
    pub fn set_threshold(&mut self, threshold: f32) {
        self.threshold = threshold.clamp(0.0, 1.0);
    }

    /// Gate `samples` in place.
    ///
    /// Computes the RMS of the whole slice.  If it is below the threshold, all
    /// samples are set to zero.  Otherwise the chunk passes through unchanged.
    pub fn process(&self, samples: &mut [f32]) {
        if samples.is_empty() {
            return;
        }
        if chunk_rms(samples) < self.threshold {
            samples.iter_mut().for_each(|s| *s = 0.0);
        }
    }

    /// Return `true` if the chunk's RMS would be gated out.
    pub fn would_gate(&self, samples: &[f32]) -> bool {
        chunk_rms(samples) < self.threshold
    }
}

// ─── AudioPreprocessor ────────────────────────────────────────────────────────

/// Two-stage audio preprocessor: RNNoise denoising followed by a noise gate.
///
/// ## Pipeline
/// ```text
/// raw audio  →  [RNNoise 480-sample frames]  →  [NoiseGate]  →  clean audio
/// ```
///
/// ## Sample rate note
/// RNNoise was trained on 48 kHz audio (480 samples = 10 ms per frame).
/// The preprocessor operates on whatever rate you feed it; no resampling is
/// performed internally.  For our 16 kHz pipeline each 480-sample frame
/// represents 30 ms of audio.  The noise model still suppresses broadband
/// stationary noise effectively at 16 kHz, though its frequency-bin labelling
/// is shifted relative to the training distribution.  Proper 48 kHz resampling
/// can be layered on top if needed in a later task.
///
/// ## Buffering
/// Input chunks do not have to be multiples of `RNNOISE_FRAME_SIZE`.
/// The preprocessor buffers incomplete frames internally; call [`flush`] at
/// the end of a stream to drain the buffer with zero-padding.
pub struct AudioPreprocessor {
    denoiser: Box<DenoiseState<'static>>,
    gate: Option<NoiseGate>,
    /// Staging buffer — accumulates samples until a full RNNoise frame is ready.
    staging: Vec<f32>,
}

impl Default for AudioPreprocessor {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioPreprocessor {
    /// Create a preprocessor with RNNoise enabled and no noise gate.
    pub fn new() -> Self {
        Self {
            denoiser: DenoiseState::new(),
            gate: None,
            staging: Vec::with_capacity(RNNOISE_FRAME_SIZE * 2),
        }
    }

    /// Create a preprocessor with RNNoise **and** a noise gate at `threshold`.
    pub fn with_gate(threshold: f32) -> Self {
        Self {
            denoiser: DenoiseState::new(),
            gate: Some(NoiseGate::new(threshold)),
            staging: Vec::with_capacity(RNNOISE_FRAME_SIZE * 2),
        }
    }

    // ── Configuration ─────────────────────────────────────────────────────────

    /// Enable (or update) the noise gate threshold.
    pub fn set_gate_threshold(&mut self, threshold: f32) {
        match &mut self.gate {
            Some(g) => g.set_threshold(threshold),
            None => self.gate = Some(NoiseGate::new(threshold)),
        }
    }

    /// Disable the noise gate.
    pub fn disable_gate(&mut self) {
        self.gate = None;
    }

    /// Current gate threshold, or `None` if the gate is disabled.
    pub fn gate_threshold(&self) -> Option<f32> {
        self.gate.as_ref().map(|g| g.threshold())
    }

    // ── Processing ────────────────────────────────────────────────────────────

    /// Push `input` through the preprocessing pipeline.
    ///
    /// Returns denoised (and optionally gated) samples.  Fewer samples than
    /// `input.len()` may be returned if the internal buffer has not yet
    /// accumulated a full RNNoise frame.
    pub fn process(&mut self, input: &[f32]) -> Vec<f32> {
        self.staging.extend_from_slice(input);

        let complete_frames = self.staging.len() / RNNOISE_FRAME_SIZE;
        if complete_frames == 0 {
            return Vec::new();
        }

        let total_to_process = complete_frames * RNNOISE_FRAME_SIZE;
        let frames: Vec<f32> = self.staging.drain(..total_to_process).collect();
        let mut output = Vec::with_capacity(total_to_process);

        for frame in frames.chunks_exact(RNNOISE_FRAME_SIZE) {
            let processed = self.process_frame(frame);
            output.extend_from_slice(&processed);
        }

        output
    }

    /// Drain the internal buffer by zero-padding the last partial frame.
    ///
    /// Call this at the end of a recording to ensure all audio is returned.
    pub fn flush(&mut self) -> Vec<f32> {
        if self.staging.is_empty() {
            return Vec::new();
        }
        let mut padded = std::mem::take(&mut self.staging);
        padded.resize(RNNOISE_FRAME_SIZE, 0.0);
        self.process_frame(&padded)
    }

    // ── Internal ──────────────────────────────────────────────────────────────

    fn process_frame(&mut self, frame: &[f32]) -> Vec<f32> {
        debug_assert_eq!(frame.len(), RNNOISE_FRAME_SIZE);

        // Scale normalised f32 → PCM range for RNNoise.
        let mut pcm_in = [0.0f32; RNNOISE_FRAME_SIZE];
        for (dst, &src) in pcm_in.iter_mut().zip(frame) {
            *dst = src * PCM_SCALE;
        }

        let mut pcm_out = [0.0f32; RNNOISE_FRAME_SIZE];
        // Returns RNNoise's own VAD probability (discarded — we use our own VAD).
        let _ = self.denoiser.process_frame(&mut pcm_out, &pcm_in);

        // Scale PCM range → normalised f32.
        let mut out: Vec<f32> = pcm_out.iter().map(|&s| s / PCM_SCALE).collect();

        // Apply gate post-denoising.
        if let Some(gate) = &self.gate {
            gate.process(&mut out);
        }

        out
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn chunk_rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let mean_sq = samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32;
    mean_sq.sqrt()
}
