//! ChurchCalibrator: loads, adjusts, and persists confidence thresholds.

use companion_database::{CalibrationRepository, ChurchRepository, ServiceRecord};

use crate::{
    analysis::{compute_analysis, OperatorAnalysis},
    error::CalibrationError,
    thresholds::CalibrationThresholds,
};

// ─── persistence keys ─────────────────────────────────────────────────────────

const KEY_AUTO_DISPLAY: &str = "calibration.auto_display";
const KEY_SHOW_WITH_WARNING: &str = "calibration.show_with_warning";

/// Number of recent service records fetched for operator pattern analysis.
const ANALYSIS_WINDOW: i64 = 20;

// ─── ChurchCalibrator ─────────────────────────────────────────────────────────

/// Self-calibrating threshold manager.
///
/// Loads persisted thresholds from `church_settings` on startup, updates them
/// after each service, and persists the result back.  Operator pattern analysis
/// inspects the last `ANALYSIS_WINDOW` service records.
pub struct ChurchCalibrator {
    thresholds: CalibrationThresholds,
    church_repo: ChurchRepository,
    calibration_repo: CalibrationRepository,
    services_seen: u32,
}

impl ChurchCalibrator {
    /// Load thresholds from the database, falling back to defaults when no
    /// persisted state is found.
    pub async fn load(
        church_repo: ChurchRepository,
        calibration_repo: CalibrationRepository,
    ) -> Result<Self, CalibrationError> {
        let auto_display = load_f32(&church_repo, KEY_AUTO_DISPLAY).await?
            .unwrap_or(CalibrationThresholds::default().auto_display);

        let show_with_warning = load_f32(&church_repo, KEY_SHOW_WITH_WARNING).await?
            .unwrap_or(CalibrationThresholds::default().show_with_warning);

        let thresholds = CalibrationThresholds::new(auto_display, show_with_warning);

        let services_seen = calibration_repo
            .get_recent_service_records(i64::MAX)
            .await
            .map(|v| v.len() as u32)
            .unwrap_or(0);

        Ok(Self {
            thresholds,
            church_repo,
            calibration_repo,
            services_seen,
        })
    }

    /// Adjust thresholds based on `record`, then persist the new values.
    ///
    /// Calibration is only applied after `minimum_services_for_calibration`
    /// services have been seen — before that the default thresholds are kept.
    pub async fn update_after_service(
        &mut self,
        record: &ServiceRecord,
    ) -> Result<(), CalibrationError> {
        self.services_seen += 1;

        if self.services_seen >= self.thresholds.minimum_services_for_calibration {
            self.thresholds = self.thresholds.adjust_for_service(record);
            self.persist().await?;
        }

        Ok(())
    }

    /// Analyse the last `ANALYSIS_WINDOW` services and return operator pattern
    /// insights plus recommended threshold adjustments.
    pub async fn analyze_operator_patterns(&self) -> Result<OperatorAnalysis, CalibrationError> {
        let records = self
            .calibration_repo
            .get_recent_service_records(ANALYSIS_WINDOW)
            .await?;

        Ok(compute_analysis(&records, &self.thresholds))
    }

    /// Current thresholds.
    pub fn thresholds(&self) -> &CalibrationThresholds {
        &self.thresholds
    }

    /// How many services have been processed since startup (or ever, if the DB
    /// count was loaded successfully).
    pub fn services_seen(&self) -> u32 {
        self.services_seen
    }

    // ── private ───────────────────────────────────────────────────────────────

    async fn persist(&self) -> Result<(), CalibrationError> {
        self.church_repo
            .update_setting(KEY_AUTO_DISPLAY, &self.thresholds.auto_display.to_string())
            .await?;

        self.church_repo
            .update_setting(
                KEY_SHOW_WITH_WARNING,
                &self.thresholds.show_with_warning.to_string(),
            )
            .await?;

        Ok(())
    }
}

// ─── helpers ─────────────────────────────────────────────────────────────────

async fn load_f32(
    repo: &ChurchRepository,
    key: &str,
) -> Result<Option<f32>, CalibrationError> {
    match repo.get_setting(key).await? {
        None => Ok(None),
        Some(s) => s
            .parse::<f32>()
            .map(Some)
            .map_err(|_| CalibrationError::ParseError(format!("{key}={s:?}"))),
    }
}

// ─── tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use companion_database::{connect, CalibrationRepository, ChurchRepository, DbPool, PoolConfig};
    use std::path::Path;
    use tempfile::TempDir;

    async fn setup_db() -> (DbPool, TempDir) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.db");
        // connect() automatically runs migrations
        let pool = connect(Path::new(path.to_str().unwrap()), &PoolConfig::default())
            .await
            .unwrap();

        // seed church + sermon so FK constraints are satisfied
        sqlx::query(
            "INSERT INTO churches (id, name, region) VALUES ('c1', 'Test Church', 'uk')",
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO sermons (id, church_id, date, started_at)
             VALUES ('sermon1', 'c1', '2026-01-01', '2026-01-01T10:00:00Z')",
        )
        .execute(&pool)
        .await
        .unwrap();

        (pool, dir)
    }

    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    fn make_record(total: i64, auto: i64, corrected: i64, rejected: i64) -> ServiceRecord {
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        ServiceRecord {
            id: format!("r{n}"),
            sermon_id: "sermon1".into(),
            total_detections: total,
            auto_accepted: auto,
            operator_corrected: corrected,
            rejected,
            avg_confidence: None,
            avg_processing_time_ms: None,
            created_at: format!("2026-01-01T{:02}:{:02}:{:02}Z", n / 3600, (n / 60) % 60, n % 60),
        }
    }

    #[tokio::test]
    async fn loads_defaults_when_no_persisted_state() {
        let (pool, _dir) = setup_db().await;
        let calibrator = ChurchCalibrator::load(
            ChurchRepository::new(pool.clone()),
            CalibrationRepository::new(pool),
        )
        .await
        .unwrap();

        let defaults = CalibrationThresholds::default();
        assert_eq!(calibrator.thresholds().auto_display, defaults.auto_display);
        assert_eq!(calibrator.thresholds().show_with_warning, defaults.show_with_warning);
    }

    #[tokio::test]
    async fn persists_and_reloads_thresholds() {
        let (pool, _dir) = setup_db().await;

        let mut calibrator = ChurchCalibrator::load(
            ChurchRepository::new(pool.clone()),
            CalibrationRepository::new(pool.clone()),
        )
        .await
        .unwrap();

        // Drive calibration past the minimum_services threshold
        for _ in 0..5 {
            calibrator
                .update_after_service(&make_record(100, 50, 20, 30))
                .await
                .unwrap();
        }

        let saved = calibrator.thresholds().auto_display;

        // Reload from the same pool
        let reloaded = ChurchCalibrator::load(
            ChurchRepository::new(pool.clone()),
            CalibrationRepository::new(pool),
        )
        .await
        .unwrap();

        assert!(
            (reloaded.thresholds().auto_display - saved).abs() < 0.001,
            "reloaded auto_display should match what was persisted"
        );
    }

    #[tokio::test]
    async fn calibration_not_applied_before_minimum_services() {
        let (pool, _dir) = setup_db().await;

        let mut calibrator = ChurchCalibrator::load(
            ChurchRepository::new(pool.clone()),
            CalibrationRepository::new(pool),
        )
        .await
        .unwrap();

        let defaults = CalibrationThresholds::default();

        // 4 services — one short of the minimum (5)
        for _ in 0..4 {
            calibrator
                .update_after_service(&make_record(100, 50, 20, 30))
                .await
                .unwrap();
        }

        assert_eq!(
            calibrator.thresholds().auto_display,
            defaults.auto_display,
            "thresholds should not change before minimum_services_for_calibration"
        );
    }

    #[tokio::test]
    async fn thresholds_rise_after_high_discard_services() {
        let (pool, _dir) = setup_db().await;

        let mut calibrator = ChurchCalibrator::load(
            ChurchRepository::new(pool.clone()),
            CalibrationRepository::new(pool),
        )
        .await
        .unwrap();

        let initial = calibrator.thresholds().auto_display;

        // 6 high-discard services (25% discard rate)
        for _ in 0..6 {
            calibrator
                .update_after_service(&make_record(100, 65, 10, 25))
                .await
                .unwrap();
        }

        assert!(
            calibrator.thresholds().auto_display > initial,
            "thresholds should tighten after high-discard services"
        );
    }

    #[tokio::test]
    async fn analyze_operator_patterns_on_empty_db_returns_stable() {
        let (pool, _dir) = setup_db().await;

        let calibrator = ChurchCalibrator::load(
            ChurchRepository::new(pool.clone()),
            CalibrationRepository::new(pool),
        )
        .await
        .unwrap();

        let analysis = calibrator.analyze_operator_patterns().await.unwrap();
        // No records yet — should be stable with current thresholds as recommendation
        assert_eq!(analysis.services_analyzed, 0);
        assert_eq!(analysis.trend, crate::analysis::CalibrationTrend::Stable);
        assert_eq!(
            analysis.recommended_auto_display,
            calibrator.thresholds().auto_display
        );
    }

    #[tokio::test]
    async fn services_seen_increments_on_each_update() {
        let (pool, _dir) = setup_db().await;

        let mut calibrator = ChurchCalibrator::load(
            ChurchRepository::new(pool.clone()),
            CalibrationRepository::new(pool),
        )
        .await
        .unwrap();

        let initial = calibrator.services_seen();
        calibrator
            .update_after_service(&make_record(100, 80, 5, 5))
            .await
            .unwrap();
        assert_eq!(calibrator.services_seen(), initial + 1);
    }
}
