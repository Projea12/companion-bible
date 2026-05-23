use std::path::{Path, PathBuf};
use std::time::Instant;

use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::error::TranscriptionError;
use crate::transcript::{TranscribeOptions, TranscriptionSegment};

// ─── Constants ────────────────────────────────────────────────────────────────

/// Whisper small occupies ~500 MB of resident memory.
pub const MEMORY_BUDGET_MB: u64 = 2_048;

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

impl std::fmt::Debug for WhisperModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WhisperModel")
            .field("model_path", &self.model_path)
            .field("load_time_ms", &self.load_time_ms)
            .field("memory_delta_mb", &self.memory_delta_mb)
            .finish_non_exhaustive()
    }
}

// ─── WhisperModel impl ────────────────────────────────────────────────────────

impl WhisperModel {
    /// Load the GGML model at `path`.
    ///
    /// `on_progress` is called with a fraction in [0, 1] at key stages.
    /// whisper.cpp does not expose per-byte load progress; the callback fires
    /// before load starts (0.0) and after the context is ready (1.0).
    pub fn load<P, F>(path: P, mut on_progress: F) -> Result<Self, TranscriptionError>
    where
        P: AsRef<Path>,
        F: FnMut(f32),
    {
        let path = path.as_ref().to_path_buf();
        if !path.exists() {
            return Err(TranscriptionError::ModelNotFound(path));
        }
        let path_str = path.to_str().ok_or(TranscriptionError::InvalidPath)?;

        on_progress(0.0);

        let before_mb = rss_mb();
        let t0 = Instant::now();

        #[allow(unused_mut)]
        let mut ctx_params = WhisperContextParameters::default();
        #[cfg(target_os = "macos")]
        {
            ctx_params.use_gpu = true;
        }

        let ctx = WhisperContext::new_with_params(path_str, ctx_params)?;

        let load_time_ms = t0.elapsed().as_millis() as u64;
        let memory_delta_mb = rss_mb().saturating_sub(before_mb);

        on_progress(1.0);

        Ok(Self {
            ctx,
            model_path: path,
            load_time_ms,
            memory_delta_mb,
        })
    }

    /// Run a 0.1 s silence through the model to verify it is fully functional.
    pub fn health_check(&self) -> Result<HealthReport, TranscriptionError> {
        let mut state = self
            .ctx
            .create_state()
            .map_err(TranscriptionError::Whisper)?;

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

        let n_segments = state
            .full_n_segments()
            .map_err(TranscriptionError::Whisper)?;

        Ok(HealthReport {
            ok: true,
            inference_ms,
            n_segments,
        })
    }

    /// Transcribe `audio` (mono f32, 16 kHz, [-1, 1]) and return time-stamped
    /// segments.
    ///
    /// Segments whose no-speech probability exceeds
    /// `options.no_speech_threshold` are silently dropped, as are segments
    /// whose text is blank after trimming.
    ///
    /// Whisper processes the entire slice at once — for best results pass
    /// between 5 s and 30 s of audio.  The sliding-window buffer in the audio
    /// pipeline is sized at 30 s for exactly this reason.
    pub fn transcribe(
        &self,
        audio: &[f32],
        options: &TranscribeOptions,
    ) -> Result<Vec<TranscriptionSegment>, TranscriptionError> {
        let mut state = self
            .ctx
            .create_state()
            .map_err(TranscriptionError::Whisper)?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_n_threads(options.n_threads);
        params.set_temperature(options.temperature);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_suppress_blank(true);
        params.set_no_speech_thold(options.no_speech_threshold);

        if options.max_tokens > 0 {
            params.set_max_tokens(options.max_tokens);
        }
        if let Some(ref lang) = options.language {
            params.set_language(Some(lang.as_str()));
        }
        if !options.initial_prompt.is_empty() {
            params.set_initial_prompt(&options.initial_prompt);
        }

        let _rc = state
            .full(params, audio)
            .map_err(|e| TranscriptionError::Transcribe(e.to_string()))?;

        let n = state
            .full_n_segments()
            .map_err(TranscriptionError::Whisper)?;

        // ── First pass: collect text, timestamps, confidence ──────────────────
        struct RawSegment {
            text: String,
            audio_start_ms: u64,
            audio_end_ms: u64,
            whisper_confidence: f32,
        }

        let mut raw: Vec<RawSegment> = Vec::with_capacity(n as usize);
        for i in 0..n {
            let text = state
                .full_get_segment_text(i)
                .map_err(TranscriptionError::Whisper)?;
            let text = text.trim().to_string();
            if text.is_empty() {
                continue;
            }

            // Whisper timestamps are in centiseconds → milliseconds.
            let t0 = state
                .full_get_segment_t0(i)
                .map_err(TranscriptionError::Whisper)?;
            let t1 = state
                .full_get_segment_t1(i)
                .map_err(TranscriptionError::Whisper)?;

            // Mean token probability as the confidence estimate.
            let confidence = mean_token_prob(&state, i);

            raw.push(RawSegment {
                text,
                audio_start_ms: (t0 * 10).max(0) as u64,
                audio_end_ms: (t1 * 10).max(0) as u64,
                whisper_confidence: confidence,
            });
        }

        // ── Second pass: build context_window from neighbouring texts ─────────
        let texts: Vec<String> = raw.iter().map(|s| s.text.clone()).collect();
        let segments = raw
            .into_iter()
            .enumerate()
            .map(|(i, s)| {
                let prev = if i > 0 { texts[i - 1].as_str() } else { "" };
                let next = if i + 1 < texts.len() {
                    texts[i + 1].as_str()
                } else {
                    ""
                };
                let context_window = [prev, next]
                    .iter()
                    .filter(|t| !t.is_empty())
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(" ");

                TranscriptionSegment {
                    text: s.text,
                    audio_start_ms: s.audio_start_ms,
                    audio_end_ms: s.audio_end_ms,
                    whisper_confidence: s.whisper_confidence,
                    is_duplicate: false,
                    context_window,
                }
            })
            .collect();

        Ok(segments)
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

// ─── Internal helpers ─────────────────────────────────────────────────────────

/// Compute the mean token probability for segment `i` as a confidence proxy.
/// Returns `1.0` on any error (conservative — don't discard on failure).
fn mean_token_prob(state: &whisper_rs::WhisperState, segment: i32) -> f32 {
    let n_tokens = match state.full_n_tokens(segment) {
        Ok(n) => n,
        Err(_) => return 1.0,
    };
    if n_tokens == 0 {
        return 1.0;
    }
    let sum: f32 = (0..n_tokens)
        .filter_map(|t| state.full_get_token_prob(segment, t).ok())
        .sum();
    (sum / n_tokens as f32).clamp(0.0, 1.0)
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
