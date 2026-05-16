//! Stateful context accumulator for an active sermon session.

use std::collections::VecDeque;

use companion_detection::{MatchCompleteness, NumberNormalizer, PatternEngine, PatternResult};
use companion_events::BibleReference;
use companion_transcription::{correct_text, TranscriptionSegment};

use crate::rolling_transcript::RollingTranscript;
use crate::types::{Detection, EnrichedSegment, ResolutionSource, SubPointRef};

// ─── Constants ────────────────────────────────────────────────────────────────

/// Maximum number of [`TranscriptionSegment`]s kept in the recent-segment window.
const MAX_RECENT_SEGMENTS: usize = 20;

/// Multiplied by `raw_match.confidence` to compute the confidence boost
/// applied for each detected scripture reference.
const CONFIDENCE_BOOST_FACTOR: f32 = 0.30;

/// Subtracted from `context_confidence` for each segment that contains no
/// detectable scripture reference.
const CONFIDENCE_DECAY_PER_EMPTY_SEGMENT: f32 = 0.02;

/// Minimum pattern confidence required for a detection to shift `active_book`
/// or `active_chapter`.  Patterns below this threshold can still resolve
/// references using the existing context but cannot overwrite it.
const CONTEXT_UPDATE_THRESHOLD: f32 = 0.70;

// ─── SermonContext ─────────────────────────────────────────────────────────────

/// Stateful context accumulator for a live sermon session.
///
/// Feed each [`TranscriptionSegment`] through [`enrich`][SermonContext::enrich].
/// The engine:
///
/// 1. Applies Nigerian-English phonetic corrections.
/// 2. Converts spoken number words to digits.
/// 3. Runs all five scripture-reference regex patterns.
/// 4. Resolves ambiguous patterns (verse-only, chapter+verse) using accumulated
///    context (`active_book`, `active_chapter`).
/// 5. Updates internal state and returns a fully annotated [`EnrichedSegment`].
pub struct SermonContext {
    // ── Public state ─────────────────────────────────────────────────────────
    /// Canonical name of the book currently being preached from (e.g.
    /// `"Romans"`), or `None` if no book has been detected yet.
    ///
    /// Updated whenever a pattern with confidence ≥ 0.70 and an explicit book
    /// is detected.
    pub active_book: Option<String>,

    /// Chapter currently being preached from, or `None`.
    ///
    /// Updated together with `active_book` — only shifts when the incoming
    /// pattern carries an explicit book name (prevents `chapter N verse N`
    /// patterns from phantom-shifting the chapter context).
    pub active_chapter: Option<u8>,

    /// Every verse reference detected during this session, paired with the
    /// audio timestamp (ms from the start of the stream) at which it appeared.
    pub mentioned_verses: Vec<(BibleReference, u64)>,

    /// Sliding window of the most recent [`MAX_RECENT_SEGMENTS`] segments.
    pub recent_segments: VecDeque<TranscriptionSegment>,

    /// The sermon's opening / anchor scripture, set by the caller.
    pub anchor_scripture: Option<BibleReference>,

    /// The current sermon sub-point, updated by the operator.
    pub current_sub_point: Option<SubPointRef>,

    /// Bounded sliding window of recent corrected transcript text.
    pub rolling_transcript: RollingTranscript,

    /// Overall confidence in the current context state [0.0 – 1.0].
    ///
    /// Rises when high-confidence patterns are detected; decays by
    /// [`CONFIDENCE_DECAY_PER_EMPTY_SEGMENT`] for each segment with no
    /// detectable reference.
    pub context_confidence: f32,

    // ── Private engine components ────────────────────────────────────────────
    engine: PatternEngine,
    normalizer: NumberNormalizer,
    max_recent_segments: usize,
}

impl SermonContext {
    // ── Constructors ──────────────────────────────────────────────────────────

    /// Create a fresh context with no prior state.
    pub fn new() -> Self {
        Self {
            active_book: None,
            active_chapter: None,
            mentioned_verses: Vec::new(),
            recent_segments: VecDeque::new(),
            anchor_scripture: None,
            current_sub_point: None,
            rolling_transcript: RollingTranscript::new(),
            context_confidence: 0.0,
            engine: PatternEngine::new(),
            normalizer: NumberNormalizer,
            max_recent_segments: MAX_RECENT_SEGMENTS,
        }
    }

    /// Create a context pre-loaded with a known anchor scripture.
    ///
    /// Also primes `active_book` and `active_chapter` so that verse-only
    /// detections in the sermon opening can be resolved immediately.
    pub fn with_anchor(anchor: BibleReference) -> Self {
        let mut ctx = Self::new();
        ctx.active_book = Some(anchor.book.clone());
        ctx.active_chapter = Some(anchor.chapter);
        ctx.anchor_scripture = Some(anchor);
        ctx
    }

    // ── Operator controls ─────────────────────────────────────────────────────

    /// Set or replace the anchor scripture.
    ///
    /// Also updates `active_book` and `active_chapter`.
    pub fn set_anchor(&mut self, anchor: BibleReference) {
        self.active_book = Some(anchor.book.clone());
        self.active_chapter = Some(anchor.chapter);
        self.anchor_scripture = Some(anchor);
    }

    /// Advance to a new sermon sub-point.
    pub fn set_sub_point(&mut self, sub_point: SubPointRef) {
        self.current_sub_point = Some(sub_point);
    }

    // ── Core pipeline ─────────────────────────────────────────────────────────

    /// Process a transcription segment through the full enrichment pipeline.
    ///
    /// Steps:
    /// 1. Nigerian-English phonetic correction (`"Revelations"` → `"Revelation"`).
    /// 2. Number-word normalisation (`"chapter three"` → `"chapter 3"`).
    /// 3. Pattern engine — all five regex patterns, highest confidence first.
    /// 4. Context-aware resolution — fills in missing book/chapter from state.
    /// 5. State update — shifts `active_book` / `active_chapter` for
    ///    high-confidence matches; tracks resolved verses; adjusts
    ///    `context_confidence`.
    /// 6. Sliding window updates — `rolling_transcript` and `recent_segments`.
    ///
    /// Returns a fully annotated [`EnrichedSegment`].
    pub fn enrich(&mut self, segment: TranscriptionSegment) -> EnrichedSegment {
        // 1. Correct Nigerian-English transcription errors.
        let corrected = correct_text(&segment.text);

        // 2. Normalise spoken number words to digits.
        let normalized = self.normalizer.normalize(&corrected);

        // 3. Detect scripture-reference patterns in the normalised text.
        let raw_patterns = self.engine.find_all(&normalized);

        // 4. Resolve each pattern against accumulated context.
        let detections: Vec<Detection> = raw_patterns
            .into_iter()
            .map(|p| self.resolve(p))
            .collect();

        // 5. Update context state from the resolved detections.
        self.integrate_detections(&detections, segment.audio_end_ms);

        // 6. Update sliding windows.
        self.rolling_transcript.push(&corrected);
        self.recent_segments.push_back(segment.clone());
        if self.recent_segments.len() > self.max_recent_segments {
            self.recent_segments.pop_front();
        }

        EnrichedSegment {
            segment,
            corrected_text: corrected,
            normalized_text: normalized,
            detections,
            context_confidence: self.context_confidence,
        }
    }

    // ── Private — resolution ──────────────────────────────────────────────────

    /// Resolve a single raw [`PatternResult`] against the current context.
    fn resolve(&self, p: PatternResult) -> Detection {
        use MatchCompleteness::*;

        let (resolved, resolution_source) = match p.completeness {
            // All parts are present in the matched text.
            FullCanonical | BookChapterVerse => {
                let r = build_reference(p.book.as_deref(), p.chapter, p.verse);
                (r, ResolutionSource::Explicit)
            }

            // Book + chapter, no verse — fully explicit, just no verse.
            BookChapter => {
                let r = build_reference(p.book.as_deref(), p.chapter, None);
                (r, ResolutionSource::Explicit)
            }

            // Chapter + verse present; need book from context.
            ChapterVerse => match &self.active_book {
                Some(book) => {
                    let r = build_reference(Some(book.as_str()), p.chapter, p.verse);
                    (r, ResolutionSource::BookInferred)
                }
                None => (None, ResolutionSource::Unresolved),
            },

            // Verse only; need both book and chapter from context.
            VerseOnly => match (&self.active_book, self.active_chapter) {
                (Some(book), Some(ch)) => match p.verse {
                    Some(v) => {
                        let r = BibleReference::new(book.clone(), ch).with_verse(v);
                        (Some(r), ResolutionSource::BothInferred)
                    }
                    None => (None, ResolutionSource::Unresolved),
                },
                _ => (None, ResolutionSource::Unresolved),
            },
        };

        Detection { raw_match: p, resolved, resolution_source }
    }

    // ── Private — state update ────────────────────────────────────────────────

    /// Integrate a batch of detections into the context state.
    fn integrate_detections(&mut self, detections: &[Detection], audio_ms: u64) {
        if detections.is_empty() {
            self.context_confidence =
                (self.context_confidence - CONFIDENCE_DECAY_PER_EMPTY_SEGMENT).max(0.0);
            return;
        }

        for d in detections {
            // Only high-confidence, book-bearing patterns may shift active context.
            if d.raw_match.confidence >= CONTEXT_UPDATE_THRESHOLD
                && d.raw_match.book.is_some()
            {
                self.active_book = d.raw_match.book.clone();
                if let Some(ch) = d.raw_match.chapter {
                    self.active_chapter = Some(ch);
                }
            }

            // Track every resolved verse reference.
            if let Some(r) = &d.resolved {
                if r.verse.is_some() {
                    self.mentioned_verses.push((r.clone(), audio_ms));
                }
            }

            // Boost context confidence.
            let boost = d.raw_match.confidence * CONFIDENCE_BOOST_FACTOR;
            self.context_confidence = (self.context_confidence + boost).min(1.0);
        }
    }
}

impl Default for SermonContext {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Build a [`BibleReference`] from optional parts.
///
/// Returns `None` only when `book` or `chapter` is absent — both are required
/// to form a valid reference.
fn build_reference(
    book: Option<&str>,
    chapter: Option<u8>,
    verse: Option<u8>,
) -> Option<BibleReference> {
    match (book, chapter) {
        (Some(b), Some(ch)) => {
            let r = BibleReference::new(b, ch);
            Some(match verse {
                Some(v) => r.with_verse(v),
                None => r,
            })
        }
        _ => None,
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Test helpers ──────────────────────────────────────────────────────────

    fn seg(text: &str) -> TranscriptionSegment {
        TranscriptionSegment {
            text: text.to_string(),
            audio_start_ms: 0,
            audio_end_ms: 5_000,
            whisper_confidence: 0.9,
            is_duplicate: false,
            context_window: String::new(),
        }
    }

    fn seg_at(text: &str, audio_end_ms: u64) -> TranscriptionSegment {
        TranscriptionSegment {
            text: text.to_string(),
            audio_start_ms: 0,
            audio_end_ms,
            whisper_confidence: 0.9,
            is_duplicate: false,
            context_window: String::new(),
        }
    }

    fn run(ctx: &mut SermonContext, text: &str) -> EnrichedSegment {
        ctx.enrich(seg(text))
    }

    // ── Constructor ───────────────────────────────────────────────────────────

    #[test]
    fn new_starts_with_empty_state() {
        let ctx = SermonContext::new();
        assert!(ctx.active_book.is_none());
        assert!(ctx.active_chapter.is_none());
        assert!(ctx.mentioned_verses.is_empty());
        assert!(ctx.recent_segments.is_empty());
        assert!(ctx.anchor_scripture.is_none());
        assert!(ctx.current_sub_point.is_none());
        assert!(ctx.rolling_transcript.is_empty());
        assert_eq!(ctx.context_confidence, 0.0);
    }

    #[test]
    fn with_anchor_primes_book_chapter_and_anchor_field() {
        let anchor = BibleReference::new("Romans", 8u8).with_verse(1);
        let ctx = SermonContext::with_anchor(anchor.clone());
        assert_eq!(ctx.active_book.as_deref(), Some("Romans"));
        assert_eq!(ctx.active_chapter, Some(8));
        assert_eq!(ctx.anchor_scripture, Some(anchor));
    }

    // ── enrich basics ─────────────────────────────────────────────────────────

    #[test]
    fn enrich_returns_original_segment_unchanged() {
        let mut ctx = SermonContext::new();
        let original = seg("John 3:16");
        let e = ctx.enrich(original.clone());
        assert_eq!(e.segment, original);
    }

    #[test]
    fn enrich_no_reference_returns_empty_detections() {
        let mut ctx = SermonContext::new();
        let e = run(&mut ctx, "Good morning, everyone.");
        assert!(e.detections.is_empty());
    }

    #[test]
    fn enrich_provides_corrected_and_normalized_text_fields() {
        let mut ctx = SermonContext::new();
        // "Revelations" corrected → "Revelation"; "three" normalised → "3"
        let e = run(&mut ctx, "Revelations chapter three verse sixteen");
        assert!(
            e.corrected_text.contains("Revelation"),
            "corrected_text should fix Revelations→Revelation; got: {}",
            e.corrected_text
        );
        assert!(
            e.normalized_text.contains("3"),
            "normalized_text should have digits; got: {}",
            e.normalized_text
        );
    }

    // ── Pattern detection ─────────────────────────────────────────────────────

    #[test]
    fn enrich_detects_full_canonical_colon_form() {
        let mut ctx = SermonContext::new();
        let e = run(&mut ctx, "John 3:16");
        assert_eq!(e.detections.len(), 1);
        let d = &e.detections[0];
        assert_eq!(d.raw_match.book.as_deref(), Some("John"));
        assert_eq!(d.raw_match.chapter, Some(3));
        assert_eq!(d.raw_match.verse, Some(16));
        assert_eq!(d.resolution_source, ResolutionSource::Explicit);
        assert_eq!(d.raw_match.confidence, 1.0);
    }

    #[test]
    fn enrich_detects_spoken_form() {
        let mut ctx = SermonContext::new();
        let e = run(&mut ctx, "John chapter 3 verse 16");
        assert_eq!(e.detections.len(), 1);
        assert_eq!(e.detections[0].resolution_source, ResolutionSource::Explicit);
        assert_eq!(e.detections[0].raw_match.confidence, 0.95);
    }

    #[test]
    fn enrich_detects_book_chapter_only() {
        let mut ctx = SermonContext::new();
        let e = run(&mut ctx, "turn to Romans 8");
        assert_eq!(e.detections.len(), 1);
        let d = &e.detections[0];
        assert_eq!(d.raw_match.book.as_deref(), Some("Romans"));
        assert_eq!(d.raw_match.chapter, Some(8));
        assert_eq!(d.raw_match.verse, None);
        assert_eq!(d.resolution_source, ResolutionSource::Explicit);
    }

    #[test]
    fn enrich_detects_chapter_verse_no_book() {
        let mut ctx = SermonContext::new();
        let e = run(&mut ctx, "chapter 3 verse 16");
        assert_eq!(e.detections.len(), 1);
        assert_eq!(e.detections[0].raw_match.chapter, Some(3));
        assert_eq!(e.detections[0].raw_match.verse, Some(16));
    }

    #[test]
    fn enrich_detects_verse_only() {
        let mut ctx = SermonContext::new();
        let e = run(&mut ctx, "please read verse 16");
        assert_eq!(e.detections.len(), 1);
        assert_eq!(e.detections[0].raw_match.verse, Some(16));
    }

    // ── Context state updates ─────────────────────────────────────────────────

    #[test]
    fn enrich_updates_active_book_and_chapter_from_full_ref() {
        let mut ctx = SermonContext::new();
        run(&mut ctx, "Romans 8:1");
        assert_eq!(ctx.active_book.as_deref(), Some("Romans"));
        assert_eq!(ctx.active_chapter, Some(8));
    }

    #[test]
    fn enrich_updates_active_book_and_chapter_from_book_chapter_only() {
        let mut ctx = SermonContext::new();
        run(&mut ctx, "turn to Psalms 23");
        assert_eq!(ctx.active_book.as_deref(), Some("Psalms"));
        assert_eq!(ctx.active_chapter, Some(23));
    }

    #[test]
    fn enrich_replaces_active_context_when_new_book_detected() {
        let mut ctx = SermonContext::new();
        run(&mut ctx, "Romans 8:1");
        run(&mut ctx, "Galatians 5:1");
        assert_eq!(ctx.active_book.as_deref(), Some("Galatians"));
        assert_eq!(ctx.active_chapter, Some(5));
    }

    #[test]
    fn enrich_chapter_verse_pattern_does_not_shift_active_chapter() {
        // ChapterVerse confidence is 0.60, below the CONTEXT_UPDATE_THRESHOLD,
        // AND it carries no book — so active_chapter must not change.
        let mut ctx = SermonContext::new();
        run(&mut ctx, "Romans 8:1");
        assert_eq!(ctx.active_chapter, Some(8));
        run(&mut ctx, "chapter 9 verse 1");
        assert_eq!(
            ctx.active_chapter,
            Some(8),
            "low-confidence bookless pattern should not shift active_chapter"
        );
    }

    #[test]
    fn enrich_verse_only_pattern_does_not_shift_active_book_or_chapter() {
        let mut ctx = SermonContext::new();
        run(&mut ctx, "Romans 8:1");
        run(&mut ctx, "verse 28");
        // VerseOnly has no book — state must remain unchanged.
        assert_eq!(ctx.active_book.as_deref(), Some("Romans"));
        assert_eq!(ctx.active_chapter, Some(8));
    }

    // ── Context-aware resolution ──────────────────────────────────────────────

    #[test]
    fn enrich_resolves_verse_only_using_active_book_and_chapter() {
        let mut ctx = SermonContext::new();
        run(&mut ctx, "Romans 8:1"); // establish context
        let e = run(&mut ctx, "also see verse 28");
        assert_eq!(e.detections.len(), 1);
        let d = &e.detections[0];
        assert_eq!(d.resolution_source, ResolutionSource::BothInferred);
        let r = d.resolved.as_ref().expect("should resolve to Romans 8:28");
        assert_eq!(r.book, "Romans");
        assert_eq!(r.chapter, 8);
        assert_eq!(r.verse, Some(28));
    }

    #[test]
    fn enrich_resolves_chapter_verse_using_active_book() {
        let mut ctx = SermonContext::new();
        run(&mut ctx, "Romans 8:1"); // establish book
        let e = run(&mut ctx, "chapter 9 verse 1");
        let d = &e.detections[0];
        assert_eq!(d.resolution_source, ResolutionSource::BookInferred);
        let r = d.resolved.as_ref().expect("should resolve to Romans 9:1");
        assert_eq!(r.book, "Romans");
        assert_eq!(r.chapter, 9);
        assert_eq!(r.verse, Some(1));
    }

    #[test]
    fn enrich_verse_only_unresolved_without_context() {
        let mut ctx = SermonContext::new();
        let e = run(&mut ctx, "verse 16");
        let d = &e.detections[0];
        assert_eq!(d.resolution_source, ResolutionSource::Unresolved);
        assert!(d.resolved.is_none());
    }

    #[test]
    fn enrich_chapter_verse_unresolved_without_book_context() {
        let mut ctx = SermonContext::new();
        let e = run(&mut ctx, "chapter 3 verse 16");
        let d = &e.detections[0];
        assert_eq!(d.resolution_source, ResolutionSource::Unresolved);
        assert!(d.resolved.is_none());
    }

    #[test]
    fn enrich_verse_only_unresolved_when_book_known_but_chapter_missing() {
        // Book is primed via anchor but chapter is explicitly cleared.
        let anchor = BibleReference::new("John", 1u8); // chapter=1 primed
        let mut ctx = SermonContext::with_anchor(anchor);
        ctx.active_chapter = None; // simulate no chapter yet
        let e = run(&mut ctx, "verse 16");
        assert_eq!(e.detections[0].resolution_source, ResolutionSource::Unresolved);
    }

    // ── Normalisation ─────────────────────────────────────────────────────────

    #[test]
    fn enrich_normalizes_number_words_to_digits() {
        let mut ctx = SermonContext::new();
        let e = run(&mut ctx, "John chapter three verse sixteen");
        assert_eq!(e.normalized_text, "John chapter 3 verse 16");
        assert_eq!(e.detections.len(), 1);
        assert_eq!(e.detections[0].raw_match.chapter, Some(3));
        assert_eq!(e.detections[0].raw_match.verse, Some(16));
    }

    #[test]
    fn enrich_normalizes_ordinal_book_prefix() {
        let mut ctx = SermonContext::new();
        // "first" → "1", "chapter thirteen" → "chapter 13", "verse thirteen" → "verse 13"
        let e = run(&mut ctx, "first Corinthians chapter thirteen verse thirteen");
        assert!(
            !e.detections.is_empty(),
            "expected detection after normalisation; normalized_text='{}'",
            e.normalized_text
        );
        assert_eq!(
            e.detections[0].raw_match.book.as_deref(),
            Some("1 Corinthians")
        );
        assert_eq!(e.detections[0].raw_match.chapter, Some(13));
        assert_eq!(e.detections[0].raw_match.verse, Some(13));
    }

    #[test]
    fn enrich_normalizes_psalm_119() {
        let mut ctx = SermonContext::new();
        let e = run(&mut ctx, "Psalms chapter one hundred and nineteen verse one");
        assert!(!e.detections.is_empty(), "Psalm 119 not detected");
        assert_eq!(e.detections[0].raw_match.chapter, Some(119));
    }

    // ── Nigerian-English correction ───────────────────────────────────────────

    #[test]
    fn enrich_corrects_revelations_to_revelation() {
        let mut ctx = SermonContext::new();
        let e = run(&mut ctx, "Revelations 22:21");
        assert!(
            !e.detections.is_empty(),
            "detection expected after correction; corrected='{}'",
            e.corrected_text
        );
        assert_eq!(
            e.detections[0].raw_match.book.as_deref(),
            Some("Revelation")
        );
    }

    #[test]
    fn enrich_corrects_and_normalizes_in_sequence() {
        // "Revelations" corrected first, then number words normalised.
        let mut ctx = SermonContext::new();
        let e = run(&mut ctx, "Revelations chapter twenty two verse twenty one");
        assert!(!e.detections.is_empty(), "detection expected");
        assert_eq!(
            e.detections[0].raw_match.book.as_deref(),
            Some("Revelation")
        );
        assert_eq!(e.detections[0].raw_match.chapter, Some(22));
        assert_eq!(e.detections[0].raw_match.verse, Some(21));
    }

    // ── Mentioned verses ──────────────────────────────────────────────────────

    #[test]
    fn enrich_tracks_mentioned_verses_with_timestamps() {
        let mut ctx = SermonContext::new();
        ctx.enrich(seg_at("John 3:16", 10_000));
        ctx.enrich(seg_at("Romans 8:1", 25_000));
        assert_eq!(ctx.mentioned_verses.len(), 2);
        assert_eq!(ctx.mentioned_verses[0].0.book, "John");
        assert_eq!(ctx.mentioned_verses[0].1, 10_000);
        assert_eq!(ctx.mentioned_verses[1].0.book, "Romans");
        assert_eq!(ctx.mentioned_verses[1].1, 25_000);
    }

    #[test]
    fn enrich_does_not_track_chapter_only_refs_in_mentioned_verses() {
        let mut ctx = SermonContext::new();
        run(&mut ctx, "Romans 8"); // BookChapter — no verse
        assert!(ctx.mentioned_verses.is_empty());
    }

    #[test]
    fn enrich_tracks_verse_resolved_from_context() {
        let mut ctx = SermonContext::new();
        run(&mut ctx, "Romans 8:1"); // set context
        run(&mut ctx, "verse 28"); // resolved to Romans 8:28
        assert_eq!(ctx.mentioned_verses.len(), 2);
        let resolved = &ctx.mentioned_verses[1].0;
        assert_eq!(resolved.book, "Romans");
        assert_eq!(resolved.chapter, 8);
        assert_eq!(resolved.verse, Some(28));
    }

    // ── Confidence ────────────────────────────────────────────────────────────

    #[test]
    fn context_confidence_starts_at_zero() {
        assert_eq!(SermonContext::new().context_confidence, 0.0);
    }

    #[test]
    fn context_confidence_increases_after_detection() {
        let mut ctx = SermonContext::new();
        run(&mut ctx, "John 3:16");
        assert!(
            ctx.context_confidence > 0.0,
            "confidence should rise after detection"
        );
    }

    #[test]
    fn context_confidence_decays_on_segments_with_no_detection() {
        let mut ctx = SermonContext::new();
        run(&mut ctx, "John 3:16");
        let after_detection = ctx.context_confidence;
        for _ in 0..10 {
            run(&mut ctx, "And so we see the truth of this teaching.");
        }
        assert!(
            ctx.context_confidence < after_detection,
            "confidence should decay after {} empty segments",
            10
        );
    }

    #[test]
    fn context_confidence_never_exceeds_1_0() {
        let mut ctx = SermonContext::new();
        for _ in 0..30 {
            run(&mut ctx, "John 3:16");
        }
        assert!(ctx.context_confidence <= 1.0);
    }

    #[test]
    fn context_confidence_never_goes_below_0() {
        let mut ctx = SermonContext::new();
        for _ in 0..200 {
            run(&mut ctx, "and so the lesson continues today");
        }
        assert!(ctx.context_confidence >= 0.0);
    }

    #[test]
    fn enriched_segment_carries_context_confidence_snapshot() {
        let mut ctx = SermonContext::new();
        let e = run(&mut ctx, "John 3:16");
        assert_eq!(e.context_confidence, ctx.context_confidence);
    }

    // ── Rolling transcript ────────────────────────────────────────────────────

    #[test]
    fn enrich_appends_corrected_text_to_rolling_transcript() {
        let mut ctx = SermonContext::new();
        run(&mut ctx, "Turn to John 3:16.");
        assert!(!ctx.rolling_transcript.is_empty());
        // Corrected text (after correction pass) should be in the transcript.
        assert!(ctx.rolling_transcript.text().contains("John"));
    }

    #[test]
    fn rolling_transcript_accumulates_across_segments() {
        let mut ctx = SermonContext::new();
        run(&mut ctx, "Good morning.");
        run(&mut ctx, "Romans 8:1.");
        let text = ctx.rolling_transcript.text();
        assert!(text.contains("Good morning."));
        assert!(text.contains("Romans"));
    }

    // ── Recent segments window ────────────────────────────────────────────────

    #[test]
    fn enrich_pushes_segments_to_recent_window() {
        let mut ctx = SermonContext::new();
        run(&mut ctx, "first");
        run(&mut ctx, "second");
        assert_eq!(ctx.recent_segments.len(), 2);
        assert_eq!(ctx.recent_segments[0].text, "first");
        assert_eq!(ctx.recent_segments[1].text, "second");
    }

    #[test]
    fn recent_segments_window_is_capped_at_max() {
        let mut ctx = SermonContext::new();
        for i in 0..=(MAX_RECENT_SEGMENTS + 5) {
            run(&mut ctx, &format!("segment {i}"));
        }
        assert_eq!(ctx.recent_segments.len(), MAX_RECENT_SEGMENTS);
    }

    #[test]
    fn recent_segments_evicts_oldest_when_capped() {
        let mut ctx = SermonContext::new();
        for i in 0..=(MAX_RECENT_SEGMENTS + 2) {
            run(&mut ctx, &format!("segment {i}"));
        }
        // The first few segments should be gone.
        assert_ne!(ctx.recent_segments[0].text, "segment 0");
    }

    // ── Multiple detections in one segment ───────────────────────────────────

    #[test]
    fn enrich_detects_multiple_references_in_one_segment() {
        let mut ctx = SermonContext::new();
        let e = run(&mut ctx, "from Genesis 1:1 to Revelation 22:21");
        assert_eq!(e.detections.len(), 2);
        assert_eq!(e.detections[0].raw_match.book.as_deref(), Some("Genesis"));
        assert_eq!(e.detections[1].raw_match.book.as_deref(), Some("Revelation"));
    }

    #[test]
    fn enrich_last_book_wins_when_multiple_in_one_segment() {
        // Both update context; the last one processed should be active.
        let mut ctx = SermonContext::new();
        run(&mut ctx, "Genesis 1:1 and Revelation 22:21");
        assert_eq!(ctx.active_book.as_deref(), Some("Revelation"));
    }

    // ── Resolved reference fields ─────────────────────────────────────────────

    #[test]
    fn resolved_reference_has_correct_book_chapter_verse() {
        let mut ctx = SermonContext::new();
        let e = run(&mut ctx, "1 Corinthians 13:13");
        let r = e.detections[0].resolved.as_ref().expect("should resolve");
        assert_eq!(r.book, "1 Corinthians");
        assert_eq!(r.chapter, 13);
        assert_eq!(r.verse, Some(13));
    }

    #[test]
    fn resolved_reference_for_book_chapter_only_has_no_verse() {
        let mut ctx = SermonContext::new();
        let e = run(&mut ctx, "Romans 8");
        let r = e.detections[0].resolved.as_ref().expect("should resolve");
        assert_eq!(r.book, "Romans");
        assert_eq!(r.chapter, 8);
        assert!(r.verse.is_none());
    }

    // ── Operator controls ─────────────────────────────────────────────────────

    #[test]
    fn set_anchor_updates_all_anchor_state() {
        let mut ctx = SermonContext::new();
        ctx.set_anchor(BibleReference::new("Ephesians", 1u8).with_verse(3));
        assert_eq!(ctx.active_book.as_deref(), Some("Ephesians"));
        assert_eq!(ctx.active_chapter, Some(1));
        assert!(ctx.anchor_scripture.is_some());
        assert_eq!(
            ctx.anchor_scripture.as_ref().unwrap().book,
            "Ephesians"
        );
    }

    #[test]
    fn set_anchor_replaces_previous_anchor() {
        let mut ctx = SermonContext::new();
        ctx.set_anchor(BibleReference::new("Romans", 8u8));
        ctx.set_anchor(BibleReference::new("John", 3u8));
        assert_eq!(ctx.active_book.as_deref(), Some("John"));
        assert_eq!(ctx.anchor_scripture.as_ref().unwrap().book, "John");
    }

    #[test]
    fn set_sub_point_updates_current_sub_point() {
        let mut ctx = SermonContext::new();
        ctx.set_sub_point(SubPointRef {
            id: "sp-1".to_string(),
            title: "The Grace of God".to_string(),
            order_index: 0,
        });
        let sp = ctx.current_sub_point.as_ref().expect("sub_point should be set");
        assert_eq!(sp.id, "sp-1");
        assert_eq!(sp.title, "The Grace of God");
        assert_eq!(sp.order_index, 0);
    }

    #[test]
    fn set_sub_point_replaces_previous_sub_point() {
        let mut ctx = SermonContext::new();
        ctx.set_sub_point(SubPointRef { id: "sp-1".into(), title: "Intro".into(), order_index: 0 });
        ctx.set_sub_point(SubPointRef { id: "sp-2".into(), title: "Body".into(), order_index: 1 });
        assert_eq!(ctx.current_sub_point.as_ref().unwrap().id, "sp-2");
    }
}
