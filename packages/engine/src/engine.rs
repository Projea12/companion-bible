//! `DetectionEngine` — orchestrates all three detection layers, arbitration,
//! validation, context tracking, event emission, and database logging.

use std::sync::Arc;
use std::time::Instant;

use companion_arbitrator::{ArbitrationDecision, ConfidenceArbitrator, PartialResults};
use companion_bible::{BibleValidator, KjvBible, ValidationResult};
use companion_calibration::{CalibrationError, ChurchCalibrator};
use companion_cloud_ai::{CloudAI, OpenAICloudAI};
use companion_context::SermonContext;
use companion_database::{
    CalibrationRepository, ChurchRepository, DetectionEvent, DetectionEventRepository,
    VerseRepository,
};
use companion_detection::detect_hymn_number;
use companion_events::{AppEvent, BibleReference as EventBibleReference};
use companion_transcription::TranscriptionSegment;

use crate::decision::{DetectionDecision, ValidationOutcome};
use crate::hymn_session::{HymnSession, HymnSessionEvent};
use crate::layers;
use crate::quotation;
use crate::worker::{collect_local_ai, LocalAiHandle};

// ─── DisplayMode ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Default)]
pub enum DisplayMode {
    #[default]
    Bible,
    Hymn,
}

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

    /// OpenAI API key — primary cloud detection layer.  `None` disables OpenAI.
    pub openai_api_key: Option<String>,

    /// Anthropic API key — fallback cloud layer.  `None` disables Anthropic.
    pub api_key: Option<String>,
}

// ─── DetectionEngine ─────────────────────────────────────────────────────────

/// Holds every component needed to process a `TranscriptionSegment` from raw
/// audio through to a validated `DetectionDecision`.
pub struct DetectionEngine {
    // ── detection layers ──────────────────────────────────────────────────────
    local_ai: Option<Arc<LocalAiHandle>>,
    /// OpenAI — primary cloud layer (fires first, 2 s budget).
    openai_cloud_ai: Option<Arc<OpenAICloudAI>>,
    /// Anthropic — fallback cloud layer.
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

    // ── hymn state ────────────────────────────────────────────────────────────
    pub display_mode: DisplayMode,
    hymn_session: Option<HymnSession>,
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

        let arbitrator = ConfidenceArbitrator {
            auto_display_threshold: calibrator.thresholds().auto_display,
            amber_threshold: calibrator.thresholds().show_with_warning,
            ..Default::default()
        };

        let local_ai = local_ai_handle.map(Arc::new);
        let openai_cloud_ai = config
            .openai_api_key
            .map(|k| Arc::new(OpenAICloudAI::new(k)));
        let cloud_ai = config.api_key.map(|k| Arc::new(CloudAI::new(k)));

        Ok(Self {
            local_ai,
            openai_cloud_ai,
            cloud_ai,
            context: SermonContext::new(),
            arbitrator,
            bible,
            calibrator,
            detection_repo,
            verse_repo: Some(verse_repo),
            sermon_id: config.sermon_id,
            event_tx: None,
            display_mode: DisplayMode::Bible,
            hymn_session: None,
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

    /// Switch the operator display between Bible verse mode and GHS hymn mode.
    pub fn set_display_mode(&mut self, mode: DisplayMode) {
        self.display_mode = mode;
    }

    /// Manually load a hymn by number — same as auto-detection but triggered
    /// by the operator typing a number.  Returns `false` if the number is not
    /// in the book (1–260).
    pub fn load_hymn(&mut self, number: u16) -> bool {
        if let Some(session) = HymnSession::load(number) {
            if let Some(HymnSessionEvent::Loaded {
                number: n,
                ref title,
                section_index,
                stanza_number,
                is_chorus,
                ref lines,
            }) = session.start_event()
            {
                self.emit_event(AppEvent::HymnDetected {
                    number: n,
                    title: title.clone(),
                });
                self.emit_event(AppEvent::HymnSectionAdvanced {
                    number: n,
                    section_index,
                    stanza_number,
                    is_chorus,
                    lines: lines.clone(),
                });
            }
            self.hymn_session = Some(session);
            self.display_mode = DisplayMode::Hymn;
            true
        } else {
            false
        }
    }

    /// Manually advance the active hymn to the next section (operator button).
    /// Returns `false` if no hymn session is active or it is already completed.
    pub fn advance_hymn(&mut self) -> bool {
        let session = match self.hymn_session.as_mut() {
            Some(s) => s,
            None => return false,
        };
        if let Some(event) = session.advance() {
            self.emit_event(hymn_session_to_app_event(event));
            true
        } else {
            false
        }
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

        // ── 1b. Hymn detection — runs regardless of display mode ──────────
        self.process_hymn_segment(&text);

        // ── 2. Pattern layer (sync, embedded in enrichment) ───────────────
        let segment_pattern = layers::pattern_layer(&enriched);
        let has_full_ref = segment_pattern
            .as_ref()
            .map(|r| r.book.is_some() && r.chapter.is_some() && r.verse.is_some())
            .unwrap_or(false);

        // Only scan the rolling transcript when the current segment contains an
        // explicit scripture-intent signal.  Without this gate, every segment
        // re-detects old references still present in the 60-second window,
        // causing the same verse to be displayed repeatedly.
        let has_intent = has_scripture_intent(&text);
        let pattern_result = if has_full_ref {
            segment_pattern
        } else if has_intent {
            let rolling =
                layers::pattern_layer_from_results(&self.context.find_in_rolling_transcript());
            rolling.or(segment_pattern)
        } else {
            segment_pattern
        };

        // ── 2c. Quotation layer — only when intent signal present ────────
        let pattern_result = if pattern_result
            .as_ref()
            .map(|r| r.verse.is_some())
            .unwrap_or(false)
        {
            pattern_result
        } else if has_intent {
            let quotation_result = self
                .quotation_layer(&transcript, active_book.as_deref(), active_chapter)
                .await;
            quotation_result.or(pattern_result)
        } else {
            pattern_result
        };

        // ── 3. Fire OpenAI immediately (primary layer, non-blocking) ─────
        // Only fire when there is a scripture-intent signal in the current
        // segment — prevents OpenAI from inferring references from context
        // alone on every unrelated utterance.
        let openai_handle = if has_intent {
            self.openai_cloud_ai.as_ref().map(|ai| {
                let ai = ai.clone();
                let text_c = text.clone();
                let transcript_c = transcript.clone();
                let book_c = active_book.clone();
                let anchor_c = anchor.clone();
                tokio::task::spawn_blocking(move || {
                    ai.detect(
                        &text_c,
                        book_c.as_deref(),
                        active_chapter,
                        &transcript_c,
                        anchor_c.as_deref(),
                    )
                })
            })
        } else {
            None
        };

        // ── 4. Submit to local AI worker (non-blocking) ───────────────────
        let local_rx = self.local_ai.as_ref().and_then(|h| {
            h.try_submit(
                text.clone(),
                active_book.clone(),
                active_chapter,
                transcript.clone(),
            )
        });

        // ── 5. Spawn Anthropic fallback cloud task ────────────────────────
        // Only used when OpenAI is not configured.
        let cloud_handle = if openai_handle.is_none() {
            self.cloud_ai.as_ref().map(|cloud| {
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
            })
        } else {
            None
        };

        // ── 6. Build partial results with pattern as fallback ─────────────
        let mut partial = PartialResults {
            pattern: pattern_result,
            local_ai_pending: local_rx.is_some(),
            cloud_pending: openai_handle.is_some() || cloud_handle.is_some(),
            elapsed_ms: t0.elapsed().as_millis() as u64,
            ..Default::default()
        };

        // ── 7. Collect local AI result ─────────────────────────────────────
        if let Some(rx) = local_rx {
            let elapsed = t0.elapsed().as_millis() as u64;
            let budget = LOCAL_AI_WAIT_MS.saturating_sub(elapsed).max(10);
            if let Some(r) = collect_local_ai(rx, budget) {
                partial.local_ai = layers::local_ai_layer(r);
            }
            partial.local_ai_pending = false;
            partial.elapsed_ms = t0.elapsed().as_millis() as u64;
        }

        // ── 8. Collect OpenAI result (primary — always wait within budget) ─
        if let Some(handle) = openai_handle {
            let elapsed = t0.elapsed().as_millis() as u64;
            let budget = CLOUD_WAIT_BUDGET_MS.saturating_sub(elapsed).max(10);
            let timeout = tokio::time::Duration::from_millis(budget);
            if let Ok(Ok(result)) = tokio::time::timeout(timeout, handle).await {
                partial.cloud = layers::cloud_ai_layer(result);
            }
            partial.cloud_pending = false;
        }

        // ── 9. Mid-point arbitration ──────────────────────────────────────
        let mid = self.arbitrator.arbitrate(&partial);

        // ── 10. Conditionally wait for Anthropic fallback ─────────────────
        if let Some(handle) = cloud_handle {
            if mid.should_wait_for_cloud {
                let elapsed = t0.elapsed().as_millis() as u64;
                let budget = CLOUD_WAIT_BUDGET_MS.saturating_sub(elapsed).max(10);
                let timeout = tokio::time::Duration::from_millis(budget);
                if let Ok(Ok(result)) = tokio::time::timeout(timeout, handle).await {
                    partial.cloud = layers::cloud_ai_layer(result);
                }
            } else {
                handle.abort();
            }
            partial.cloud_pending = false;
        }
        partial.elapsed_ms = t0.elapsed().as_millis() as u64;

        // ── 11. Final arbitration ─────────────────────────────────────────
        let final_decision = self.arbitrator.arbitrate(&partial);
        let processing_ms = t0.elapsed().as_millis() as u64;

        // ── 12. Resolve winning reference ─────────────────────────────────
        let event_ref = layer_result_to_event_ref(&final_decision);

        // ── 13. Validate ──────────────────────────────────────────────────
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
        self.log_event(
            &partial,
            &final_decision,
            &event_ref,
            &validation,
            &text,
            processing_ms,
        )
        .await;

        // ── 15. Build decision ────────────────────────────────────────────
        let reference = if validation.is_valid() {
            event_ref
        } else {
            None
        };

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

    fn process_hymn_segment(&mut self, text: &str) {
        // Check for a new hymn number in speech first.
        if let Some(number) = detect_hymn_number(text) {
            let already_at_start = self
                .hymn_session
                .as_ref()
                .map(|s| s.position == 0 && !s.completed)
                .unwrap_or(false);

            if !already_at_start {
                if let Some(session) = HymnSession::load(number) {
                    // Emit detection event (carries title).
                    if let Some(HymnSessionEvent::Loaded {
                        number: n,
                        ref title,
                        section_index,
                        stanza_number,
                        is_chorus,
                        ref lines,
                    }) = session.start_event()
                    {
                        self.emit_event(AppEvent::HymnDetected {
                            number: n,
                            title: title.clone(),
                        });
                        self.emit_event(AppEvent::HymnSectionAdvanced {
                            number: n,
                            section_index,
                            stanza_number,
                            is_chorus,
                            lines: lines.clone(),
                        });
                    }
                    self.hymn_session = Some(session);
                    self.display_mode = DisplayMode::Hymn;
                    return;
                }
            }
        }

        // Check last-line match against the active section.
        if let Some(session) = self.hymn_session.as_mut() {
            if let Some(event) = session.check_advance(text) {
                self.emit_event(hymn_session_to_app_event(event));
            }
        }
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

fn layer_result_to_event_ref(decision: &ArbitrationDecision) -> Option<EventBibleReference> {
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
        other => ValidationOutcome::Invalid {
            reason: other.to_string(),
        },
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
        _ => AppEvent::NoReferenceFound {
            source_text: source_text.to_string(),
        },
    }
}

/// Returns `true` when the segment text contains an explicit scripture-intent
/// signal — a Bible book name, or the words "verse", "chapter", "scripture",
/// "passage", or a standalone number that could be a chapter/verse reference.
///
/// Used to gate the rolling-transcript scan and cloud AI calls so that
/// ordinary speech never re-detects stale references from history.
fn has_scripture_intent(text: &str) -> bool {
    use companion_detection::build_book_alternation;
    // Fast keyword check first — avoids regex if obvious signals are absent.
    let lower = text.to_lowercase();
    if [
        "verse",
        "chapter",
        "scripture",
        "passage",
        "psalm",
        "proverb",
    ]
    .iter()
    .any(|kw| lower.contains(kw))
    {
        return true;
    }
    // Check for any digit in text — "3:16", "chapter 3", "verse 16" etc.
    if text.chars().any(|c| c.is_ascii_digit()) {
        return true;
    }
    // Check for a Bible book name.
    static BOOK_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = BOOK_RE.get_or_init(|| {
        regex::Regex::new(&format!("(?i)\\b(?:{})\\b", build_book_alternation())).expect("book_re")
    });
    re.is_match(text)
}

fn hymn_session_to_app_event(ev: HymnSessionEvent) -> AppEvent {
    match ev {
        HymnSessionEvent::Loaded {
            number,
            title: _,
            section_index,
            stanza_number,
            is_chorus,
            lines,
        } => AppEvent::HymnSectionAdvanced {
            number,
            section_index,
            stanza_number,
            is_chorus,
            lines,
        },
        HymnSessionEvent::Advanced {
            number,
            section_index,
            stanza_number,
            is_chorus,
            lines,
        } => AppEvent::HymnSectionAdvanced {
            number,
            section_index,
            stanza_number,
            is_chorus,
            lines,
        },
        HymnSessionEvent::Completed { number } => AppEvent::HymnCompleted { number },
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
