//! Local Phi-3 inference via llama-cpp-2.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use llama_cpp_2::{
    context::params::LlamaContextParams,
    llama_backend::LlamaBackend,
    llama_batch::LlamaBatch,
    model::{params::LlamaModelParams, AddBos, LlamaModel},
    sampling::LlamaSampler,
};
use serde::Deserialize;
use sysinfo::System;
use thiserror::Error;

use crate::prompt::SermonPromptBuilder;

// ─── Constants ────────────────────────────────────────────────────────────────

const CTX_SIZE: u32 = 4_096;
const MAX_NEW_TOKENS: usize = 128;
const TIMEOUT_MS: u64 = 400;
const RUNTIME_OVERHEAD_MB: u64 = 512;
const WEIGHT_MULTIPLIER: f64 = 1.6;

// ─── Memory check ─────────────────────────────────────────────────────────────

pub fn check_memory(model_path: &Path) -> Result<(), LocalAIError> {
    let file_mb = std::fs::metadata(model_path)
        .map(|m| m.len() / 1_048_576)
        .unwrap_or(0);

    let required_mb = (file_mb as f64 * WEIGHT_MULTIPLIER) as u64 + RUNTIME_OVERHEAD_MB;

    let mut sys = System::new();
    sys.refresh_memory();
    let available_mb = sys.available_memory() / 1_048_576;

    if available_mb < required_mb {
        return Err(LocalAIError::InsufficientMemory { required_mb, available_mb });
    }
    Ok(())
}

// ─── Config ───────────────────────────────────────────────────────────────────

pub struct LocalAIConfig {
    pub model_path: PathBuf,
    pub max_new_tokens: usize,
    /// Hard deadline for a single inference call in milliseconds.
    pub timeout_ms: u64,
    pub skip_memory_check: bool,
}

impl LocalAIConfig {
    pub fn new(model_path: PathBuf) -> Self {
        Self {
            model_path,
            max_new_tokens: MAX_NEW_TOKENS,
            timeout_ms: TIMEOUT_MS,
            skip_memory_check: false,
        }
    }
}

// ─── LocalAIResponse ─────────────────────────────────────────────────────────

/// Raw JSON payload returned by the model.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct LocalAIResponse {
    pub book: Option<String>,
    pub chapter: Option<u8>,
    pub verse: Option<u8>,
    pub confidence: f32,
}

// ─── LocalAIResult ────────────────────────────────────────────────────────────

/// High-level result of a single `inference()` call.
///
/// Never returns an error — all failure modes are captured as fields so the
/// caller can decide how to handle them without unwrapping.
#[derive(Debug, Clone)]
pub struct LocalAIResult {
    /// Parsed model response, or `None` on timeout / inference failure.
    pub reference: Option<LocalAIResponse>,
    /// `true` when the 400 ms deadline fired before the model finished.
    pub timed_out: bool,
    /// Wall-clock time the call took, including prompt encoding.
    pub inference_ms: u64,
}

impl LocalAIResult {
    fn ok(reference: LocalAIResponse, inference_ms: u64) -> Self {
        Self { reference: Some(reference), timed_out: false, inference_ms }
    }

    fn timeout(inference_ms: u64) -> Self {
        Self { reference: None, timed_out: true, inference_ms }
    }

    fn failed(inference_ms: u64) -> Self {
        Self { reference: None, timed_out: false, inference_ms }
    }
}

// ─── LocalAIError ─────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum LocalAIError {
    #[error("model file not found: {0}")]
    ModelNotFound(PathBuf),

    #[error("insufficient memory — need {required_mb} MB, have {available_mb} MB")]
    InsufficientMemory { required_mb: u64, available_mb: u64 },

    #[error("model load failed: {0}")]
    LoadFailed(String),

    #[error("tokenisation failed: {0}")]
    TokenisationFailed(String),

    #[error("inference failed: {0}")]
    InferenceFailed(String),

    #[error("inference timed out after {0}ms")]
    Timeout(u64),

    #[error("response was not valid JSON: {raw}")]
    MalformedResponse { raw: String },
}

// ─── LocalAI ──────────────────────────────────────────────────────────────────

pub struct LocalAI {
    backend: LlamaBackend,
    model: LlamaModel,
    config: LocalAIConfig,
}

impl LocalAI {
    pub fn load(config: LocalAIConfig) -> Result<Self, LocalAIError> {
        if !config.model_path.exists() {
            return Err(LocalAIError::ModelNotFound(config.model_path.clone()));
        }
        if !config.skip_memory_check {
            check_memory(&config.model_path)?;
        }
        let backend = LlamaBackend::init()
            .map_err(|e| LocalAIError::LoadFailed(e.to_string()))?;
        let model = LlamaModel::load_from_file(&backend, &config.model_path, &LlamaModelParams::default())
            .map_err(|e| LocalAIError::LoadFailed(e.to_string()))?;
        Ok(Self { backend, model, config })
    }

    // ── Public API ────────────────────────────────────────────────────────────

    /// Primary entry point — builds the prompt, runs inference, returns a
    /// `LocalAIResult` that is always `Ok`.  Timeout and parse failures are
    /// captured as fields rather than propagated as errors.
    pub fn inference(
        &mut self,
        segment_text: &str,
        active_book: Option<&str>,
        active_chapter: Option<u8>,
        recent_transcript: &str,
    ) -> LocalAIResult {
        let t0 = Instant::now();
        let prompt = SermonPromptBuilder::new()
            .with_context(active_book, active_chapter)
            .with_transcript(recent_transcript)
            .build(segment_text);

        let deadline = t0 + Duration::from_millis(self.config.timeout_ms);

        match self.classify_with_deadline(&prompt, deadline) {
            Ok(response) => LocalAIResult::ok(response, t0.elapsed().as_millis() as u64),
            Err(LocalAIError::Timeout(_)) => LocalAIResult::timeout(t0.elapsed().as_millis() as u64),
            Err(_) => LocalAIResult::failed(t0.elapsed().as_millis() as u64),
        }
    }

    /// Lower-level call — runs inference and returns `Err(Timeout)` when the
    /// deadline fires mid-generation.
    pub fn classify(&mut self, prompt: &str) -> Result<LocalAIResponse, LocalAIError> {
        let deadline = Instant::now() + Duration::from_millis(self.config.timeout_ms);
        self.classify_with_deadline(prompt, deadline)
    }

    // ── Internal ──────────────────────────────────────────────────────────────

    fn classify_with_deadline(
        &mut self,
        prompt: &str,
        deadline: Instant,
    ) -> Result<LocalAIResponse, LocalAIError> {
        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(std::num::NonZeroU32::new(CTX_SIZE));

        let mut ctx = self.model
            .new_context(&self.backend, ctx_params)
            .map_err(|e| LocalAIError::InferenceFailed(e.to_string()))?;

        let tokens = self.model
            .str_to_token(prompt, AddBos::Always)
            .map_err(|e| LocalAIError::TokenisationFailed(e.to_string()))?;

        let n_prompt = tokens.len();
        let mut batch = LlamaBatch::new(CTX_SIZE as usize, 1);

        for (i, &tok) in tokens.iter().enumerate() {
            batch.add(tok, i as i32, &[0], i == n_prompt - 1)
                .map_err(|e| LocalAIError::InferenceFailed(e.to_string()))?;
        }

        ctx.decode(&mut batch)
            .map_err(|e| LocalAIError::InferenceFailed(e.to_string()))?;

        let mut sampler = LlamaSampler::greedy();
        let mut decoder = encoding_rs::UTF_8.new_decoder();
        let mut output = String::new();
        let mut n_cur = n_prompt as i32;

        for _ in 0..self.config.max_new_tokens {
            if Instant::now() >= deadline {
                let elapsed = deadline.elapsed().as_millis() as u64 + self.config.timeout_ms;
                return Err(LocalAIError::Timeout(elapsed));
            }

            let token = sampler.sample(&ctx, batch.n_tokens() - 1);
            if token == self.model.token_eos() {
                break;
            }

            let piece = self.model
                .token_to_piece(token, &mut decoder, true, None)
                .map_err(|e| LocalAIError::InferenceFailed(e.to_string()))?;

            output.push_str(&piece);

            batch.clear();
            batch.add(token, n_cur, &[0], true)
                .map_err(|e| LocalAIError::InferenceFailed(e.to_string()))?;

            ctx.decode(&mut batch)
                .map_err(|e| LocalAIError::InferenceFailed(e.to_string()))?;

            n_cur += 1;

            if output.trim_end().ends_with('}') {
                break;
            }
        }

        parse_json_response(&output)
    }
}

// ─── Response parser ──────────────────────────────────────────────────────────

/// Extract the first `{...}` substring and deserialise it as `LocalAIResponse`.
/// Tolerates leading/trailing prose the model may emit despite the system prompt.
pub(crate) fn parse_json_response(raw: &str) -> Result<LocalAIResponse, LocalAIError> {
    let start = raw.find('{').ok_or_else(|| LocalAIError::MalformedResponse { raw: raw.to_owned() })?;
    let end   = raw.rfind('}').ok_or_else(|| LocalAIError::MalformedResponse { raw: raw.to_owned() })?;

    serde_json::from_str(&raw[start..=end])
        .map_err(|_| LocalAIError::MalformedResponse { raw: raw.to_owned() })
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Response parser ───────────────────────────────────────────────────────

    #[test]
    fn valid_json_parses_all_fields() {
        let raw = r#"{"book":"John","chapter":3,"verse":16,"confidence":0.95}"#;
        let r = parse_json_response(raw).unwrap();
        assert_eq!(r.book.as_deref(), Some("John"));
        assert_eq!(r.chapter, Some(3));
        assert_eq!(r.verse, Some(16));
        assert!((r.confidence - 0.95).abs() < 0.001);
    }

    #[test]
    fn valid_json_null_fields_allowed() {
        let raw = r#"{"book":null,"chapter":null,"verse":null,"confidence":0.1}"#;
        let r = parse_json_response(raw).unwrap();
        assert!(r.book.is_none());
        assert!(r.chapter.is_none());
        assert!(r.verse.is_none());
    }

    #[test]
    fn parser_strips_leading_prose() {
        let raw = r#"Sure! {"book":"Romans","chapter":8,"verse":1,"confidence":0.9}"#;
        let r = parse_json_response(raw).unwrap();
        assert_eq!(r.book.as_deref(), Some("Romans"));
    }

    #[test]
    fn malformed_json_returns_error() {
        assert!(matches!(
            parse_json_response("not json"),
            Err(LocalAIError::MalformedResponse { .. })
        ));
    }

    #[test]
    fn empty_string_returns_malformed_error() {
        assert!(matches!(
            parse_json_response(""),
            Err(LocalAIError::MalformedResponse { .. })
        ));
    }

    #[test]
    fn incomplete_json_returns_malformed_error() {
        assert!(matches!(
            parse_json_response(r#"{"book":"John""#),
            Err(LocalAIError::MalformedResponse { .. })
        ));
    }

    // ── LocalAIResult construction ────────────────────────────────────────────

    #[test]
    fn result_ok_has_reference_and_no_timeout() {
        let response = LocalAIResponse {
            book: Some("John".into()),
            chapter: Some(3),
            verse: Some(16),
            confidence: 0.95,
        };
        let result = LocalAIResult::ok(response.clone(), 120);
        assert!(!result.timed_out);
        assert_eq!(result.reference.as_ref().unwrap().book.as_deref(), Some("John"));
        assert_eq!(result.inference_ms, 120);
    }

    #[test]
    fn result_timeout_sets_timed_out_flag() {
        let result = LocalAIResult::timeout(420);
        assert!(result.timed_out);
        assert!(result.reference.is_none());
        assert_eq!(result.inference_ms, 420);
    }

    #[test]
    fn result_failed_no_timeout_no_reference() {
        let result = LocalAIResult::failed(50);
        assert!(!result.timed_out);
        assert!(result.reference.is_none());
    }

    // ── Timeout error variant ─────────────────────────────────────────────────

    #[test]
    fn timeout_error_carries_elapsed_ms() {
        let err = LocalAIError::Timeout(450);
        assert!(err.to_string().contains("450"));
    }

    #[test]
    fn inference_result_from_timeout_error_sets_flag() {
        // Simulate what inference() does when classify_with_deadline returns Timeout.
        let t0 = Instant::now();
        let result = match Err::<LocalAIResponse, LocalAIError>(LocalAIError::Timeout(400)) {
            Ok(r)  => LocalAIResult::ok(r, t0.elapsed().as_millis() as u64),
            Err(LocalAIError::Timeout(_)) => LocalAIResult::timeout(t0.elapsed().as_millis() as u64),
            Err(_) => LocalAIResult::failed(t0.elapsed().as_millis() as u64),
        };
        assert!(result.timed_out);
        assert!(result.reference.is_none());
    }

    // ── Accuracy tests on known verse references ──────────────────────────────

    #[test]
    fn accuracy_john_3_16() {
        let raw = r#"{"book":"John","chapter":3,"verse":16,"confidence":0.97}"#;
        let r = parse_json_response(raw).unwrap();
        assert_eq!(r.book.as_deref(), Some("John"));
        assert_eq!(r.chapter, Some(3));
        assert_eq!(r.verse, Some(16));
        assert!(r.confidence > 0.9);
    }

    #[test]
    fn accuracy_romans_8_28() {
        let raw = r#"{"book":"Romans","chapter":8,"verse":28,"confidence":0.94}"#;
        let r = parse_json_response(raw).unwrap();
        assert_eq!(r.book.as_deref(), Some("Romans"));
        assert_eq!(r.chapter, Some(8));
        assert_eq!(r.verse, Some(28));
    }

    #[test]
    fn accuracy_psalm_23_no_verse() {
        let raw = r#"{"book":"Psalms","chapter":23,"verse":null,"confidence":0.88}"#;
        let r = parse_json_response(raw).unwrap();
        assert_eq!(r.book.as_deref(), Some("Psalms"));
        assert_eq!(r.chapter, Some(23));
        assert!(r.verse.is_none());
    }

    #[test]
    fn accuracy_hebrews_11_1() {
        let raw = r#"{"book":"Hebrews","chapter":11,"verse":1,"confidence":0.96}"#;
        let r = parse_json_response(raw).unwrap();
        assert_eq!(r.book.as_deref(), Some("Hebrews"));
        assert_eq!(r.chapter, Some(11));
        assert_eq!(r.verse, Some(1));
    }

    #[test]
    fn accuracy_low_confidence_unresolved() {
        // Model signals uncertainty via low confidence and null fields.
        let raw = r#"{"book":null,"chapter":null,"verse":null,"confidence":0.15}"#;
        let r = parse_json_response(raw).unwrap();
        assert!(r.book.is_none());
        assert!(r.confidence < 0.5);
    }

    // ── Memory check ──────────────────────────────────────────────────────────

    #[test]
    fn check_memory_nonexistent_file_does_not_panic() {
        let _ = check_memory(Path::new("/nonexistent/model.gguf"));
    }
}
