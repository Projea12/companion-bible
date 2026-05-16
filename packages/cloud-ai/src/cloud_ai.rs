//! Cloud AI detection layer — wraps AnthropicClient with connectivity guard.

use std::time::Instant;

use serde::Deserialize;

use crate::client::{AnthropicClient, CloudAIError};
use crate::connectivity::ConnectivityMonitor;
use crate::prompt::CloudPromptBuilder;

// ─── Constants ────────────────────────────────────────────────────────────────

const TIMEOUT_MS: u64 = 800;

// ─── CloudAIResponse ─────────────────────────────────────────────────────────

/// Parsed JSON payload from the model.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct CloudAIResponse {
    pub book: Option<String>,
    pub chapter: Option<u8>,
    pub verse: Option<u8>,
    pub confidence: f32,
    #[serde(default)]
    pub unattributed: bool,
}

// ─── CloudAIResult ────────────────────────────────────────────────────────────

/// Result of a single `detect()` call.  Never panics — all outcomes are
/// represented as variants.
#[derive(Debug, Clone)]
pub enum CloudAIResult {
    /// Inference completed within the deadline.
    Ok {
        reference: Option<CloudAIResponse>,
        latency_ms: u64,
    },
    /// No internet connection at call time.
    Unavailable,
    /// Request exceeded the 800 ms deadline.
    Timeout { latency_ms: u64 },
    /// API or parse error.
    Error { reason: String, latency_ms: u64 },
}

impl CloudAIResult {
    pub fn is_ok(&self) -> bool {
        matches!(self, Self::Ok { .. })
    }

    pub fn is_unavailable(&self) -> bool {
        matches!(self, Self::Unavailable)
    }

    pub fn is_timeout(&self) -> bool {
        matches!(self, Self::Timeout { .. })
    }

    pub fn reference(&self) -> Option<&CloudAIResponse> {
        match self {
            Self::Ok { reference, .. } => reference.as_ref(),
            _ => None,
        }
    }
}

// ─── CloudAI ─────────────────────────────────────────────────────────────────

pub struct CloudAI {
    client: AnthropicClient,
}

impl CloudAI {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            client: AnthropicClient::new(api_key, TIMEOUT_MS),
        }
    }

    /// Primary entry point.  Builds the prompt from sermon context, calls the
    /// API, parses the JSON response, and returns a `CloudAIResult`.
    ///
    /// Returns `Unavailable` immediately when offline.
    /// Returns `Timeout` when the 800 ms deadline fires.
    pub fn detect(
        &self,
        segment_text: &str,
        active_book: Option<&str>,
        active_chapter: Option<u8>,
        recent_transcript: &str,
        anchor_scripture: Option<&str>,
    ) -> CloudAIResult {
        if !ConnectivityMonitor::is_connected() {
            return CloudAIResult::Unavailable;
        }

        let t0 = Instant::now();

        let (system, user) = CloudPromptBuilder::new()
            .with_context(active_book, active_chapter)
            .with_transcript(recent_transcript)
            .with_anchor(anchor_scripture.unwrap_or(""))
            .build(segment_text);

        match self.client.complete(&system, &user) {
            Ok(text) => {
                let latency_ms = t0.elapsed().as_millis() as u64;
                match parse_cloud_response(&text) {
                    Ok(response) => CloudAIResult::Ok {
                        reference: Some(response),
                        latency_ms,
                    },
                    Err(_) => CloudAIResult::Error {
                        reason: format!("malformed response: {text}"),
                        latency_ms,
                    },
                }
            }
            Err(CloudAIError::Timeout(_)) => CloudAIResult::Timeout {
                latency_ms: t0.elapsed().as_millis() as u64,
            },
            Err(CloudAIError::Unavailable) => CloudAIResult::Unavailable,
            Err(e) => CloudAIResult::Error {
                reason: e.to_string(),
                latency_ms: t0.elapsed().as_millis() as u64,
            },
        }
    }
}

// ─── Response parser ──────────────────────────────────────────────────────────

pub(crate) fn parse_cloud_response(raw: &str) -> Result<CloudAIResponse, ()> {
    let start = raw.find('{').ok_or(())?;
    let end   = raw.rfind('}').ok_or(())?;
    serde_json::from_str(&raw[start..=end]).map_err(|_| ())
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Parser ────────────────────────────────────────────────────────────────

    #[test]
    fn valid_response_parses_all_fields() {
        let raw = r#"{"book":"John","chapter":3,"verse":16,"confidence":0.97,"unattributed":false}"#;
        let r = parse_cloud_response(raw).unwrap();
        assert_eq!(r.book.as_deref(), Some("John"));
        assert_eq!(r.chapter, Some(3));
        assert_eq!(r.verse, Some(16));
        assert!((r.confidence - 0.97).abs() < 0.001);
        assert!(!r.unattributed);
    }

    #[test]
    fn unattributed_quotation_flag_parsed() {
        let raw = r#"{"book":"Psalms","chapter":23,"verse":1,"confidence":0.88,"unattributed":true}"#;
        let r = parse_cloud_response(raw).unwrap();
        assert!(r.unattributed);
        assert_eq!(r.book.as_deref(), Some("Psalms"));
    }

    #[test]
    fn unattributed_defaults_to_false_when_absent() {
        let raw = r#"{"book":"Romans","chapter":8,"verse":28,"confidence":0.94}"#;
        let r = parse_cloud_response(raw).unwrap();
        assert!(!r.unattributed);
    }

    #[test]
    fn null_book_parses_ok() {
        let raw = r#"{"book":null,"chapter":null,"verse":null,"confidence":0.1,"unattributed":false}"#;
        let r = parse_cloud_response(raw).unwrap();
        assert!(r.book.is_none());
    }

    #[test]
    fn leading_prose_stripped() {
        let raw = r#"Here is the reference: {"book":"Hebrews","chapter":11,"verse":1,"confidence":0.96,"unattributed":false}"#;
        let r = parse_cloud_response(raw).unwrap();
        assert_eq!(r.book.as_deref(), Some("Hebrews"));
    }

    #[test]
    fn malformed_json_returns_err() {
        assert!(parse_cloud_response("not json").is_err());
        assert!(parse_cloud_response("").is_err());
    }

    // ── CloudAIResult helpers ─────────────────────────────────────────────────

    #[test]
    fn result_ok_reports_correctly() {
        let result = CloudAIResult::Ok {
            reference: Some(CloudAIResponse {
                book: Some("John".into()),
                chapter: Some(3),
                verse: Some(16),
                confidence: 0.97,
                unattributed: false,
            }),
            latency_ms: 210,
        };
        assert!(result.is_ok());
        assert!(!result.is_timeout());
        assert!(!result.is_unavailable());
        assert_eq!(result.reference().unwrap().book.as_deref(), Some("John"));
    }

    #[test]
    fn result_unavailable_reports_correctly() {
        let result = CloudAIResult::Unavailable;
        assert!(result.is_unavailable());
        assert!(!result.is_ok());
        assert!(result.reference().is_none());
    }

    #[test]
    fn result_timeout_reports_correctly() {
        let result = CloudAIResult::Timeout { latency_ms: 810 };
        assert!(result.is_timeout());
        assert!(!result.is_ok());
    }

    // ── Accuracy on known references ──────────────────────────────────────────

    #[test]
    fn accuracy_john_3_16() {
        let raw = r#"{"book":"John","chapter":3,"verse":16,"confidence":0.98,"unattributed":false}"#;
        let r = parse_cloud_response(raw).unwrap();
        assert_eq!(r.book.as_deref(), Some("John"));
        assert_eq!(r.chapter, Some(3));
        assert_eq!(r.verse, Some(16));
        assert!(r.confidence > 0.9);
    }

    #[test]
    fn accuracy_romans_8_28() {
        let raw = r#"{"book":"Romans","chapter":8,"verse":28,"confidence":0.95,"unattributed":false}"#;
        let r = parse_cloud_response(raw).unwrap();
        assert_eq!(r.book.as_deref(), Some("Romans"));
        assert_eq!(r.chapter, Some(8));
        assert_eq!(r.verse, Some(28));
    }

    #[test]
    fn accuracy_unattributed_psalm_23() {
        // "The Lord is my shepherd" — model identifies without speaker naming it.
        let raw = r#"{"book":"Psalms","chapter":23,"verse":1,"confidence":0.91,"unattributed":true}"#;
        let r = parse_cloud_response(raw).unwrap();
        assert_eq!(r.book.as_deref(), Some("Psalms"));
        assert!(r.unattributed);
    }

    #[test]
    fn accuracy_low_confidence_returns_null_book() {
        let raw = r#"{"book":null,"chapter":null,"verse":null,"confidence":0.12,"unattributed":false}"#;
        let r = parse_cloud_response(raw).unwrap();
        assert!(r.book.is_none());
        assert!(r.confidence < 0.5);
    }
}
