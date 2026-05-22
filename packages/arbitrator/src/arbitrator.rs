//! Confidence arbitration — combines layer results into a display decision.

use crate::types::{ArbitrationDecision, DisplayAction, LayerResult, PartialResults};

// ─── Constants ────────────────────────────────────────────────────────────────

/// Layer weights (must sum to 1.0).
const PATTERN_WEIGHT: f32 = 0.40;
const LOCAL_AI_WEIGHT: f32 = 0.35;
const CLOUD_WEIGHT: f32 = 0.25;

/// Multiplier applied to weighted confidence when all available layers agree.
const CONSENSUS_BOOST: f32 = 1.10;

/// `decide_action` thresholds.
const AUTO_DISPLAY_THRESHOLD: f32 = 0.85;
const AMBER_THRESHOLD: f32 = 0.60;

/// `should_wait_for_cloud` confidence band — only worthwhile in [70 %, 95 %].
const CLOUD_WAIT_MIN_CONFIDENCE: f32 = 0.70;
const CLOUD_WAIT_MAX_CONFIDENCE: f32 = 0.95;

/// Default time budget before giving up on cloud (ms).
const DEFAULT_CLOUD_WAIT_BUDGET_MS: u64 = 600;

// ─── ConfidenceArbitrator ─────────────────────────────────────────────────────

pub struct ConfidenceArbitrator {
    pub auto_display_threshold: f32,
    pub amber_threshold: f32,
    pub cloud_wait_budget_ms: u64,
}

impl Default for ConfidenceArbitrator {
    fn default() -> Self {
        Self {
            auto_display_threshold: AUTO_DISPLAY_THRESHOLD,
            amber_threshold: AMBER_THRESHOLD,
            cloud_wait_budget_ms: DEFAULT_CLOUD_WAIT_BUDGET_MS,
        }
    }
}

impl ConfidenceArbitrator {
    pub fn new() -> Self {
        Self::default()
    }

    // ── Public methods ────────────────────────────────────────────────────────

    /// `true` when every available layer with a reference agrees on the same
    /// book, chapter and verse.  Requires at least two non-None results.
    pub fn all_agree(&self, results: &PartialResults) -> bool {
        let available: Vec<&LayerResult> = results
            .available_results()
            .into_iter()
            .filter(|r| r.has_reference())
            .collect();

        if available.len() < 2 {
            return false;
        }

        let first = available[0];
        available[1..].iter().all(|r| r.matches(first))
    }

    /// Weighted confidence of the winning reference, boosted when all layers agree.
    ///
    /// Always higher than any individual layer's contribution when all agree.
    pub fn consensus_confidence(&self, results: &PartialResults) -> f32 {
        let base = self.calculate_weighted_confidence(results);
        (base * CONSENSUS_BOOST).min(1.0)
    }

    /// Sum of `weight_i × confidence_i` for every layer that agrees with the
    /// winning reference, divided by the total weight of responding layers.
    ///
    /// Normalising by available weight ensures a single layer at full confidence
    /// still yields a meaningful score rather than being penalised for the
    /// absence of other layers.
    pub fn calculate_weighted_confidence(&self, results: &PartialResults) -> f32 {
        let winner = match self.find_winning_reference(results) {
            Some(w) => w,
            None => return 0.0,
        };

        let mut score = 0.0f32;
        let mut available_weight = 0.0f32;

        if let Some(p) = &results.pattern {
            available_weight += PATTERN_WEIGHT;
            if p.matches(&winner) {
                score += PATTERN_WEIGHT * p.confidence;
            }
        }
        if let Some(l) = &results.local_ai {
            available_weight += LOCAL_AI_WEIGHT;
            if l.matches(&winner) {
                score += LOCAL_AI_WEIGHT * l.confidence;
            }
        }
        if let Some(c) = &results.cloud {
            available_weight += CLOUD_WEIGHT;
            if c.matches(&winner) {
                score += CLOUD_WEIGHT * c.confidence;
            }
        }

        if available_weight == 0.0 {
            return 0.0;
        }

        (score / available_weight).min(1.0)
    }

    /// `true` when waiting for the cloud layer is likely to improve the result.
    ///
    /// Only returns `true` when:
    /// - the cloud layer is still pending, **and**
    /// - `confidence` is in the uncertain band [70 %, 95 %], **and**
    /// - we are still within the time budget.
    pub fn should_wait_for_cloud(&self, results: &PartialResults, confidence: f32) -> bool {
        results.cloud_pending
            && (CLOUD_WAIT_MIN_CONFIDENCE..=CLOUD_WAIT_MAX_CONFIDENCE).contains(&confidence)
            && results.elapsed_ms < self.cloud_wait_budget_ms
    }

    /// Map a confidence score to a display action.
    pub fn decide_action(&self, confidence: f32) -> DisplayAction {
        if confidence >= self.auto_display_threshold {
            DisplayAction::AutoDisplay
        } else if confidence >= self.amber_threshold {
            DisplayAction::DisplayWithAmberWarning
        } else {
            DisplayAction::HoldForOperator
        }
    }

    /// Full arbitration: validate → find winner → blend confidence → decide.
    pub fn arbitrate(&self, results: &PartialResults) -> ArbitrationDecision {
        let results = &self.validate(results);

        let all_agree = self.all_agree(results);

        let confidence = if all_agree {
            self.consensus_confidence(results)
        } else {
            self.calculate_weighted_confidence(results)
        };

        let action = self.decide_action(confidence);
        let should_wait = self.should_wait_for_cloud(results, confidence);
        let reference = self.find_winning_reference(results);

        ArbitrationDecision {
            reference,
            confidence,
            action,
            all_agree,
            should_wait_for_cloud: should_wait,
        }
    }

    // ── Internal helpers ──────────────────────────────────────────────────────

    /// Clamp all confidence values to [0, 1] so no layer can supply invalid input.
    fn validate<'a>(&self, results: &'a PartialResults) -> std::borrow::Cow<'a, PartialResults> {
        let needs_fix = [&results.pattern, &results.local_ai, &results.cloud]
            .iter()
            .filter_map(|o| o.as_ref())
            .any(|r| r.confidence < 0.0 || r.confidence > 1.0);

        if !needs_fix {
            return std::borrow::Cow::Borrowed(results);
        }

        let clamp = |opt: &Option<LayerResult>| {
            opt.as_ref().map(|r| LayerResult {
                confidence: r.confidence.clamp(0.0, 1.0),
                ..r.clone()
            })
        };

        std::borrow::Cow::Owned(PartialResults {
            pattern: clamp(&results.pattern),
            local_ai: clamp(&results.local_ai),
            cloud: clamp(&results.cloud),
            ..results.clone()
        })
    }

    /// The reference supported by the greatest total weighted score across layers.
    /// Returns `None` when no layer produced a usable reference.
    fn find_winning_reference(&self, results: &PartialResults) -> Option<LayerResult> {
        // Collect (reference, raw_score) candidates.
        let mut candidates: Vec<(LayerResult, f32)> = Vec::new();

        let score_for = |existing: &mut Vec<(LayerResult, f32)>, result: &LayerResult, w: f32| {
            if !result.has_reference() {
                return;
            }
            if let Some(entry) = existing.iter_mut().find(|(r, _)| r.matches(result)) {
                entry.1 += w * result.confidence;
            } else {
                existing.push((result.clone(), w * result.confidence));
            }
        };

        if let Some(p) = &results.pattern {
            score_for(&mut candidates, p, PATTERN_WEIGHT);
        }
        if let Some(l) = &results.local_ai {
            score_for(&mut candidates, l, LOCAL_AI_WEIGHT);
        }
        if let Some(c) = &results.cloud {
            score_for(&mut candidates, c, CLOUD_WEIGHT);
        }

        candidates
            .into_iter()
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(r, _)| r)
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn arb() -> ConfidenceArbitrator {
        ConfidenceArbitrator::new()
    }

    fn john_3_16(confidence: f32) -> LayerResult {
        LayerResult::new("John", 3, 16, confidence)
    }

    fn romans_8_28(confidence: f32) -> LayerResult {
        LayerResult::new("Romans", 8, 28, confidence)
    }

    fn hebrews_11_1(confidence: f32) -> LayerResult {
        LayerResult::new("Hebrews", 11, 1, confidence)
    }

    // ── all_agree ─────────────────────────────────────────────────────────────

    #[test]
    fn all_agree_true_when_all_three_match() {
        let results = PartialResults {
            pattern: Some(john_3_16(0.9)),
            local_ai: Some(john_3_16(0.85)),
            cloud: Some(john_3_16(0.92)),
            ..Default::default()
        };
        assert!(arb().all_agree(&results));
    }

    #[test]
    fn all_agree_false_when_layers_disagree() {
        let results = PartialResults {
            pattern: Some(john_3_16(0.9)),
            local_ai: Some(romans_8_28(0.85)),
            ..Default::default()
        };
        assert!(!arb().all_agree(&results));
    }

    #[test]
    fn all_agree_false_with_only_one_layer() {
        let results = PartialResults {
            pattern: Some(john_3_16(0.9)),
            ..Default::default()
        };
        assert!(!arb().all_agree(&results));
    }

    #[test]
    fn all_agree_ignores_unresolved_results() {
        // Pattern has a reference; local AI has unresolved. Only one resolved →
        // cannot agree (need ≥ 2 with references).
        let results = PartialResults {
            pattern: Some(john_3_16(0.9)),
            local_ai: Some(LayerResult::unresolved(0.3)),
            ..Default::default()
        };
        assert!(!arb().all_agree(&results));
    }

    #[test]
    fn all_agree_true_for_two_matching_layers() {
        let results = PartialResults {
            pattern: Some(john_3_16(0.9)),
            local_ai: Some(john_3_16(0.80)),
            ..Default::default()
        };
        assert!(arb().all_agree(&results));
    }

    // ── consensus_confidence ──────────────────────────────────────────────────

    #[test]
    fn consensus_confidence_higher_than_any_individual_layer() {
        let results = PartialResults {
            pattern: Some(john_3_16(0.80)),
            local_ai: Some(john_3_16(0.80)),
            cloud: Some(john_3_16(0.80)),
            ..Default::default()
        };
        let a = arb();
        let consensus = a.consensus_confidence(&results);
        // Pattern alone would contribute only PATTERN_WEIGHT * 0.80 = 0.32.
        assert!(
            consensus > 0.80,
            "consensus {consensus} should beat any single layer (0.80)"
        );
    }

    #[test]
    fn consensus_confidence_capped_at_1() {
        let results = PartialResults {
            pattern: Some(john_3_16(1.0)),
            local_ai: Some(john_3_16(1.0)),
            cloud: Some(john_3_16(1.0)),
            ..Default::default()
        };
        assert!(arb().consensus_confidence(&results) <= 1.0);
    }

    // ── calculate_weighted_confidence ────────────────────────────────────────

    #[test]
    fn weighted_confidence_pattern_only() {
        let results = PartialResults {
            pattern: Some(john_3_16(0.90)),
            ..Default::default()
        };
        let conf = arb().calculate_weighted_confidence(&results);
        // Normalised by pattern weight only → should equal pattern confidence.
        assert!((conf - 0.90).abs() < 0.001, "got {conf}");
    }

    #[test]
    fn weighted_confidence_increases_as_layers_agree() {
        let pattern_only = PartialResults {
            pattern: Some(john_3_16(0.85)),
            ..Default::default()
        };
        let two_agree = PartialResults {
            pattern: Some(john_3_16(0.85)),
            local_ai: Some(john_3_16(0.85)),
            ..Default::default()
        };
        let a = arb();
        let c1 = a.calculate_weighted_confidence(&pattern_only);
        let c2 = a.calculate_weighted_confidence(&two_agree);
        assert!(c2 > c1, "two agreeing layers ({c2}) should beat one ({c1})");
    }

    #[test]
    fn weighted_confidence_zero_with_no_results() {
        let results = PartialResults::default();
        assert_eq!(arb().calculate_weighted_confidence(&results), 0.0);
    }

    #[test]
    fn weighted_confidence_uses_correct_weights() {
        // Only pattern result at 1.0 → normalised = 1.0 (single layer, full weight used).
        let results = PartialResults {
            pattern: Some(john_3_16(1.0)),
            ..Default::default()
        };
        let conf = arb().calculate_weighted_confidence(&results);
        assert!((conf - 1.0).abs() < 0.001);
    }

    // ── should_wait_for_cloud ─────────────────────────────────────────────────

    #[test]
    fn should_wait_when_in_uncertain_band_and_within_budget() {
        let results = PartialResults {
            pattern: Some(john_3_16(0.80)),
            cloud_pending: true,
            elapsed_ms: 200,
            ..Default::default()
        };
        assert!(arb().should_wait_for_cloud(&results, 0.80));
    }

    #[test]
    fn should_not_wait_when_cloud_not_pending() {
        let results = PartialResults {
            pattern: Some(john_3_16(0.80)),
            cloud_pending: false,
            elapsed_ms: 200,
            ..Default::default()
        };
        assert!(!arb().should_wait_for_cloud(&results, 0.80));
    }

    #[test]
    fn should_not_wait_when_confidence_too_low() {
        let results = PartialResults {
            pattern: Some(john_3_16(0.50)),
            cloud_pending: true,
            elapsed_ms: 200,
            ..Default::default()
        };
        // 0.50 < CLOUD_WAIT_MIN_CONFIDENCE (0.70)
        assert!(!arb().should_wait_for_cloud(&results, 0.50));
    }

    #[test]
    fn should_not_wait_when_confidence_already_high() {
        let results = PartialResults {
            pattern: Some(john_3_16(0.97)),
            cloud_pending: true,
            elapsed_ms: 200,
            ..Default::default()
        };
        // 0.97 > CLOUD_WAIT_MAX_CONFIDENCE (0.95)
        assert!(!arb().should_wait_for_cloud(&results, 0.97));
    }

    #[test]
    fn should_not_wait_when_budget_exceeded() {
        let results = PartialResults {
            pattern: Some(john_3_16(0.80)),
            cloud_pending: true,
            elapsed_ms: 700, // exceeds default 600ms budget
            ..Default::default()
        };
        assert!(!arb().should_wait_for_cloud(&results, 0.80));
    }

    // ── decide_action ─────────────────────────────────────────────────────────

    #[test]
    fn decide_auto_display_above_threshold() {
        assert_eq!(arb().decide_action(0.90), DisplayAction::AutoDisplay);
        assert_eq!(arb().decide_action(0.85), DisplayAction::AutoDisplay);
        assert_eq!(arb().decide_action(1.00), DisplayAction::AutoDisplay);
    }

    #[test]
    fn decide_amber_warning_in_middle_band() {
        assert_eq!(
            arb().decide_action(0.75),
            DisplayAction::DisplayWithAmberWarning
        );
        assert_eq!(
            arb().decide_action(0.60),
            DisplayAction::DisplayWithAmberWarning
        );
        assert_eq!(
            arb().decide_action(0.84),
            DisplayAction::DisplayWithAmberWarning
        );
    }

    #[test]
    fn decide_hold_below_amber_threshold() {
        assert_eq!(arb().decide_action(0.50), DisplayAction::HoldForOperator);
        assert_eq!(arb().decide_action(0.00), DisplayAction::HoldForOperator);
        assert_eq!(arb().decide_action(0.59), DisplayAction::HoldForOperator);
    }

    // ── validate ─────────────────────────────────────────────────────────────

    #[test]
    fn validate_clamps_negative_confidence() {
        let results = PartialResults {
            pattern: Some(LayerResult {
                confidence: -0.5,
                ..john_3_16(0.0)
            }),
            ..Default::default()
        };
        let decision = arb().arbitrate(&results);
        assert!(decision.confidence >= 0.0);
    }

    #[test]
    fn validate_clamps_confidence_above_1() {
        let results = PartialResults {
            pattern: Some(LayerResult {
                confidence: 1.5,
                ..john_3_16(0.0)
            }),
            ..Default::default()
        };
        let a = arb();
        let conf = a.calculate_weighted_confidence(&results);
        assert!(conf <= 1.0);
    }

    // ── arbitrate — full scenarios ────────────────────────────────────────────

    #[test]
    fn scenario_all_layers_agree_high_confidence() {
        let results = PartialResults {
            pattern: Some(john_3_16(0.90)),
            local_ai: Some(john_3_16(0.88)),
            cloud: Some(john_3_16(0.92)),
            ..Default::default()
        };
        let decision = arb().arbitrate(&results);

        assert!(decision.all_agree);
        assert!(
            decision.confidence > 0.88,
            "confidence {} should be high",
            decision.confidence
        );
        assert_eq!(decision.action, DisplayAction::AutoDisplay);
        assert_eq!(
            decision.reference.as_ref().unwrap().book.as_deref(),
            Some("John")
        );
    }

    #[test]
    fn scenario_all_layers_disagree_low_confidence_hold() {
        let results = PartialResults {
            pattern: Some(john_3_16(0.55)),
            local_ai: Some(romans_8_28(0.60)),
            cloud: Some(hebrews_11_1(0.65)),
            ..Default::default()
        };
        let decision = arb().arbitrate(&results);

        assert!(!decision.all_agree);
        assert_eq!(decision.action, DisplayAction::HoldForOperator);
    }

    #[test]
    fn scenario_only_pattern_available_offline() {
        let results = PartialResults {
            pattern: Some(john_3_16(0.92)),
            local_ai_pending: false,
            cloud_pending: false,
            elapsed_ms: 0,
            ..Default::default()
        };
        let decision = arb().arbitrate(&results);

        // Pattern alone at 0.92 → normalised = 0.92 → AutoDisplay.
        assert_eq!(decision.action, DisplayAction::AutoDisplay);
        assert_eq!(
            decision.reference.as_ref().unwrap().book.as_deref(),
            Some("John")
        );
    }

    #[test]
    fn scenario_pattern_and_local_agree_cloud_times_out() {
        let results = PartialResults {
            pattern: Some(john_3_16(0.88)),
            local_ai: Some(john_3_16(0.85)),
            // cloud responded with nothing (timed out).
            cloud: None,
            cloud_pending: false,
            ..Default::default()
        };
        let decision = arb().arbitrate(&results);

        // Two layers agree → consensus boost applied.
        assert!(decision.all_agree);
        assert!(decision.confidence >= 0.85);
        assert!(matches!(
            decision.action,
            DisplayAction::AutoDisplay | DisplayAction::DisplayWithAmberWarning
        ));
    }

    #[test]
    fn scenario_cloud_overrides_pattern_with_higher_confidence() {
        // Pattern weakly suggests John 3:16; cloud is very confident about Romans 8:28.
        let results = PartialResults {
            pattern: Some(john_3_16(0.40)), // raw score: 0.4 × 0.40 = 0.16
            cloud: Some(romans_8_28(0.95)), // raw score: 0.25 × 0.95 = 0.2375
            cloud_pending: false,
            ..Default::default()
        };
        let decision = arb().arbitrate(&results);

        // Romans 8:28 has the higher weighted score → cloud wins.
        assert_eq!(
            decision.reference.as_ref().unwrap().book.as_deref(),
            Some("Romans"),
            "cloud reference should override weak pattern"
        );
    }
}
