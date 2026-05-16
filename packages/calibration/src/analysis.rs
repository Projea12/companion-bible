//! Operator pattern analysis types.

use companion_database::ServiceRecord;

use crate::thresholds::CalibrationThresholds;

// ─── CalibrationTrend ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalibrationTrend {
    /// Rates are within acceptable bands — no structural change needed.
    Stable,
    /// Discard or correction rates are elevated — thresholds should rise.
    TighteningUp,
    /// Low error rates over many services — thresholds can safely drop.
    RelaxingDown,
}

// ─── OperatorAnalysis ─────────────────────────────────────────────────────────

/// Summary produced by `ChurchCalibrator::analyze_operator_patterns`.
#[derive(Debug, Clone)]
pub struct OperatorAnalysis {
    /// Number of service records used in this analysis.
    pub services_analyzed: u32,
    /// Average fraction of detections the operator discarded.
    pub avg_discard_rate: f32,
    /// Average fraction of detections the operator had to correct.
    pub avg_correction_rate: f32,
    /// Average fraction of detections accepted without intervention.
    pub avg_auto_accept_rate: f32,
    /// Direction the calibration is trending.
    pub trend: CalibrationTrend,
    /// Threshold value this analysis recommends for auto-display.
    pub recommended_auto_display: f32,
    /// Threshold value this analysis recommends for the amber warning band.
    pub recommended_show_with_warning: f32,
}

// ─── calculation ─────────────────────────────────────────────────────────────

const HIGH_DISCARD: f32 = 0.15;
const HIGH_CORRECTION: f32 = 0.12;
const LOW_DISCARD: f32 = 0.04;
const LOW_CORRECTION: f32 = 0.04;
const MIN_SERVICES_FOR_RELAX: u32 = 3;

/// Derive an `OperatorAnalysis` from a slice of recent service records and the
/// current calibration thresholds.
pub fn compute_analysis(
    records: &[ServiceRecord],
    current: &CalibrationThresholds,
) -> OperatorAnalysis {
    if records.is_empty() {
        return OperatorAnalysis {
            services_analyzed: 0,
            avg_discard_rate: 0.0,
            avg_correction_rate: 0.0,
            avg_auto_accept_rate: 0.0,
            trend: CalibrationTrend::Stable,
            recommended_auto_display: current.auto_display,
            recommended_show_with_warning: current.show_with_warning,
        };
    }

    let mut total_discard = 0.0_f32;
    let mut total_correction = 0.0_f32;
    let mut total_auto = 0.0_f32;
    let mut counted = 0u32;

    for r in records {
        if r.total_detections == 0 {
            continue;
        }
        let total = r.total_detections as f32;
        total_discard += r.rejected as f32 / total;
        total_correction += r.operator_corrected as f32 / total;
        total_auto += r.auto_accepted as f32 / total;
        counted += 1;
    }

    if counted == 0 {
        return OperatorAnalysis {
            services_analyzed: 0,
            avg_discard_rate: 0.0,
            avg_correction_rate: 0.0,
            avg_auto_accept_rate: 0.0,
            trend: CalibrationTrend::Stable,
            recommended_auto_display: current.auto_display,
            recommended_show_with_warning: current.show_with_warning,
        };
    }

    let avg_discard = total_discard / counted as f32;
    let avg_correction = total_correction / counted as f32;
    let avg_auto = total_auto / counted as f32;

    let trend = if avg_discard > HIGH_DISCARD || avg_correction > HIGH_CORRECTION {
        CalibrationTrend::TighteningUp
    } else if counted >= MIN_SERVICES_FOR_RELAX
        && avg_discard < LOW_DISCARD
        && avg_correction < LOW_CORRECTION
    {
        CalibrationTrend::RelaxingDown
    } else {
        CalibrationTrend::Stable
    };

    // Derive recommended thresholds by simulating adjustments over the window.
    let mut simulated = current.clone();
    for r in records {
        simulated = simulated.adjust_for_service(r);
    }

    OperatorAnalysis {
        services_analyzed: counted,
        avg_discard_rate: avg_discard,
        avg_correction_rate: avg_correction,
        avg_auto_accept_rate: avg_auto,
        trend,
        recommended_auto_display: simulated.auto_display,
        recommended_show_with_warning: simulated.show_with_warning,
    }
}

// ─── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(total: i64, auto: i64, corrected: i64, rejected: i64) -> ServiceRecord {
        ServiceRecord {
            id: "r".into(),
            sermon_id: "s".into(),
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
    fn empty_records_returns_current_thresholds() {
        let current = CalibrationThresholds::default();
        let a = compute_analysis(&[], &current);
        assert_eq!(a.services_analyzed, 0);
        assert_eq!(a.recommended_auto_display, current.auto_display);
        assert_eq!(a.trend, CalibrationTrend::Stable);
    }

    #[test]
    fn high_discard_signals_tightening_trend() {
        let current = CalibrationThresholds::default();
        // 25% discard rate across 3 services (> both 0.15 trend trigger and 0.20 adjust trigger)
        let records: Vec<ServiceRecord> = (0..3).map(|_| rec(100, 63, 12, 25)).collect();
        let a = compute_analysis(&records, &current);
        assert_eq!(a.trend, CalibrationTrend::TighteningUp);
        assert!(a.recommended_auto_display > current.auto_display);
    }

    #[test]
    fn clean_services_signal_relaxing_trend() {
        let current = CalibrationThresholds::new(0.96, 0.82);
        // 2% discard, 2% correction, 80% auto-accept over 4 services
        let records: Vec<ServiceRecord> = (0..4).map(|_| rec(100, 80, 2, 2)).collect();
        let a = compute_analysis(&records, &current);
        assert_eq!(a.trend, CalibrationTrend::RelaxingDown);
        assert!(a.recommended_auto_display < current.auto_display);
    }

    #[test]
    fn relaxing_trend_requires_minimum_services() {
        let current = CalibrationThresholds::new(0.96, 0.82);
        // Only 2 services — below MIN_SERVICES_FOR_RELAX
        let records: Vec<ServiceRecord> = (0..2).map(|_| rec(100, 80, 2, 2)).collect();
        let a = compute_analysis(&records, &current);
        assert_eq!(a.trend, CalibrationTrend::Stable);
    }

    #[test]
    fn rates_are_averaged_correctly() {
        let current = CalibrationThresholds::default();
        // service 1: 10% reject, service 2: 30% reject → avg 20%
        let records = vec![rec(100, 80, 5, 10), rec(100, 60, 5, 30)];
        let a = compute_analysis(&records, &current);
        assert!((a.avg_discard_rate - 0.20).abs() < 0.001);
    }

    #[test]
    fn zero_detection_records_are_skipped() {
        let current = CalibrationThresholds::default();
        let records = vec![rec(0, 0, 0, 0), rec(100, 80, 5, 10)];
        let a = compute_analysis(&records, &current);
        assert_eq!(a.services_analyzed, 1);
    }

    #[test]
    fn recommended_thresholds_stay_in_bounds() {
        use crate::thresholds::{AUTO_DISPLAY_MAX, AUTO_DISPLAY_MIN, WARNING_MIN};
        let current = CalibrationThresholds::default();
        let records: Vec<ServiceRecord> = (0..10).map(|_| rec(100, 40, 25, 30)).collect();
        let a = compute_analysis(&records, &current);
        assert!(a.recommended_auto_display >= AUTO_DISPLAY_MIN);
        assert!(a.recommended_auto_display <= AUTO_DISPLAY_MAX);
        assert!(a.recommended_show_with_warning >= WARNING_MIN);
        assert!(a.recommended_show_with_warning < a.recommended_auto_display);
    }
}
