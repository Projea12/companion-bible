//! In-memory calibration thresholds for the arbitrator display layer.

use companion_database::ServiceRecord;

// ─── bounds ───────────────────────────────────────────────────────────────────

pub const AUTO_DISPLAY_MIN: f32 = 0.85;
pub const AUTO_DISPLAY_MAX: f32 = 0.99;
pub const WARNING_MIN: f32 = 0.60;
/// Must stay at least this far below `auto_display`.
const WARNING_GAP: f32 = 0.05;
const MAX_ADJUSTMENT: f32 = 0.03;

// ─── trigger rates ────────────────────────────────────────────────────────────

const HIGH_DISCARD_RATE: f32 = 0.20;
const HIGH_CORRECTION_RATE: f32 = 0.15;
const LOW_DISCARD_RATE: f32 = 0.05;
const LOW_CORRECTION_RATE: f32 = 0.05;
const HIGH_AUTO_ACCEPT_RATE: f32 = 0.70;

// ─── CalibrationThresholds ────────────────────────────────────────────────────

/// Arbitrator-level display thresholds, owned by `ChurchCalibrator`.
///
/// These differ from the per-layer DB rows: they govern what the final
/// arbitration decision emits to the display layer.
#[derive(Debug, Clone, PartialEq)]
pub struct CalibrationThresholds {
    /// Confidence ≥ this → `AutoDisplay`.  Default 0.95.
    pub auto_display: f32,
    /// Confidence ≥ this (but < `auto_display`) → `DisplayWithAmberWarning`. Default 0.75.
    pub show_with_warning: f32,
    /// Calibration is only applied after this many services have been recorded.
    pub minimum_services_for_calibration: u32,
}

impl Default for CalibrationThresholds {
    fn default() -> Self {
        Self {
            auto_display: 0.95,
            show_with_warning: 0.65,
            minimum_services_for_calibration: 5,
        }
    }
}

impl CalibrationThresholds {
    pub fn new(auto_display: f32, show_with_warning: f32) -> Self {
        let clamped_auto = auto_display.clamp(AUTO_DISPLAY_MIN, AUTO_DISPLAY_MAX);
        let warning_ceil = (clamped_auto - WARNING_GAP).max(WARNING_MIN);
        Self {
            auto_display: clamped_auto,
            show_with_warning: show_with_warning.clamp(WARNING_MIN, warning_ceil),
            ..Default::default()
        }
    }

    /// Pure function: compute new thresholds after observing `record`.
    ///
    /// Adjustments are bounded to ±`MAX_ADJUSTMENT` per service and the
    /// resulting thresholds are clamped to [0.85, 0.99].
    pub fn adjust_for_service(&self, record: &ServiceRecord) -> Self {
        if record.total_detections == 0 {
            return self.clone();
        }

        let total = record.total_detections as f32;
        let discard_rate = record.rejected as f32 / total;
        let correction_rate = record.operator_corrected as f32 / total;
        let auto_accept_rate = record.auto_accepted as f32 / total;

        let mut auto_delta: f32 = 0.0;
        let mut warning_delta: f32 = 0.0;

        // Too many discards → system surfacing noise → tighten
        if discard_rate > HIGH_DISCARD_RATE {
            auto_delta += 0.01;
            warning_delta += 0.01;
        }

        // Auto-displayed references often wrong → tighten auto_display
        if correction_rate > HIGH_CORRECTION_RATE {
            auto_delta += 0.01;
        }

        // Both rates simultaneously high → bigger correction
        if discard_rate > HIGH_DISCARD_RATE && correction_rate > HIGH_CORRECTION_RATE {
            auto_delta += 0.01;
        }

        // Very clean service → cautiously relax
        if discard_rate < LOW_DISCARD_RATE
            && correction_rate < LOW_CORRECTION_RATE
            && auto_accept_rate > HIGH_AUTO_ACCEPT_RATE
        {
            auto_delta -= 0.01;
            warning_delta -= 0.005;
        }

        auto_delta = auto_delta.clamp(-MAX_ADJUSTMENT, MAX_ADJUSTMENT);
        warning_delta = warning_delta.clamp(-MAX_ADJUSTMENT, MAX_ADJUSTMENT);

        let new_auto = (self.auto_display + auto_delta)
            .clamp(AUTO_DISPLAY_MIN, AUTO_DISPLAY_MAX);

        let warning_ceiling = (new_auto - WARNING_GAP).max(WARNING_MIN);
        let new_warning = (self.show_with_warning + warning_delta)
            .clamp(WARNING_MIN, warning_ceiling);

        Self {
            auto_display: new_auto,
            show_with_warning: new_warning,
            minimum_services_for_calibration: self.minimum_services_for_calibration,
        }
    }
}

// ─── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn record(total: i64, auto: i64, corrected: i64, rejected: i64) -> ServiceRecord {
        ServiceRecord {
            id: "test".into(),
            sermon_id: "s1".into(),
            total_detections: total,
            auto_accepted: auto,
            operator_corrected: corrected,
            rejected,
            avg_confidence: None,
            avg_processing_time_ms: None,
            created_at: "2026-01-01T00:00:00Z".into(),
        }
    }

    #[test]
    fn defaults_are_correct() {
        let t = CalibrationThresholds::default();
        assert_eq!(t.auto_display, 0.95);
        assert_eq!(t.show_with_warning, 0.65);
        assert_eq!(t.minimum_services_for_calibration, 5);
    }

    #[test]
    fn no_change_when_zero_detections() {
        let t = CalibrationThresholds::default();
        let after = t.adjust_for_service(&record(0, 0, 0, 0));
        assert_eq!(after, t);
    }

    #[test]
    fn high_discard_rate_raises_both_thresholds() {
        let t = CalibrationThresholds::default();
        // 25 rejected out of 100 → discard_rate = 0.25 > 0.20
        let after = t.adjust_for_service(&record(100, 70, 5, 25));
        assert!(after.auto_display > t.auto_display, "auto_display should rise");
        assert!(after.show_with_warning > t.show_with_warning, "warning should rise");
    }

    #[test]
    fn high_correction_rate_raises_auto_display_only() {
        let t = CalibrationThresholds::default();
        // 20 corrected, 5 rejected → correction_rate=0.20 > 0.15, discard_rate=0.05=0.05
        let after = t.adjust_for_service(&record(100, 75, 20, 5));
        assert!(after.auto_display > t.auto_display, "auto_display should rise");
    }

    #[test]
    fn both_rates_high_triggers_bigger_raise() {
        let t = CalibrationThresholds::default();
        // discard=0.25 > 0.20, correction=0.20 > 0.15 → delta should be 0.03
        let after = t.adjust_for_service(&record(100, 55, 20, 25));
        assert!(
            after.auto_display >= t.auto_display + 0.025,
            "should get max upward adjustment"
        );
    }

    #[test]
    fn clean_service_lowers_thresholds() {
        let t = CalibrationThresholds::new(0.96, 0.80);
        // 80% auto-accepted, 2% corrections, 2% rejections
        let after = t.adjust_for_service(&record(100, 80, 2, 2));
        assert!(after.auto_display < t.auto_display, "auto_display should relax");
        assert!(after.show_with_warning < t.show_with_warning, "warning should relax");
    }

    #[test]
    fn auto_display_never_below_minimum() {
        let t = CalibrationThresholds::new(AUTO_DISPLAY_MIN, 0.75);
        let after = t.adjust_for_service(&record(100, 80, 2, 2));
        assert!(after.auto_display >= AUTO_DISPLAY_MIN);
    }

    #[test]
    fn auto_display_never_above_maximum() {
        let t = CalibrationThresholds::new(AUTO_DISPLAY_MAX, 0.80);
        let after = t.adjust_for_service(&record(100, 55, 20, 25));
        assert!(after.auto_display <= AUTO_DISPLAY_MAX);
    }

    #[test]
    fn warning_threshold_always_below_auto_display() {
        let t = CalibrationThresholds::default();
        let after = t.adjust_for_service(&record(100, 55, 20, 25));
        assert!(
            after.show_with_warning < after.auto_display,
            "invariant: show_with_warning < auto_display"
        );
    }

    #[test]
    fn adjustment_per_service_capped_at_max() {
        let t = CalibrationThresholds::default();
        // extreme case — all rejected
        let after = t.adjust_for_service(&record(100, 0, 0, 100));
        let delta = (after.auto_display - t.auto_display).abs();
        assert!(delta <= MAX_ADJUSTMENT + f32::EPSILON);
    }

    #[test]
    fn new_clamps_out_of_range_values() {
        let t = CalibrationThresholds::new(0.50, 0.20); // both below minimums
        assert!(t.auto_display >= AUTO_DISPLAY_MIN);
        assert!(t.show_with_warning >= WARNING_MIN);
    }

    // ── calibration adjustment ────────────────────────────────────────────────

    /// High discard rate (> 20 %) drives auto_display upward over multiple services.
    #[test]
    fn high_discard_rate_increases_threshold_over_services() {
        let mut t = CalibrationThresholds::default();
        let initial = t.auto_display;
        // 25 % discard rate — clearly above the 20 % trigger
        let bad_service = record(100, 65, 10, 25);
        for _ in 0..3 {
            t = t.adjust_for_service(&bad_service);
        }
        assert!(
            t.auto_display > initial,
            "auto_display ({}) should be above initial ({initial}) after high-discard services",
            t.auto_display
        );
    }

    /// Low discard and correction rates (both < 5 %) with high auto-accept (> 70 %)
    /// drive auto_display downward over multiple services.
    #[test]
    fn low_discard_rate_decreases_threshold_over_services() {
        // Start above minimum so there is room to decrease.
        let mut t = CalibrationThresholds::new(0.97, 0.82);
        let initial = t.auto_display;
        // 2 % discard, 2 % correction, 80 % auto-accepted
        let clean_service = record(100, 80, 2, 2);
        for _ in 0..3 {
            t = t.adjust_for_service(&clean_service);
        }
        assert!(
            t.auto_display < initial,
            "auto_display ({}) should be below initial ({initial}) after clean services",
            t.auto_display
        );
    }

    /// Error (correction) rate above 15 % raises auto_display even when
    /// discard rate is low.
    #[test]
    fn error_rate_above_target_increases_threshold() {
        let mut t = CalibrationThresholds::default();
        let initial = t.auto_display;
        // 18 % correction rate, 4 % discard — only correction trigger fires
        let noisy_service = record(100, 78, 18, 4);
        for _ in 0..3 {
            t = t.adjust_for_service(&noisy_service);
        }
        assert!(
            t.auto_display > initial,
            "auto_display ({}) should rise when error rate exceeds target",
            t.auto_display
        );
    }

    // ── threshold bounds ──────────────────────────────────────────────────────

    /// No matter how many high-discard services occur, auto_display must not
    /// exceed 0.99.
    #[test]
    fn threshold_never_exceeds_0_99() {
        let mut t = CalibrationThresholds::new(0.99, 0.88);
        // All detections rejected — maximally bad input
        let worst = record(100, 0, 0, 100);
        for _ in 0..50 {
            t = t.adjust_for_service(&worst);
        }
        assert!(
            t.auto_display <= 0.99,
            "auto_display ({}) must never exceed 0.99",
            t.auto_display
        );
        assert!(
            t.show_with_warning <= 0.99,
            "show_with_warning ({}) must never exceed 0.99",
            t.show_with_warning
        );
    }

    /// No matter how many clean services occur, auto_display must not fall
    /// below 0.85.
    #[test]
    fn threshold_never_falls_below_0_85() {
        let mut t = CalibrationThresholds::new(0.85, 0.75);
        // Perfect service — 100 % auto-accepted, zero corrections or discards
        let perfect = record(100, 100, 0, 0);
        for _ in 0..50 {
            t = t.adjust_for_service(&perfect);
        }
        assert!(
            t.auto_display >= 0.85,
            "auto_display ({}) must never fall below 0.85",
            t.auto_display
        );
    }

    /// Boundaries hold across the full range: starting at default thresholds
    /// and hammering with opposite extremes should always stay in [0.85, 0.99].
    #[test]
    fn bounds_hold_under_alternating_extreme_services() {
        let mut t = CalibrationThresholds::default();
        let bad = record(100, 0, 0, 100);
        let perfect = record(100, 100, 0, 0);
        for i in 0..100 {
            t = t.adjust_for_service(if i % 2 == 0 { &bad } else { &perfect });
            assert!(t.auto_display >= 0.85, "auto_display dropped below 0.85 on iteration {i}");
            assert!(t.auto_display <= 0.99, "auto_display exceeded 0.99 on iteration {i}");
            assert!(
                t.show_with_warning < t.auto_display,
                "invariant violated on iteration {i}"
            );
        }
    }
}
