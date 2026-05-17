//! Local Phi-3 inference via llama-cpp-2 (optional feature: `local-llm`).
//! When `local-llm` is not enabled, LocalAI spawns a phi3-worker subprocess.

use std::path::{Path, PathBuf};
use std::time::Instant;

#[cfg(feature = "local-llm")]
use std::time::Duration;

#[cfg(feature = "local-llm")]
use llama_cpp_2::{
    context::params::LlamaContextParams,
    llama_backend::LlamaBackend,
    llama_batch::LlamaBatch,
    model::{params::LlamaModelParams, AddBos, LlamaModel},
    sampling::LlamaSampler,
};
use serde::Deserialize;
#[cfg(feature = "local-llm")]
use sysinfo::System;
use thiserror::Error;

#[cfg(feature = "local-llm")]
use crate::prompt::SermonPromptBuilder;

// ─── Constants ────────────────────────────────────────────────────────────────

#[cfg(feature = "local-llm")]
const CTX_SIZE: u32 = 4_096;
#[cfg(feature = "local-llm")]
const MAX_NEW_TOKENS: usize = 128;
#[cfg(feature = "local-llm")]
const TIMEOUT_MS: u64 = 400;
#[cfg(feature = "local-llm")]
const RUNTIME_OVERHEAD_MB: u64 = 512;
#[cfg(feature = "local-llm")]
const WEIGHT_MULTIPLIER: f64 = 1.6;

// ─── Memory check ─────────────────────────────────────────────────────────────

pub fn check_memory(model_path: &Path) -> Result<(), LocalAIError> {
    #[cfg(feature = "local-llm")]
    {
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
    }
    #[cfg(not(feature = "local-llm"))]
    let _ = model_path;
    Ok(())
}

// ─── Config ───────────────────────────────────────────────────────────────────

pub struct LocalAIConfig {
    pub model_path: PathBuf,
    pub max_new_tokens: usize,
    pub timeout_ms: u64,
    pub skip_memory_check: bool,
}

impl LocalAIConfig {
    pub fn new(model_path: PathBuf) -> Self {
        Self {
            model_path,
            max_new_tokens: 128,
            timeout_ms: 3_000,
            skip_memory_check: false,
        }
    }
}

// ─── LocalAIResponse ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct LocalAIResponse {
    pub book: Option<String>,
    pub chapter: Option<u8>,
    pub verse: Option<u8>,
    pub confidence: f32,
}

// ─── LocalAIResult ────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct LocalAIResult {
    pub reference: Option<LocalAIResponse>,
    pub timed_out: bool,
    pub inference_ms: u64,
}

impl LocalAIResult {
    pub(crate) fn ok(reference: LocalAIResponse, inference_ms: u64) -> Self {
        Self { reference: Some(reference), timed_out: false, inference_ms }
    }

    pub(crate) fn timeout(inference_ms: u64) -> Self {
        Self { reference: None, timed_out: true, inference_ms }
    }

    pub(crate) fn failed(inference_ms: u64) -> Self {
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

// ─── LocalAI (native llama-cpp-2 backend) ────────────────────────────────────

#[cfg(feature = "local-llm")]
pub struct LocalAI {
    backend: LlamaBackend,
    model: LlamaModel,
    config: LocalAIConfig,
}

#[cfg(feature = "local-llm")]
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
        let model = LlamaModel::load_from_file(
            &backend,
            &config.model_path,
            &LlamaModelParams::default(),
        )
        .map_err(|e| LocalAIError::LoadFailed(e.to_string()))?;
        Ok(Self { backend, model, config })
    }

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
            Err(LocalAIError::Timeout(_)) => {
                LocalAIResult::timeout(t0.elapsed().as_millis() as u64)
            }
            Err(_) => LocalAIResult::failed(t0.elapsed().as_millis() as u64),
        }
    }

    pub fn classify(&mut self, prompt: &str) -> Result<LocalAIResponse, LocalAIError> {
        let deadline = Instant::now() + Duration::from_millis(self.config.timeout_ms);
        self.classify_with_deadline(prompt, deadline)
    }

    fn classify_with_deadline(
        &mut self,
        prompt: &str,
        deadline: Instant,
    ) -> Result<LocalAIResponse, LocalAIError> {
        let ctx_params =
            LlamaContextParams::default().with_n_ctx(std::num::NonZeroU32::new(CTX_SIZE));
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
            batch
                .add(tok, i as i32, &[0], i == n_prompt - 1)
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
}

// ─── LocalAI (subprocess backend via phi3-worker) ────────────────────────────

#[cfg(not(feature = "local-llm"))]
pub struct LocalAI {
    _child: std::process::Child,
    stdin: std::io::BufWriter<std::process::ChildStdin>,
    rx: std::sync::mpsc::Receiver<String>,
    config: LocalAIConfig,
}

#[cfg(not(feature = "local-llm"))]
impl LocalAI {
    pub fn load(config: LocalAIConfig) -> Result<Self, LocalAIError> {
        let worker = find_worker_binary()?;

        let mut child = std::process::Command::new(&worker)
            .arg(&config.model_path)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .spawn()
            .map_err(|e| LocalAIError::LoadFailed(format!("failed to spawn phi3-worker: {e}")))?;

        let child_stdout = child
            .stdout
            .take()
            .ok_or_else(|| LocalAIError::LoadFailed("no stdout handle".into()))?;
        let child_stdin = child
            .stdin
            .take()
            .ok_or_else(|| LocalAIError::LoadFailed("no stdin handle".into()))?;

        let (tx, rx) = std::sync::mpsc::channel::<String>();
        std::thread::Builder::new()
            .name("phi3-reader".into())
            .spawn(move || {
                use std::io::BufRead;
                let reader = std::io::BufReader::new(child_stdout);
                for line in reader.lines() {
                    match line {
                        Ok(l) if !l.is_empty() => {
                            if tx.send(l).is_err() {
                                break;
                            }
                        }
                        _ => break,
                    }
                }
            })
            .map_err(|e| LocalAIError::LoadFailed(e.to_string()))?;

        Ok(Self {
            _child: child,
            stdin: std::io::BufWriter::new(child_stdin),
            rx,
            config,
        })
    }

    pub fn inference(
        &mut self,
        segment_text: &str,
        active_book: Option<&str>,
        active_chapter: Option<u8>,
        recent_transcript: &str,
    ) -> LocalAIResult {
        let t0 = Instant::now();

        let req = serde_json::json!({
            "text": segment_text,
            "book": active_book,
            "chapter": active_chapter,
            "recent": recent_transcript,
        });

        use std::io::Write;
        if writeln!(self.stdin, "{req}").is_err() || self.stdin.flush().is_err() {
            return LocalAIResult::failed(t0.elapsed().as_millis() as u64);
        }

        match self.rx.recv_timeout(std::time::Duration::from_millis(self.config.timeout_ms)) {
            Ok(line) => match parse_json_response(&line) {
                Ok(resp) => LocalAIResult::ok(resp, t0.elapsed().as_millis() as u64),
                Err(_) => LocalAIResult::failed(t0.elapsed().as_millis() as u64),
            },
            Err(_) => LocalAIResult::timeout(t0.elapsed().as_millis() as u64),
        }
    }

    pub fn classify(&mut self, prompt: &str) -> Result<LocalAIResponse, LocalAIError> {
        let result = self.inference(prompt, None, None, "");
        result
            .reference
            .ok_or_else(|| LocalAIError::LoadFailed("no result".into()))
    }
}

#[cfg(not(feature = "local-llm"))]
fn find_worker_binary() -> Result<PathBuf, LocalAIError> {
    if let Ok(exe) = std::env::current_exe() {
        let candidate = exe
            .parent()
            .unwrap_or(Path::new("."))
            .join("phi3-worker");
        if candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(LocalAIError::ModelNotFound(PathBuf::from("phi3-worker")))
}

// ─── Response parser ──────────────────────────────────────────────────────────

pub(crate) fn parse_json_response(raw: &str) -> Result<LocalAIResponse, LocalAIError> {
    let start = raw
        .find('{')
        .ok_or_else(|| LocalAIError::MalformedResponse { raw: raw.to_owned() })?;
    let end = raw
        .rfind('}')
        .ok_or_else(|| LocalAIError::MalformedResponse { raw: raw.to_owned() })?;
    serde_json::from_str(&raw[start..=end])
        .map_err(|_| LocalAIError::MalformedResponse { raw: raw.to_owned() })
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn result_ok_has_reference_and_no_timeout() {
        let response = LocalAIResponse {
            book: Some("John".into()),
            chapter: Some(3),
            verse: Some(16),
            confidence: 0.95,
        };
        let result = LocalAIResult::ok(response, 120);
        assert!(!result.timed_out);
        assert_eq!(result.reference.as_ref().unwrap().book.as_deref(), Some("John"));
        assert_eq!(result.inference_ms, 120);
    }

    #[test]
    fn result_timeout_sets_timed_out_flag() {
        let result = LocalAIResult::timeout(420);
        assert!(result.timed_out);
        assert!(result.reference.is_none());
    }

    #[test]
    fn result_failed_no_timeout_no_reference() {
        let result = LocalAIResult::failed(50);
        assert!(!result.timed_out);
        assert!(result.reference.is_none());
    }

    #[test]
    fn check_memory_nonexistent_file_does_not_panic() {
        let _ = check_memory(Path::new("/nonexistent/model.gguf"));
    }
}
