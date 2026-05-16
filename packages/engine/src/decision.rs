//! Output type returned by `DetectionEngine::process`.

use companion_arbitrator::DisplayAction;
use companion_events::BibleReference;

// ─── ValidationOutcome ───────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum ValidationOutcome {
    /// Reference exists in the KJV canon.
    Valid,
    /// Reference was produced but is not in the KJV canon.
    Invalid { reason: String },
    /// No layer produced any reference.
    NoReference,
}

impl ValidationOutcome {
    pub fn is_valid(&self) -> bool {
        matches!(self, Self::Valid)
    }
}

// ─── DetectionDecision ───────────────────────────────────────────────────────

/// Final output of one `DetectionEngine::process` call.
#[derive(Debug, Clone)]
pub struct DetectionDecision {
    /// Validated reference, or `None` when nothing was detected or the
    /// reference failed KJV validation.
    pub reference: Option<BibleReference>,

    /// Blended confidence in [0.0, 1.0].
    pub confidence: f32,

    /// Recommended display action based on confidence and calibrated thresholds.
    pub action: DisplayAction,

    /// Whether the reference passed KJV canon validation.
    pub validation: ValidationOutcome,

    /// `true` when every available layer agreed on the same reference.
    pub all_layers_agreed: bool,

    /// Wall-clock time from segment receipt to decision, in milliseconds.
    pub processing_ms: u64,
}
