//! Conversions from each layer's native result type into the common
//! `LayerResult` used by the confidence arbitrator.

use companion_ai::LocalAIResult;
use companion_arbitrator::LayerResult;
use companion_cloud_ai::CloudAIResult;
use companion_context::EnrichedSegment;

// ─── pattern layer ────────────────────────────────────────────────────────────

/// Extract the highest-confidence `LayerResult` from the pattern detections
/// already embedded in an `EnrichedSegment`.
///
/// Returns `None` when the segment contained no pattern detections.
pub fn pattern_layer(enriched: &EnrichedSegment) -> Option<LayerResult> {
    enriched
        .detections
        .iter()
        .filter(|d| d.raw_match.confidence > 0.0)
        .max_by(|a, b| {
            a.raw_match
                .confidence
                .partial_cmp(&b.raw_match.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|d| {
            let r = &d.raw_match;
            LayerResult {
                book: r.book.clone(),
                chapter: r.chapter,
                verse: r.verse,
                confidence: r.confidence,
            }
        })
}

/// Extract the best `LayerResult` from a raw slice of `PatternResult`s.
///
/// Used to run the pattern engine over the rolling transcript buffer so that
/// references split across multiple Deepgram utterances can be re-assembled.
/// Prefers results that include a verse number over chapter-only results.
pub fn pattern_layer_from_results(
    results: &[companion_detection::PatternResult],
) -> Option<LayerResult> {
    // Prefer full references (with verse) over partial ones.
    let best = results
        .iter()
        .filter(|r| r.confidence > 0.0 && r.verse.is_some())
        .max_by(|a, b| {
            a.confidence
                .partial_cmp(&b.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

    best.map(|r| LayerResult {
        book: r.book.clone(),
        chapter: r.chapter,
        verse: r.verse,
        confidence: r.confidence,
    })
}

// ─── local AI layer ──────────────────────────────────────────────────────────

/// Convert a `LocalAIResult` into a `LayerResult`.
///
/// Returns `None` when the model timed out, failed, or returned no reference.
pub fn local_ai_layer(result: LocalAIResult) -> Option<LayerResult> {
    let response = result.reference?;
    response_to_layer(
        response.book,
        response.chapter,
        response.verse,
        response.confidence,
    )
}

// ─── cloud AI layer ──────────────────────────────────────────────────────────

/// Convert a `CloudAIResult` into a `LayerResult`.
///
/// Returns `None` on `Unavailable`, `Timeout`, `Error`, or when the cloud
/// model returned no reference.
pub fn cloud_ai_layer(result: CloudAIResult) -> Option<LayerResult> {
    match result {
        CloudAIResult::Ok {
            reference: Some(r), ..
        } => response_to_layer(r.book, r.chapter, r.verse, r.confidence),
        _ => None,
    }
}

// ─── helpers ──────────────────────────────────────────────────────────────────

fn response_to_layer(
    book: Option<String>,
    chapter: Option<u8>,
    verse: Option<u8>,
    confidence: f32,
) -> Option<LayerResult> {
    if confidence <= 0.0 {
        return None;
    }
    Some(LayerResult {
        book,
        chapter,
        verse,
        confidence,
    })
}

// ─── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use companion_ai::LocalAIResponse;
    use companion_cloud_ai::CloudAIResponse;
    use companion_context::{Detection, EnrichedSegment, ResolutionSource};
    use companion_detection::PatternResult;
    use companion_transcription::TranscriptionSegment;

    fn enriched_with_pattern(book: &str, chapter: u8, verse: u8, conf: f32) -> EnrichedSegment {
        let pattern = PatternResult {
            book: Some(book.into()),
            chapter: Some(chapter),
            verse: Some(verse),
            confidence: conf,
            completeness: companion_detection::MatchCompleteness::BookChapterVerse,
            start: 0,
            end: 5,
        };
        EnrichedSegment {
            segment: TranscriptionSegment {
                text: "test".into(),
                audio_start_ms: 0,
                audio_end_ms: 1000,
                whisper_confidence: 0.9,
                is_duplicate: false,
                context_window: String::new(),
            },
            corrected_text: "test".into(),
            normalized_text: "test".into(),
            detections: vec![Detection {
                raw_match: pattern,
                resolved: None,
                resolution_source: ResolutionSource::Unresolved,
            }],
            context_confidence: 0.0,
        }
    }

    // ── pattern_layer ─────────────────────────────────────────────────────────

    #[test]
    fn pattern_layer_extracts_highest_confidence() {
        let mut enriched = enriched_with_pattern("John", 3, 16, 0.80);
        // Add a second detection with lower confidence
        enriched.detections.push(Detection {
            raw_match: PatternResult {
                book: Some("Romans".into()),
                chapter: Some(8),
                verse: Some(28),
                confidence: 0.60,
                completeness: companion_detection::MatchCompleteness::BookChapterVerse,
                start: 10,
                end: 20,
            },
            resolved: None,
            resolution_source: ResolutionSource::Unresolved,
        });

        let layer = pattern_layer(&enriched).unwrap();
        assert_eq!(layer.book.as_deref(), Some("John"));
        assert_eq!(layer.confidence, 0.80);
    }

    #[test]
    fn pattern_layer_returns_none_when_no_detections() {
        let enriched = EnrichedSegment {
            segment: TranscriptionSegment {
                text: "no reference here".into(),
                audio_start_ms: 0,
                audio_end_ms: 1000,
                whisper_confidence: 0.8,
                is_duplicate: false,
                context_window: String::new(),
            },
            corrected_text: String::new(),
            normalized_text: String::new(),
            detections: vec![],
            context_confidence: 0.0,
        };
        assert!(pattern_layer(&enriched).is_none());
    }

    #[test]
    fn pattern_layer_skips_zero_confidence() {
        let mut enriched = enriched_with_pattern("John", 3, 16, 0.0);
        enriched.detections[0].raw_match.confidence = 0.0;
        assert!(pattern_layer(&enriched).is_none());
    }

    // ── local_ai_layer ────────────────────────────────────────────────────────

    #[test]
    fn local_ai_layer_converts_ok_result() {
        let result = LocalAIResult {
            reference: Some(LocalAIResponse {
                book: Some("Psalms".into()),
                chapter: Some(23),
                verse: Some(1),
                confidence: 0.88,
            }),
            timed_out: false,
            inference_ms: 310,
        };
        let layer = local_ai_layer(result).unwrap();
        assert_eq!(layer.book.as_deref(), Some("Psalms"));
        assert_eq!(layer.chapter, Some(23));
        assert_eq!(layer.confidence, 0.88);
    }

    #[test]
    fn local_ai_layer_returns_none_on_timeout() {
        let result = LocalAIResult {
            reference: None,
            timed_out: true,
            inference_ms: 400,
        };
        assert!(local_ai_layer(result).is_none());
    }

    #[test]
    fn local_ai_layer_returns_none_when_no_reference() {
        let result = LocalAIResult {
            reference: None,
            timed_out: false,
            inference_ms: 150,
        };
        assert!(local_ai_layer(result).is_none());
    }

    #[test]
    fn local_ai_layer_returns_none_on_zero_confidence() {
        let result = LocalAIResult {
            reference: Some(LocalAIResponse {
                book: Some("John".into()),
                chapter: Some(3),
                verse: Some(16),
                confidence: 0.0,
            }),
            timed_out: false,
            inference_ms: 200,
        };
        assert!(local_ai_layer(result).is_none());
    }

    // ── cloud_ai_layer ────────────────────────────────────────────────────────

    #[test]
    fn cloud_ai_layer_converts_ok_result() {
        let result = CloudAIResult::Ok {
            reference: Some(CloudAIResponse {
                book: Some("Hebrews".into()),
                chapter: Some(11),
                verse: Some(1),
                confidence: 0.94,
                unattributed: false,
            }),
            latency_ms: 620,
        };
        let layer = cloud_ai_layer(result).unwrap();
        assert_eq!(layer.book.as_deref(), Some("Hebrews"));
        assert_eq!(layer.confidence, 0.94);
    }

    #[test]
    fn cloud_ai_layer_returns_none_on_unavailable() {
        assert!(cloud_ai_layer(CloudAIResult::Unavailable).is_none());
    }

    #[test]
    fn cloud_ai_layer_returns_none_on_timeout() {
        assert!(cloud_ai_layer(CloudAIResult::Timeout { latency_ms: 800 }).is_none());
    }

    #[test]
    fn cloud_ai_layer_returns_none_on_error() {
        assert!(cloud_ai_layer(CloudAIResult::Error {
            reason: "api error".into(),
            latency_ms: 100,
        })
        .is_none());
    }

    #[test]
    fn cloud_ai_layer_returns_none_when_ok_but_no_reference() {
        assert!(cloud_ai_layer(CloudAIResult::Ok {
            reference: None,
            latency_ms: 200
        })
        .is_none());
    }
}
