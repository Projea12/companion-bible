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
    endpoint: String,
    /// Per-call deadline shared across retries.
    timeout_ms: u64,
}

impl AnthropicClient {
    pub fn new(api_key: impl Into<String>, timeout_ms: u64) -> Self {
        Self {
            api_key: api_key.into(),
            endpoint: ANTHROPIC_ENDPOINT.to_owned(),
            timeout_ms,
        }
    }

    /// Override the API endpoint — used in tests to point at a mock server.
    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = endpoint.into();
        self
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
            .post(&self.endpoint)
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

    fn valid_api_body(text: &str) -> String {
        format!(r#"{{"content":[{{"type":"text","text":"{text}"}}]}}"#)
    }

    // ── Backoff ───────────────────────────────────────────────────────────────

    #[test]
    fn backoff_doubles_per_attempt() {
        assert_eq!(BACKOFF_BASE_MS * (1 << 0), 100);
        assert_eq!(BACKOFF_BASE_MS * (1 << 1), 200);
    }

    // ── Timeout ───────────────────────────────────────────────────────────────

    #[test]
    fn timeout_error_when_deadline_already_expired() {
        let client = AnthropicClient::new("test-key", 0);
        assert!(matches!(client.complete("sys", "user"), Err(CloudAIError::Timeout(_))));
    }

    #[test]
    fn timeout_error_from_slow_server() {
        let mut server = mockito::Server::new();
        // Respond after the client has already given up.
        server.mock("POST", "/")
            .with_status(200)
            .with_body_from_fn(|_| {
                std::thread::sleep(std::time::Duration::from_millis(200));
                Ok(())
            })
            .create();

        // 1 ms timeout — server is far too slow.
        let client = AnthropicClient::new("key", 1)
            .with_endpoint(server.url());

        let result = client.complete("sys", "user");
        assert!(matches!(result, Err(CloudAIError::Timeout(_) | CloudAIError::Network(_))));
    }

    // ── Retry logic ───────────────────────────────────────────────────────────
    //
    // mockito matches mocks in LIFO order — create the fallback (200) first,
    // then the trigger (429/500) second so it fires on the first request.

    #[test]
    fn retries_on_429_exhausts_max_retries() {
        let mut server = mockito::Server::new();

        // All attempts return 429 — verifies the client retries MAX_RETRIES times.
        let _m = server.mock("POST", "/")
            .with_status(429)
            .expect(3) // initial + 2 retries
            .create();

        let client = AnthropicClient::new("key", 5_000)
            .with_endpoint(server.url());

        let result = client.complete("sys", "user");
        assert!(matches!(result, Err(CloudAIError::ApiError { status: 429, .. })));
        _m.assert(); // confirms exactly 3 calls were made
    }

    #[test]
    fn retries_on_500_then_fails_after_max_retries() {
        let mut server = mockito::Server::new();

        // All three attempts (initial + 2 retries) return 500.
        let _m = server.mock("POST", "/")
            .with_status(500)
            .with_body("internal error")
            .expect(3)
            .create();

        let client = AnthropicClient::new("key", 5_000)
            .with_endpoint(server.url());

        let result = client.complete("sys", "user");
        assert!(matches!(result, Err(CloudAIError::ApiError { status: 500, .. })));
        _m.assert(); // verifies exactly 3 calls were made
    }

    #[test]
    fn no_retry_on_401_unauthorized() {
        let mut server = mockito::Server::new();

        // Only one call should be made — 401 is not retryable.
        let _m = server.mock("POST", "/")
            .with_status(401)
            .expect(1)
            .create();

        let client = AnthropicClient::new("bad-key", 3_000)
            .with_endpoint(server.url());

        let result = client.complete("sys", "user");
        assert!(matches!(result, Err(CloudAIError::Unauthorized)));
        _m.assert(); // exactly 1 call, no retry
    }

    #[test]
    fn no_retry_on_400_bad_request() {
        let mut server = mockito::Server::new();

        let _m = server.mock("POST", "/")
            .with_status(400)
            .with_body("bad request")
            .expect(1)
            .create();

        let client = AnthropicClient::new("key", 3_000)
            .with_endpoint(server.url());

        let result = client.complete("sys", "user");
        assert!(matches!(result, Err(CloudAIError::ApiError { status: 400, .. })));
        _m.assert();
    }

    // ── Malformed API response ────────────────────────────────────────────────

    #[test]
    fn malformed_response_empty_content_array() {
        let mut server = mockito::Server::new();

        server.mock("POST", "/")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"content":[]}"#)
            .create();

        let client = AnthropicClient::new("key", 3_000)
            .with_endpoint(server.url());

        let result = client.complete("sys", "user");
        assert!(matches!(result, Err(CloudAIError::MalformedResponse { .. })));
    }

    #[test]
    fn malformed_response_non_json_body() {
        let mut server = mockito::Server::new();

        server.mock("POST", "/")
            .with_status(200)
            .with_body("this is not json at all")
            .create();

        let client = AnthropicClient::new("key", 3_000)
            .with_endpoint(server.url());

        let result = client.complete("sys", "user");
        assert!(result.is_err());
    }

    #[test]
    fn successful_response_returns_text() {
        let mut server = mockito::Server::new();
        let text = r#"{\"book\":\"John\",\"chapter\":3,\"verse\":16,\"confidence\":0.97,\"unattributed\":false}"#;

        server.mock("POST", "/")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(valid_api_body(text))
            .create();

        let client = AnthropicClient::new("key", 3_000)
            .with_endpoint(server.url());

        let result = client.complete("sys", "user");
        assert!(result.is_ok());
        assert!(result.unwrap().contains("John"));
    }

    // ── Error messages ────────────────────────────────────────────────────────

    #[test]
    fn error_messages_are_human_readable() {
        assert!(CloudAIError::Timeout(800).to_string().contains("800"));
        assert!(CloudAIError::ApiError { status: 429, body: "limit".into() }.to_string().contains("429"));
        assert!(CloudAIError::Unauthorized.to_string().contains("ANTHROPIC_API_KEY"));
        assert!(CloudAIError::Unavailable.to_string().contains("internet"));
    }
}
