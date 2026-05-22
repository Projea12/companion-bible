//! OpenAI Chat Completions client for scripture reference detection.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ─── Constants ────────────────────────────────────────────────────────────────

const OPENAI_ENDPOINT: &str = "https://api.openai.com/v1/chat/completions";
/// Fast, cheap, accurate — ideal for structured extraction.
pub const DETECTION_MODEL: &str = "gpt-4o-mini";
const MAX_TOKENS: u32 = 256;
const MAX_RETRIES: u32 = 2;
const BACKOFF_BASE_MS: u64 = 100;

// ─── Wire types ───────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    messages: Vec<ChatMessage<'a>>,
}

#[derive(Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: ChoiceMessage,
}

#[derive(Deserialize)]
struct ChoiceMessage {
    content: String,
}

// ─── OpenAIError ──────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum OpenAIError {
    #[error("no internet connection")]
    Unavailable,

    #[error("request timed out after {0}ms")]
    Timeout(u64),

    #[error("authentication failed — check OpenAI API key")]
    Unauthorized,

    #[error("rate limited after {attempts} attempts")]
    RateLimited { attempts: u32 },

    #[error("API error {status}: {body}")]
    ApiError { status: u16, body: String },

    #[error("response was not valid JSON: {raw}")]
    MalformedResponse { raw: String },

    #[error("network error: {0}")]
    Network(String),
}

// ─── OpenAIClient ─────────────────────────────────────────────────────────────

pub struct OpenAIClient {
    api_key: String,
    endpoint: String,
    timeout_ms: u64,
}

impl OpenAIClient {
    pub fn new(api_key: impl Into<String>, timeout_ms: u64) -> Self {
        Self {
            api_key: api_key.into(),
            endpoint: OPENAI_ENDPOINT.to_owned(),
            timeout_ms,
        }
    }

    #[cfg(test)]
    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = endpoint.into();
        self
    }

    pub fn complete(&self, system_prompt: &str, user_content: &str) -> Result<String, OpenAIError> {
        let deadline = Instant::now() + Duration::from_millis(self.timeout_ms);
        let mut last_err = None;

        for attempt in 0..=MAX_RETRIES {
            let remaining = deadline.checked_duration_since(Instant::now());
            let timeout = match remaining {
                Some(d) if d.as_millis() > 50 => d,
                _ => return Err(OpenAIError::Timeout(self.timeout_ms)),
            };

            match self.try_complete(system_prompt, user_content, timeout) {
                Ok(text) => return Ok(text),
                Err(e) => {
                    let should_retry = matches!(
                        &e,
                        OpenAIError::ApiError { status, .. } if *status == 429 || *status >= 500
                    ) || matches!(&e, OpenAIError::Network(_));

                    last_err = Some(e);

                    if !should_retry || attempt == MAX_RETRIES {
                        break;
                    }

                    let backoff = BACKOFF_BASE_MS * (1 << attempt);
                    std::thread::sleep(Duration::from_millis(backoff));
                }
            }
        }

        Err(last_err.unwrap_or(OpenAIError::Network("unknown error".into())))
    }

    fn try_complete(
        &self,
        system_prompt: &str,
        user_content: &str,
        timeout: Duration,
    ) -> Result<String, OpenAIError> {
        let client = reqwest::blocking::Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|e| OpenAIError::Network(e.to_string()))?;

        let body = ChatRequest {
            model: DETECTION_MODEL,
            max_tokens: MAX_TOKENS,
            messages: vec![
                ChatMessage {
                    role: "system",
                    content: system_prompt,
                },
                ChatMessage {
                    role: "user",
                    content: user_content,
                },
            ],
        };

        let response = client
            .post(&self.endpoint)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .map_err(|e| {
                if e.is_timeout() {
                    OpenAIError::Timeout(timeout.as_millis() as u64)
                } else if e.is_connect() {
                    OpenAIError::Unavailable
                } else {
                    OpenAIError::Network(e.to_string())
                }
            })?;

        let status = response.status().as_u16();

        if status == 401 {
            return Err(OpenAIError::Unauthorized);
        }

        if !response.status().is_success() {
            let body = response.text().unwrap_or_default();
            return Err(OpenAIError::ApiError { status, body });
        }

        let parsed: ChatResponse = response
            .json()
            .map_err(|e| OpenAIError::Network(e.to_string()))?;

        parsed
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .ok_or_else(|| OpenAIError::MalformedResponse {
                raw: "empty choices".into(),
            })
    }
}
