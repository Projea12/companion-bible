//! Shared types for the confidence arbitration layer.

// ─── LayerResult ─────────────────────────────────────────────────────────────

/// Normalised output from any detection layer.
///
/// Each layer (pattern engine, local AI, cloud AI) converts its own result
/// type into this common form before handing it to the arbitrator.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LayerResult {
    pub book: Option<String>,
    pub chapter: Option<u8>,
    pub verse: Option<u8>,
    /// Raw confidence score in [0.0, 1.0].
    pub confidence: f32,
}

impl LayerResult {
    pub fn new(
        book: impl Into<String>,
        chapter: u8,
        verse: u8,
        confidence: f32,
    ) -> Self {
        Self {
            book: Some(book.into()),
            chapter: Some(chapter),
            verse: Some(verse),
            confidence: confidence.clamp(0.0, 1.0),
        }
    }

    pub fn unresolved(confidence: f32) -> Self {
        Self {
            book: None,
            chapter: None,
            verse: None,
            confidence: confidence.clamp(0.0, 1.0),
        }
    }

    /// `true` when this result contains a fully-specified reference.
    pub fn has_reference(&self) -> bool {
        self.book.is_some()
    }

    /// `true` when both results refer to the same book, chapter and verse.
    pub fn matches(&self, other: &LayerResult) -> bool {
        self.book == other.book && self.chapter == other.chapter && self.verse == other.verse
    }
}

// ─── PartialResults ───────────────────────────────────────────────────────────

/// Accumulator for results as each detection layer completes.
///
/// `None` in a layer field means that layer hasn't responded yet **or** it
/// returned no usable result.  Use `*_pending` flags to distinguish.
#[derive(Debug, Clone, Default)]
pub struct PartialResults {
    /// Fast regex-based pattern engine (Layer 1).  Always the first to arrive.
    pub pattern: Option<LayerResult>,

    /// Local Phi-3 inference (Layer 2).
    pub local_ai: Option<LayerResult>,

    /// Cloud Claude inference (Layer 3).
    pub cloud: Option<LayerResult>,

    /// `true` while local AI inference is still running.
    pub local_ai_pending: bool,

    /// `true` while the cloud request is still in flight.
    pub cloud_pending: bool,

    /// Milliseconds since the pipeline started for this segment.
    pub elapsed_ms: u64,
}

impl PartialResults {
    /// Convenience: only the pattern engine has responded.
    pub fn pattern_only(pattern: LayerResult, elapsed_ms: u64) -> Self {
        Self {
            pattern: Some(pattern),
            local_ai_pending: true,
            cloud_pending: true,
            elapsed_ms,
            ..Default::default()
        }
    }

    /// Number of layers that have completed (pending = false, regardless of result).
    pub fn layers_responded(&self) -> usize {
        let mut n = 0;
        if self.pattern.is_some() { n += 1; }
        if !self.local_ai_pending { n += 1; }
        if !self.cloud_pending    { n += 1; }
        n
    }

    /// Non-None results across all layers.
    pub fn available_results(&self) -> Vec<&LayerResult> {
        [self.pattern.as_ref(), self.local_ai.as_ref(), self.cloud.as_ref()]
            .into_iter()
            .flatten()
            .collect()
    }
}

// ─── DisplayAction ────────────────────────────────────────────────────────────

/// What the display layer should do with an arbitration decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayAction {
    /// Confidence is high enough to display automatically.
    AutoDisplay,
    /// Moderate confidence — display but show an amber warning indicator.
    DisplayWithAmberWarning,
    /// Confidence is too low — hold the reference for operator review.
    HoldForOperator,
}

// ─── ArbitrationDecision ──────────────────────────────────────────────────────

/// Complete output of one arbitration pass.
#[derive(Debug, Clone)]
pub struct ArbitrationDecision {
    /// The reference selected by the arbitrator, or `None` when no layer
    /// produced a usable result.
    pub reference: Option<LayerResult>,

    /// Final blended confidence in [0.0, 1.0].
    pub confidence: f32,

    /// Recommended display action based on `confidence`.
    pub action: DisplayAction,

    /// `true` when every available layer agreed on the same reference.
    pub all_agree: bool,

    /// Whether the arbitrator recommends waiting for the cloud layer.
    pub should_wait_for_cloud: bool,
}
