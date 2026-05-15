use std::path::{Path, PathBuf};
use std::time::Instant;

use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::error::TranscriptionError;

// ─── Constants ────────────────────────────────────────────────────────────────

/// SHA-1 of `ggml-medium.bin` as published by the whisper.cpp project.
pub const GGML_MEDIUM_SHA1: &str = "fd9727b6e1217c2f614f9b698455c4ffd82463b4";

/// HuggingFace download URL for the GGML medium model weights.
pub const GGML_MEDIUM_URL: &str =
    "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.bin";

/// Whisper medium occupies ~1.5 GB of resident memory.  We cap the model's
/// share of the 8 GB RAM budget at 4 GB to leave headroom for the rest of
/// the app.
pub const MEMORY_BUDGET_MB: u64 = 4_096;

// ─── WhisperModel ─────────────────────────────────────────────────────────────

/// A loaded Whisper GGML model ready for inference.
pub struct WhisperModel {
    pub(crate) ctx: WhisperContext,
    /// Path to the GGML weights file on disk.
    pub model_path: PathBuf,
    /// Wall-clock time to load the model from disk.
    pub load_time_ms: u64,
    /// Increase in resident memory caused by loading the model (best-effort).
    pub memory_delta_mb: u64,
}

// ─── HealthReport ─────────────────────────────────────────────────────────────

/// Outcome of a brief smoke-test inference on 0.1 s of silence.
pub struct HealthReport {
    /// `true` when inference completed without error.
    pub ok: bool,
    /// Wall-clock inference time in milliseconds.
    pub inference_ms: u64,
    /// Transcript segments produced (typically 0 for silence input).
    pub n_segments: i32,
}

// ─── WhisperModel impl ────────────────────────────────────────────────────────

impl WhisperModel {
    /// Load the GGML model at `path`.
    ///
    /// `on_progress` is called with a fraction in [0, 1] at key stages.
    /// whisper.cpp does not expose per-byte load progress; the callback fires
    /// before load starts (0.0) and after the context is ready (1.0).
    pub fn load<P, F>(path: P, on_progress: F) -> Result<Self, TranscriptionError>
    where
        P: AsRef<Path>,
        F: Fn(f32),
    {
        let path = path.as_ref().to_path_buf();
        if !path.exists() {
            return Err(TranscriptionError::ModelNotFound(path));
        }
        let path_str = path.to_str().ok_or(TranscriptionError::InvalidPath)?;

        on_progress(0.0);

        let before_mb = rss_mb();
        let t0 = Instant::now();

        let mut ctx_params = WhisperContextParameters::default();
        // Metal GPU acceleration on macOS — no-op on CPU-only builds.
        #[cfg(target_os = "macos")]
        {
            ctx_params.use_gpu = true;
        }

        let ctx = WhisperContext::new_with_params(path_str, ctx_params)?;

        let load_time_ms = t0.elapsed().as_millis() as u64;
        let memory_delta_mb = rss_mb().saturating_sub(before_mb);

        on_progress(1.0);

        Ok(Self { ctx, model_path: path, load_time_ms, memory_delta_mb })
    }

    /// Run a 0.1 s silence through the model to verify it is fully functional.
    pub fn health_check(&self) -> Result<HealthReport, TranscriptionError> {
        let mut state =
            self.ctx.create_state().map_err(TranscriptionError::Whisper)?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_n_threads(1);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_suppress_blank(true);

        // 0.1 s at 16 kHz — minimal work, just enough to warm up the graph.
        let samples = vec![0.0f32; 1_600];

        let t0 = Instant::now();
        let _rc = state
            .full(params, &samples)
            .map_err(|e| TranscriptionError::HealthCheck(e.to_string()))?;
        let inference_ms = t0.elapsed().as_millis() as u64;

        let n_segments =
            state.full_n_segments().map_err(TranscriptionError::Whisper)?;

        Ok(HealthReport { ok: true, inference_ms, n_segments })
    }

    /// Verify that the loaded model fits within [`MEMORY_BUDGET_MB`].
    pub fn assert_within_budget(&self) -> Result<(), TranscriptionError> {
        if self.memory_delta_mb > MEMORY_BUDGET_MB {
            return Err(TranscriptionError::HealthCheck(format!(
                "model uses {} MB, exceeds {MEMORY_BUDGET_MB} MB budget",
                self.memory_delta_mb
            )));
        }
        Ok(())
    }
}

// ─── Memory helpers ───────────────────────────────────────────────────────────

/// Best-effort resident set size of the current process in megabytes.
///
/// Returns 0 on platforms where measurement is not implemented.
pub fn rss_mb() -> u64 {
    #[cfg(target_os = "macos")]
    {
        let out = std::process::Command::new("ps")
            .args(["-o", "rss=", "-p", &std::process::id().to_string()])
            .output();
        if let Ok(o) = out {
            if let Ok(s) = std::str::from_utf8(&o.stdout) {
                if let Ok(kb) = s.trim().parse::<u64>() {
                    return kb / 1_024;
                }
            }
        }
        0
    }
    #[cfg(target_os = "linux")]
    {
        if let Ok(s) = std::fs::read_to_string("/proc/self/status") {
            for line in s.lines() {
                if line.starts_with("VmRSS:") {
                    if let Some(kb_str) = line.split_whitespace().nth(1) {
                        if let Ok(kb) = kb_str.parse::<u64>() {
                            return kb / 1_024;
                        }
                    }
                }
            }
        }
        0
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        0
    }
}
