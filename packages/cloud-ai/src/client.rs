//! Anthropic Messages API client with retry + exponential backoff.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use thiserror::Error;

// ─── Constants ────────────────────────────────────────────────────────────────

const ANTHROPIC_ENDPOINT: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";
/// Model used for scripture detection — fast and cost-efficient.
pub const DETECTION_MODEL: &str = "claude-haiku-4-5-20251001";
const MAX_TOKENS: u32 = 256;
const MAX_RETRIES: u32 = 2;
/// Initial backoff delay; doubles on each retry.
const BACKOFF_BASE_MS: u64 = 100;

// ─── Wire types ───────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct MessagesRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    system: &'a str,
    messages: Vec<Message<'a>>,
}

#[derive(Serialize)]
struct Message<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Deserialize)]
struct MessagesResponse {
    content: Vec<ContentBlock>,
}

#[derive(Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    kind: String,
    text: Option<String>,
}

// ─── CloudAIError ─────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum CloudAIError {
    #[error("no internet connection")]
    Unavailable,

    #[error("request timed out after {0}ms")]
    Timeout(u64),

    #[error("authentication failed — check ANTHROPIC_API_KEY")]
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

// ─── AnthropicClient ──────────────────────────────────────────────────────────

pub struct AnthropicClient {
    api_key: String,
    /// Per-call deadline shared across retries.
    timeout_ms: u64,
}

impl AnthropicClient {
    pub fn new(api_key: impl Into<String>, timeout_ms: u64) -> Self {
        Self { api_key: api_key.into(), timeout_ms }
    }

    /// Send `user_content` to Claude with `system_prompt`, retrying on
    /// transient failures.  Returns the text of the first content block.
    pub fn complete(
        &self,
        system_prompt: &str,
        user_content: &str,
    ) -> Result<String, CloudAIError> {
        let deadline = Instant::now() + Duration::from_millis(self.timeout_ms);
        let mut last_err = None;

        for attempt in 0..=MAX_RETRIES {
            let remaining = deadline.checked_duration_since(Instant::now());

            let timeout = match remaining {
                Some(d) if d.as_millis() > 50 => d,
                _ => return Err(CloudAIError::Timeout(self.timeout_ms)),
            };

            match self.try_complete(system_prompt, user_content, timeout) {
                Ok(text) => return Ok(text),
                Err(e) => {
                    let should_retry = matches!(
                        &e,
                        CloudAIError::ApiError { status, .. } if *status == 429
                            || *status >= 500
                    ) || matches!(&e, CloudAIError::Network(_));

                    last_err = Some(e);

                    if !should_retry || attempt == MAX_RETRIES {
                        break;
                    }

                    let backoff = BACKOFF_BASE_MS * (1 << attempt);
                    std::thread::sleep(Duration::from_millis(backoff));
                }
            }
        }

        Err(last_err.unwrap_or(CloudAIError::Network("unknown error".into())))
    }

    // ── Internal ──────────────────────────────────────────────────────────────

    fn try_complete(
        &self,
        system_prompt: &str,
        user_content: &str,
        timeout: Duration,
    ) -> Result<String, CloudAIError> {
        let client = reqwest::blocking::Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|e| CloudAIError::Network(e.to_string()))?;

        let body = MessagesRequest {
            model: DETECTION_MODEL,
            max_tokens: MAX_TOKENS,
            system: system_prompt,
            messages: vec![Message { role: "user", content: user_content }],
        };

        let response = client
            .post(ANTHROPIC_ENDPOINT)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .map_err(|e| {
                if e.is_timeout() {
                    CloudAIError::Timeout(timeout.as_millis() as u64)
                } else if e.is_connect() {
                    CloudAIError::Unavailable
                } else {
                    CloudAIError::Network(e.to_string())
                }
            })?;

        let status = response.status().as_u16();

        if status == 401 {
            return Err(CloudAIError::Unauthorized);
        }

        if !response.status().is_success() {
            let body = response.text().unwrap_or_default();
            return Err(CloudAIError::ApiError { status, body });
        }

        let parsed: MessagesResponse = response
            .json()
            .map_err(|e| CloudAIError::Network(e.to_string()))?;

        parsed
            .content
            .into_iter()
            .find(|b| b.kind == "text")
            .and_then(|b| b.text)
            .ok_or_else(|| CloudAIError::MalformedResponse { raw: "empty content".into() })
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_doubles_per_attempt() {
        assert_eq!(BACKOFF_BASE_MS * (1 << 0), 100);
        assert_eq!(BACKOFF_BASE_MS * (1 << 1), 200);
    }

    #[test]
    fn timeout_error_when_deadline_already_passed() {
        let client = AnthropicClient::new("test-key", 0);
        // timeout_ms=0 means deadline is already expired.
        let result = client.complete("system", "user");
        assert!(matches!(result, Err(CloudAIError::Timeout(_))));
    }

    #[test]
    fn cloud_ai_error_messages_are_readable() {
        let e = CloudAIError::Timeout(800);
        assert!(e.to_string().contains("800"));

        let e = CloudAIError::ApiError { status: 429, body: "rate limit".into() };
        assert!(e.to_string().contains("429"));

        let e = CloudAIError::RateLimited { attempts: 3 };
        assert!(e.to_string().contains("3"));
    }
}
