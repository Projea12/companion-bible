//! Local Phi-3 inference via llama-cpp-2.

use std::path::{Path, PathBuf};

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
        return Err(LocalAIError::InsufficientMemory {
            required_mb,
            available_mb,
        });
    }
    Ok(())
}

// ─── Config / Response / Error ────────────────────────────────────────────────

pub struct LocalAIConfig {
    pub model_path: PathBuf,
    pub max_new_tokens: usize,
    pub skip_memory_check: bool,
}

impl LocalAIConfig {
    pub fn new(model_path: PathBuf) -> Self {
        Self {
            model_path,
            max_new_tokens: MAX_NEW_TOKENS,
            skip_memory_check: false,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct LocalAIResponse {
    pub book: Option<String>,
    pub chapter: Option<u8>,
    pub verse: Option<u8>,
    pub confidence: f32,
}

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

        let model_params = LlamaModelParams::default();
        let model = LlamaModel::load_from_file(&backend, &config.model_path, &model_params)
            .map_err(|e| LocalAIError::LoadFailed(e.to_string()))?;

        Ok(Self { backend, model, config })
    }

    /// Run greedy-decode inference on an already-formatted prompt string.
    ///
    /// A fresh `LlamaContext` is created per call to avoid self-referential
    /// lifetime issues when storing model and context together.
    pub fn classify(&mut self, prompt: &str) -> Result<LocalAIResponse, LocalAIError> {
        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(std::num::NonZeroU32::new(CTX_SIZE));

        let mut ctx = self
            .model
            .new_context(&self.backend, ctx_params)
            .map_err(|e| LocalAIError::InferenceFailed(e.to_string()))?;

        let tokens = self
            .model
            .str_to_token(prompt, AddBos::Always)
            .map_err(|e| LocalAIError::TokenisationFailed(e.to_string()))?;

        let n_prompt = tokens.len();
        let mut batch = LlamaBatch::new(CTX_SIZE as usize, 1);

        for (i, &tok) in tokens.iter().enumerate() {
            let is_last = i == n_prompt - 1;
            batch
                .add(tok, i as i32, &[0], is_last)
                .map_err(|e| LocalAIError::InferenceFailed(e.to_string()))?;
        }

        ctx.decode(&mut batch)
            .map_err(|e| LocalAIError::InferenceFailed(e.to_string()))?;

        let mut sampler = LlamaSampler::greedy();
        let mut decoder = encoding_rs::UTF_8.new_decoder();
        let mut output = String::new();
        let mut n_cur = n_prompt as i32;

        for _ in 0..self.config.max_new_tokens {
            let token = sampler.sample(&ctx, batch.n_tokens() - 1);

            if token == self.model.token_eos() {
                break;
            }

            let piece = self
                .model
                .token_to_piece(token, &mut decoder, true, None)
                .map_err(|e| LocalAIError::InferenceFailed(e.to_string()))?;

            output.push_str(&piece);

            batch.clear();
            batch
                .add(token, n_cur, &[0], true)
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

    pub fn classify_segment(
        &mut self,
        segment_text: &str,
        active_book: Option<&str>,
        active_chapter: Option<u8>,
        recent_transcript: &str,
    ) -> Result<LocalAIResponse, LocalAIError> {
        let prompt = SermonPromptBuilder::new()
            .with_context(active_book, active_chapter)
            .with_transcript(recent_transcript)
            .build(segment_text);

        self.classify(&prompt)
    }
}

// ─── JSON parsing ─────────────────────────────────────────────────────────────

fn parse_json_response(raw: &str) -> Result<LocalAIResponse, LocalAIError> {
    let start = raw.find('{').ok_or_else(|| LocalAIError::MalformedResponse {
        raw: raw.to_owned(),
    })?;
    let end = raw.rfind('}').ok_or_else(|| LocalAIError::MalformedResponse {
        raw: raw.to_owned(),
    })?;

    let json_str = &raw[start..=end];
    serde_json::from_str(json_str).map_err(|_| LocalAIError::MalformedResponse {
        raw: raw.to_owned(),
    })
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_well_formed_json() {
        let raw = r#"{"book":"John","chapter":3,"verse":16,"confidence":0.95}"#;
        let r = parse_json_response(raw).unwrap();
        assert_eq!(r.book.as_deref(), Some("John"));
        assert_eq!(r.chapter, Some(3));
        assert_eq!(r.verse, Some(16));
        assert!((r.confidence - 0.95).abs() < 0.001);
    }

    #[test]
    fn parse_json_with_null_book() {
        let raw = r#"{"book":null,"chapter":null,"verse":null,"confidence":0.1}"#;
        let r = parse_json_response(raw).unwrap();
        assert!(r.book.is_none());
    }

    #[test]
    fn parse_json_strips_leading_noise() {
        let raw = r#"Sure! {"book":"Romans","chapter":8,"verse":1,"confidence":0.9}"#;
        let r = parse_json_response(raw).unwrap();
        assert_eq!(r.book.as_deref(), Some("Romans"));
    }

    #[test]
    fn parse_malformed_json_returns_error() {
        let raw = "not json at all";
        assert!(matches!(
            parse_json_response(raw),
            Err(LocalAIError::MalformedResponse { .. })
        ));
    }

    #[test]
    fn check_memory_missing_file_uses_zero_size() {
        let path = Path::new("/nonexistent/model.gguf");
        let _ = check_memory(path);
    }
}
