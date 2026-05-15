use serde::{Deserialize, Serialize};

// ─── Church ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct Church {
    pub id: String,
    pub name: String,
    pub region: String,
    pub installed_at: String,
    pub onboarding_complete: bool,
}

// ─── Verse ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct Verse {
    pub id: i64,
    pub book: String,
    pub chapter: i64,
    pub verse_number: i64,
    pub text: String,
    pub book_order: i64,
}

// ─── Sermon ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct Sermon {
    pub id: String,
    pub church_id: String,
    pub title: Option<String>,
    pub pastor: Option<String>,
    pub date: String,
    pub anchor_scripture: Option<String>,
    pub started_at: String,
    pub ended_at: Option<String>,
}

// ─── SubPoint ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct SubPoint {
    pub id: String,
    pub sermon_id: String,
    pub title: String,
    pub order_index: i64,
    pub started_at: Option<String>,
}

// ─── DetectionEvent ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct DetectionEvent {
    pub id: String,
    pub sermon_id: String,
    pub raw_transcript: String,
    pub pattern_result: Option<String>,
    pub local_ai_result: Option<String>,
    pub cloud_ai_result: Option<String>,
    pub final_reference: Option<String>,
    pub confidence: f64,
    pub decision: String,
    pub operator_action: Option<String>,
    pub correct_reference: Option<String>,
    pub processing_time_ms: i64,
    pub timestamp: String,
}

// ─── ChurchSettings ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct ChurchSettings {
    pub church_id: String,
    pub key: String,
    pub value: String,
}

// ─── CalibrationThresholds ────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct CalibrationThresholds {
    pub id: String,
    pub church_id: String,
    pub stage: String,
    pub accept_above: f64,
    pub escalate_below: f64,
    pub updated_at: String,
}

// ─── ServiceRecord ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct ServiceRecord {
    pub id: String,
    pub sermon_id: String,
    pub total_detections: i64,
    pub auto_accepted: i64,
    pub operator_corrected: i64,
    pub rejected: i64,
    pub avg_confidence: Option<f64>,
    pub avg_processing_time_ms: Option<f64>,
    pub created_at: String,
}
