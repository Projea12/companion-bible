//! Shared output types for the context enrichment pipeline.

use companion_detection::PatternResult;
use companion_events::BibleReference;
use companion_transcription::TranscriptionSegment;

// ─── ResolutionSource ─────────────────────────────────────────────────────────

/// Describes how an ambiguous scripture reference was resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolutionSource {
    /// Every part of the reference (book, chapter, verse) was explicit in the
    /// matched text — no context was needed.
    Explicit,
    /// The book was inferred from `active_book`; chapter and/or verse were in
    /// the text.
    BookInferred,
    /// Both book and chapter were inferred from `active_book` / `active_chapter`.
    BothInferred,
    /// Insufficient context to complete the reference (e.g. verse-only match
    /// with no active book/chapter yet established).
    Unresolved,
}

// ─── Detection ────────────────────────────────────────────────────────────────

/// A single scripture-reference candidate after context-aware resolution.
#[derive(Debug, Clone, PartialEq)]
pub struct Detection {
    /// The raw output from the pattern engine (byte offsets, confidence, etc.).
    pub raw_match: PatternResult,

    /// Fully resolved [`BibleReference`], or `None` when context was
    /// insufficient (see [`resolution_source`][Detection::resolution_source]).
    pub resolved: Option<BibleReference>,

    /// Explains how `resolved` was produced.
    pub resolution_source: ResolutionSource,
}

// ─── EnrichedSegment ─────────────────────────────────────────────────────────

/// A transcription segment after the full enrichment pipeline has run.
#[derive(Debug, Clone, PartialEq)]
pub struct EnrichedSegment {
    /// The original segment as received from the transcription channel.
    pub segment: TranscriptionSegment,

    /// Segment text after Nigerian-English phonetic corrections.
    pub corrected_text: String,

    /// Segment text after number-word normalisation (`"chapter three"` →
    /// `"chapter 3"`).  This is the text the pattern engine ran against.
    pub normalized_text: String,

    /// Every scripture reference detected in this segment, in the order they
    /// appear in `normalized_text`.
    pub detections: Vec<Detection>,

    /// [`SermonContext::context_confidence`] at the moment this segment was
    /// processed — snapshot of overall context quality.
    pub context_confidence: f32,
}

// ─── SubPointRef ─────────────────────────────────────────────────────────────

/// A lightweight sermon sub-point marker, independent of database types.
///
/// Set via [`SermonContext::set_sub_point`][crate::SermonContext::set_sub_point]
/// when the operator (or an automatic detector) advances to a new sermon
/// division.  The `id` field matches `SubPoint.id` in the database so the
/// caller can look up the full record when needed.
#[derive(Debug, Clone, PartialEq)]
pub struct SubPointRef {
    /// Matches `SubPoint.id` in the database.
    pub id: String,
    /// Display title shown in the operator UI.
    pub title: String,
    /// Zero-indexed position within the sermon.
    pub order_index: u32,
}
