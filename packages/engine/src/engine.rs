//! `DetectionEngine` — orchestrates all three detection layers, arbitration,
//! validation, context tracking, event emission, and database logging.

use std::sync::Arc;
use std::time::Instant;

use companion_arbitrator::{ArbitrationDecision, ConfidenceArbitrator, PartialResults};
use companion_bible::{BibleValidator, KjvBible, ValidationResult};
use companion_calibration::{CalibrationError, ChurchCalibrator};
use companion_cloud_ai::CloudAI;
use companion_context::SermonContext;
use companion_database::{
    CalibrationRepository, ChurchRepository, DetectionEvent, DetectionEventRepository,
    VerseRepository,
};
use companion_events::{AppEvent, BibleReference as EventBibleReference};
use companion_transcription::TranscriptionSegment;

use crate::decision::{DetectionDecision, ValidationOutcome};
use crate::layers;
use crate::quotation;
use crate::worker::{collect_local_ai, LocalAiHandle};

// ─── timing constants ────────────────────────────────────────────────────────

/// How long (ms) to wait for the local AI reply after submitting the job.
/// Matches LocalAI's internal 400 ms budget plus a small margin.
const LOCAL_AI_WAIT_MS: u64 = 450;

/// How long (ms) to wait for the cloud AI once arbitration says we need it.
/// This is the budget remaining after pattern + local AI have already run.
const CLOUD_WAIT_BUDGET_MS: u64 = 800;

// ─── EngineConfig ─────────────────────────────────────────────────────────────

pub struct EngineConfig {
    /// Identifies the current sermon session in the database.
    pub sermon_id: String,

    /// Anthropic API key.  `None` disables the cloud layer.
    pub api_key: Option<String>,
}

// ─── DetectionEngine ─────────────────────────────────────────────────────────

/// Holds every component needed to process a `TranscriptionSegment` from raw
/// audio through to a validated `DetectionDecision`.
pub struct DetectionEngine {
    // ── detection layers ──────────────────────────────────────────────────────
    local_ai: Option<Arc<LocalAiHandle>>,
    cloud_ai: Option<Arc<CloudAI>>,

    // ── pipeline components ───────────────────────────────────────────────────
    context: SermonContext,
    arbitrator: ConfidenceArbitrator,
    bible: KjvBible,
    calibrator: ChurchCalibrator,

    // ── persistence / events ──────────────────────────────────────────────────
    detection_repo: DetectionEventRepository,
    verse_repo: Option<VerseRepository>,
    sermon_id: String,
    event_tx: Option<std::sync::mpsc::Sender<AppEvent>>,
}

impl DetectionEngine {
    /// Construct the engine, loading calibration state from the database.
    ///
    /// `local_ai_handle` may be `None` when the Phi-3 model has not been
    /// downloaded or failed to load.  The engine degrades gracefully to
    /// pattern + cloud only.
    pub async fn new(
        config: EngineConfig,
        bible: KjvBible,
        church_repo: ChurchRepository,
        calibration_repo: CalibrationRepository,
        detection_repo: DetectionEventRepository,
        verse_repo: VerseRepository,
        local_ai_handle: Option<LocalAiHandle>,
    ) -> Result<Self, CalibrationError> {
        let calibrator = ChurchCalibrator::load(church_repo, calibration_repo).await?;

        let mut arbitrator = ConfidenceArbitrator::default();
        arbitrator.auto_display_threshold = calibrator.thresholds().auto_display;
        arbitrator.amber_threshold = calibrator.thresholds().show_with_warning;

        let local_ai = local_ai_handle.map(|h| Arc::new(h));
        let cloud_ai = config.api_key.map(|k| Arc::new(CloudAI::new(k)));

        Ok(Self {
            local_ai,
            cloud_ai,
            context: SermonContext::new(),
            arbitrator,
            bible,
            calibrator,
            detection_repo,
            verse_repo: Some(verse_repo),
            sermon_id: config.sermon_id,
            event_tx: None,
        })
    }

    /// Attach an event channel.  Every `AppEvent` produced by `process` is
    /// forwarded to `tx`; a disconnected sender is silently ignored.
    pub fn set_event_sender(&mut self, tx: std::sync::mpsc::Sender<AppEvent>) {
        self.event_tx = Some(tx);
    }

    /// Set the anchor scripture for this sermon session.
    pub fn set_anchor(&mut self, anchor: EventBibleReference) {
        self.context.set_anchor(anchor);
    }

    /// Current calibrated thresholds (auto_display / show_with_warning).
    pub fn thresholds(&self) -> &companion_calibration::CalibrationThresholds {
        self.calibrator.thresholds()
    }

    // ── process ───────────────────────────────────────────────────────────────

    /// Process one transcription segment through the full pipeline and return
    /// a `DetectionDecision`.
    ///
    /// This method is `async` only to allow async DB logging; all CPU-bound
    /// work runs synchronously or on dedicated background threads.
    pub async fn process(&mut self, segment: TranscriptionSegment) -> DetectionDecision {
        let t0 = Instant::now();

        // ── 1. Enrich with context ─────────────────────────────────────────
        let enriched = self.context.enrich(segment);
        let text = enriched.normalized_text.clone();
        let transcript = self.context.rolling_transcript.text();
        let active_book = self.context.active_book.clone();
        let active_chapter = self.context.active_chapter;
        let anchor = self
            .context
            .anchor_scripture
            .as_ref()
            .map(|r| r.to_string());

        // ── 2. Pattern layer (sync, embedded in enrichment) ───────────────
        // First try the current utterance alone.  If it doesn't yield a full
        // reference (book + chapter + verse), re-run the pattern engine over
        // the rolling transcript buffer — this re-assembles references that
        // Deepgram fragmented across several short utterances (e.g. "John." /
        // "chapter 3." / "verse 16" each arriving as separate events).
        let segment_pattern = layers::pattern_layer(&enriched);
        let has_full_ref = segment_pattern.as_ref()
            .map(|r| r.book.is_some() && r.chapter.is_some() && r.verse.is_some())
            .unwrap_or(false);
        let pattern_result = if has_full_ref {
            segment_pattern
        } else {
            let rolling = layers::pattern_layer_from_results(
                &self.context.find_in_rolling_transcript(),
            );
            rolling.or(segment_pattern)
        };

        // ── 2c. Quotation layer — FTS5 match when pattern layers found nothing ──
        // Runs only when pattern + rolling gave no verse, to avoid latency on
        // the fast path. Uses the rolling transcript so partial utterances
        // accumulate enough text before a match fires.
        let pattern_result = if pattern_result.as_ref().map(|r| r.verse.is_some()).unwrap_or(false) {
            pattern_result
        } else {
            let quotation_result = self
                .quotation_layer(&transcript, active_book.as_deref(), active_chapter)
                .await;
            quotation_result.or(pattern_result)
        };

        // ── 3. Submit to local AI worker (non-blocking) ───────────────────
        let local_rx = self.local_ai.as_ref().and_then(|h| {
            h.try_submit(
                text.clone(),
                active_book.clone(),
                active_chapter,
                transcript.clone(),
            )
        });

        // ── 4. Spawn cloud AI task ─────────────────────────────────────────
        let cloud_handle = self.cloud_ai.as_ref().map(|cloud| {
            let cloud = cloud.clone();
            let text_c = text.clone();
            let transcript_c = transcript.clone();
            let book_c = active_book.clone();
            let anchor_c = anchor.clone();
            tokio::task::spawn_blocking(move || {
                cloud.detect(
                    &text_c,
                    book_c.as_deref(),
                    active_chapter,
                    &transcript_c,
                    anchor_c.as_deref(),
                )
            })
        });

        // ── 5. Build partial results with pattern only ─────────────────────
        let mut partial = PartialResults {
            pattern: pattern_result,
            local_ai_pending: local_rx.is_some(),
            cloud_pending: cloud_handle.is_some(),
            elapsed_ms: t0.elapsed().as_millis() as u64,
            ..Default::default()
        };

        // ── 6. Collect local AI result ─────────────────────────────────────
        if let Some(rx) = local_rx {
            let elapsed = t0.elapsed().as_millis() as u64;
            let budget = LOCAL_AI_WAIT_MS.saturating_sub(elapsed).max(10);
            if let Some(r) = collect_local_ai(rx, budget) {
                partial.local_ai = layers::local_ai_layer(r);
            }
            partial.local_ai_pending = false;
            partial.elapsed_ms = t0.elapsed().as_millis() as u64;
        }

        // ── 7. Mid-point arbitration (pattern + local AI) ─────────────────
        let mid = self.arbitrator.arbitrate(&partial);

        // ── 8. Conditionally wait for cloud ───────────────────────────────
        if let Some(handle) = cloud_handle {
            if mid.should_wait_for_cloud {
                let elapsed = t0.elapsed().as_millis() as u64;
                let budget = CLOUD_WAIT_BUDGET_MS.saturating_sub(elapsed).max(10);
                let timeout = tokio::time::Duration::from_millis(budget);
                match tokio::time::timeout(timeout, handle).await {
                    Ok(Ok(result)) => partial.cloud = layers::cloud_ai_layer(result),
                    _ => {}
                }
            } else {
                handle.abort();
            }
            partial.cloud_pending = false;
        }
        partial.elapsed_ms = t0.elapsed().as_millis() as u64;

        // ── 9. Final arbitration ───────────────────────────────────────────
        let final_decision = self.arbitrator.arbitrate(&partial);
        let processing_ms = t0.elapsed().as_millis() as u64;

        // ── 10. Resolve winning reference ─────────────────────────────────
        let event_ref = layer_result_to_event_ref(&final_decision);

        // ── 11. Validate ──────────────────────────────────────────────────
        let validation = match &event_ref {
            Some(r) => validate_reference(&self.bible, r),
            None => ValidationOutcome::NoReference,
        };

        // ── 12. Emit AppEvent ─────────────────────────────────────────────
        let event = build_event(&event_ref, &validation, &text);
        self.emit_event(event);

        // ── 13. Update context ────────────────────────────────────────────
        if let (Some(r), true) = (&event_ref, validation.is_valid()) {
            self.context.update(r.clone(), processing_ms);
        }

        // ── 14. Log to database ───────────────────────────────────────────
        self.log_event(&partial, &final_decision, &event_ref, &validation, &text, processing_ms)
            .await;

        // ── 15. Build decision ────────────────────────────────────────────
        let reference = if validation.is_valid() { event_ref } else { None };

        // ── 15a. Fuzzy fallback — if no verse detected but context known ──
        if reference.is_none() {
            if let (Some(book), Some(chapter)) = (&active_book, active_chapter) {
                if let Some((verse, _score)) =
                    crate::fuzzy::fuzzy_verse_match(&text, &self.bible, book, chapter)
                {
                    let fuzzy_ref =
                        EventBibleReference::new(book.clone(), chapter).with_verse(verse);
                    if validate_reference(&self.bible, &fuzzy_ref).is_valid() {
                        self.emit_event(AppEvent::ScriptureReferenceDetected {
                            references: vec![fuzzy_ref],
                            source_text: text.clone(),
                        });
                    }
                }
            }
        }

        DetectionDecision {
            reference,
            confidence: final_decision.confidence,
            action: final_decision.action,
            validation,
            all_layers_agreed: final_decision.all_agree,
            processing_ms,
        }
    }

    // ── private helpers ───────────────────────────────────────────────────────

    async fn quotation_layer(
        &self,
        transcript: &str,
        book: Option<&str>,
        chapter: Option<u8>,
    ) -> Option<companion_arbitrator::LayerResult> {
        let repo = self.verse_repo.as_ref()?;
        let candidates = repo.search_fts(transcript, book, chapter, 8).await.ok()?;
        if candidates.is_empty() {
            return None;
        }
        quotation::best_quotation_match(&candidates, transcript)
    }

    fn emit_event(&self, event: AppEvent) {
        if let Some(tx) = &self.event_tx {
            let _ = tx.send(event);
        }
    }

    async fn log_event(
        &self,
        partial: &PartialResults,
        decision: &ArbitrationDecision,
        reference: &Option<EventBibleReference>,
        validation: &ValidationOutcome,
        text: &str,
        processing_ms: u64,
    ) {
        let id = format!(
            "{}-{}",
            self.sermon_id,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        );

        let final_reference = if validation.is_valid() {
            reference.as_ref().map(|r| r.to_string())
        } else {
            None
        };

        let event = DetectionEvent {
            id,
            sermon_id: self.sermon_id.clone(),
            raw_transcript: text.to_string(),
            pattern_result: partial
                .pattern
                .as_ref()
                .and_then(|r| serde_json::to_string(r).ok()),
            local_ai_result: partial
                .local_ai
                .as_ref()
                .and_then(|r| serde_json::to_string(r).ok()),
            cloud_ai_result: partial
                .cloud
                .as_ref()
                .and_then(|r| serde_json::to_string(r).ok()),
            final_reference,
            confidence: decision.confidence as f64,
            decision: format!("{:?}", decision.action),
            operator_action: None,
            correct_reference: None,
            processing_time_ms: processing_ms as i64,
            timestamp: chrono_now(),
        };

        let _ = self.detection_repo.create(event).await;
    }
}

// ─── free helpers ─────────────────────────────────────────────────────────────

fn layer_result_to_event_ref(
    decision: &ArbitrationDecision,
) -> Option<EventBibleReference> {
    let layer = decision.reference.as_ref()?;
    let book = layer.book.as_ref()?.clone();
    let chapter = layer.chapter?;
    let r = EventBibleReference::new(book, chapter);
    Some(match layer.verse {
        Some(v) => r.with_verse(v),
        None => r,
    })
}

fn validate_reference(bible: &KjvBible, r: &EventBibleReference) -> ValidationOutcome {
    let bible_ref = companion_bible::BibleReference {
        book: r.book.clone(),
        chapter: r.chapter,
        verse: r.verse,
        verse_end: r.verse_end,
    };
    let validator = BibleValidator::new(bible);
    match validator.validate(&bible_ref) {
        ValidationResult::Valid(_) => ValidationOutcome::Valid,
        other => ValidationOutcome::Invalid { reason: other.to_string() },
    }
}

fn build_event(
    event_ref: &Option<EventBibleReference>,
    validation: &ValidationOutcome,
    source_text: &str,
) -> AppEvent {
    match (event_ref, validation.is_valid()) {
        (Some(r), true) => AppEvent::ScriptureReferenceDetected {
            references: vec![r.clone()],
            source_text: source_text.to_string(),
        },
        _ => AppEvent::NoReferenceFound { source_text: source_text.to_string() },
    }
}

fn chrono_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Minimal ISO-8601 without chrono dependency
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3600) % 24;
    let days = secs / 86400;
    // Approximate date from epoch (good enough for log timestamps)
    let year = 1970 + days / 365;
    format!("{year}-01-01T{h:02}:{m:02}:{s:02}Z")
}
