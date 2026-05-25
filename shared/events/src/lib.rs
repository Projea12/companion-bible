use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BibleReference {
    pub book: String,
    pub chapter: u8,
    pub verse: Option<u8>,
    pub verse_end: Option<u8>,
}

impl BibleReference {
    pub fn new(book: impl Into<String>, chapter: u8) -> Self {
        Self {
            book: book.into(),
            chapter,
            verse: None,
            verse_end: None,
        }
    }

    pub fn with_verse(mut self, verse: u8) -> Self {
        self.verse = Some(verse);
        self
    }

    pub fn with_range(mut self, from: u8, to: u8) -> Self {
        self.verse = Some(from);
        self.verse_end = Some(to);
        self
    }
}

impl std::fmt::Display for BibleReference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.book, self.chapter)?;
        match (self.verse, self.verse_end) {
            (Some(v), Some(end)) => write!(f, ":{v}-{end}"),
            (Some(v), None) => write!(f, ":{v}"),
            _ => Ok(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AppEvent {
    // ── Audio ────────────────────────────────────────────────────────────────
    AudioCaptureStarted {
        device_id: String,
    },
    AudioCaptureStopped,
    AudioChunkCaptured {
        chunk_id: u64,
        duration_ms: u32,
    },

    // ── Transcription ────────────────────────────────────────────────────────
    TranscriptionStarted {
        chunk_id: u64,
    },
    TranscriptionCompleted {
        chunk_id: u64,
        text: String,
        duration_ms: u32,
    },
    TranscriptionFailed {
        chunk_id: u64,
        reason: String,
    },

    // ── Detection ────────────────────────────────────────────────────────────
    ScriptureReferenceDetected {
        references: Vec<BibleReference>,
        source_text: String,
    },
    NoReferenceFound {
        source_text: String,
    },

    // ── Bible ────────────────────────────────────────────────────────────────
    VerseLoaded {
        reference: BibleReference,
        text: String,
        translation: String,
    },
    VerseLoadFailed {
        reference: BibleReference,
        reason: String,
    },

    // ── AI ───────────────────────────────────────────────────────────────────
    AiQueryStarted {
        query_id: u64,
    },
    AiResponseReceived {
        query_id: u64,
        response: String,
    },
    AiQueryFailed {
        query_id: u64,
        reason: String,
    },

    // ── Display ──────────────────────────────────────────────────────────────
    VerseDisplayed {
        reference: BibleReference,
    },
    DisplayCleared,
    SermonTitleShown {
        title: String,
    },
    SubPointShown {
        text: String,
    },
    DisplayBlanked,

    // ── Connectivity ─────────────────────────────────────────────────────────
    InternetConnected,
    InternetDisconnected,

    // ── Screen ───────────────────────────────────────────────────────────────
    SecondaryScreenConnected,
    SecondaryScreenDisconnected,
    /// Congregation and operator windows are on the wrong screens.
    ScreenSwapDetected,
    /// Screen configuration has been corrected (swap fixed or reconnected).
    ScreenRestored,

    // ── System ───────────────────────────────────────────────────────────────
    AppStarted {
        version: String,
    },
    AppShutdown,
    UpdateAvailable {
        version: String,
        release_notes: Option<String>,
    },
    UpdateDownloaded {
        version: String,
    },
    UpdateInstalled {
        version: String,
    },
    OnboardingCompleted,

    // ── Watchdog ─────────────────────────────────────────────────────────────
    HealthCheckPassed {
        component: String,
    },
    HealthCheckFailed {
        component: String,
        reason: String,
    },
    ProcessRestarted {
        component: String,
        restart_count: u32,
    },

    // ── Database ─────────────────────────────────────────────────────────────
    DatabaseReady,
    DatabaseMigrated {
        from_version: u32,
        to_version: u32,
    },

    // ── Config ───────────────────────────────────────────────────────────────
    ConfigLoaded,
    ConfigUpdated {
        key: String,
    },

    // ── Audio quality ────────────────────────────────────────────────────────
    AudioQualityDegraded,

    // ── AI layers ────────────────────────────────────────────────────────────
    AiLayersChanged {
        layers: String,
    },

    // ── Storage ──────────────────────────────────────────────────────────────
    StorageStatus {
        level: String,
        available_bytes: u64,
    },

    // ── Sermon lifecycle ─────────────────────────────────────────────────────
    SermonStarted {
        title: Option<String>,
        pastor: Option<String>,
        anchor_scripture: Option<String>,
    },
    SermonEnded {
        summary: Option<String>,
    },
    SubPointAdded {
        text: String,
        index: u32,
    },

    // ── Operator ─────────────────────────────────────────────────────────────
    OperatorManualOverride {
        reference: String,
    },

    // ── Hymns ────────────────────────────────────────────────────────────────
    /// A hymn number was detected in transcription — load and show stanza 1.
    HymnDetected {
        number: u16,
        title: String,
    },
    /// The current section advanced (auto via last-line match or manual).
    HymnSectionAdvanced {
        number: u16,
        /// 0-based index into the playback sequence.
        section_index: usize,
        /// 1-based stanza number; `None` when this section is a chorus.
        stanza_number: Option<u16>,
        is_chorus: bool,
        lines: Vec<String>,
    },
    /// All sections have been displayed — hymn is complete.
    HymnCompleted {
        number: u16,
    },

    // ── Announcements ────────────────────────────────────────────────────────
    /// A slide in the pre-service announcement loop became active.
    AnnouncementShown {
        id: u32,
        body: String,
        /// 0-based position in the list.
        index: u32,
        total: u32,
        duration_secs: u32,
    },
    /// Announcement playback was stopped by the operator.
    AnnouncementsStopped,

    // ── Congregation scroll ───────────────────────────────────────────────────
    /// Operator requested a scroll on the active congregation panel.
    CongregationScroll {
        amount: i32,
    },

    // ── Model setup ──────────────────────────────────────────────────────────
    /// First launch: model weights are not present — setup is required.
    ModelSetupRequired,
    /// Download started; `bytes_total` is `None` when the server omits Content-Length.
    ModelDownloadStarted {
        bytes_total: Option<u64>,
    },
    /// Periodic progress during download.
    ModelDownloadProgress {
        bytes_done: u64,
        bytes_total: Option<u64>,
    },
    /// Download finished; checksum is being verified.
    ModelVerifying,
    /// Model weights are on disk and verified; loading into memory.
    ModelLoadStarted,
    /// Model is loaded and the health check passed.
    ModelReady {
        load_time_ms: u64,
        memory_mb: u64,
    },
    /// Any step in the setup flow failed.
    ModelSetupFailed {
        reason: String,
    },
}

impl AppEvent {
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(event: &AppEvent) -> AppEvent {
        let json = event.to_json().expect("serialize failed");
        let restored = AppEvent::from_json(&json).expect("deserialize failed");
        assert_eq!(event, &restored, "round-trip mismatch for: {json}");
        restored
    }

    fn assert_type_tag(event: &AppEvent, expected_type: &str) {
        let json = event.to_json().unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["type"], expected_type, "wrong type tag in: {json}");
    }

    // ── Audio ─────────────────────────────────────────────────────────────────

    #[test]
    fn audio_capture_started_round_trip() {
        let e = AppEvent::AudioCaptureStarted {
            device_id: "default".into(),
        };
        round_trip(&e);
        assert_type_tag(&e, "AUDIO_CAPTURE_STARTED");
    }

    #[test]
    fn audio_capture_stopped_round_trip() {
        let e = AppEvent::AudioCaptureStopped;
        round_trip(&e);
        assert_type_tag(&e, "AUDIO_CAPTURE_STOPPED");
    }

    #[test]
    fn audio_chunk_captured_round_trip() {
        let e = AppEvent::AudioChunkCaptured {
            chunk_id: 42,
            duration_ms: 3000,
        };
        round_trip(&e);
        assert_type_tag(&e, "AUDIO_CHUNK_CAPTURED");
    }

    // ── Transcription ─────────────────────────────────────────────────────────

    #[test]
    fn transcription_started_round_trip() {
        round_trip(&AppEvent::TranscriptionStarted { chunk_id: 1 });
    }

    #[test]
    fn transcription_completed_round_trip() {
        let e = AppEvent::TranscriptionCompleted {
            chunk_id: 1,
            text: "John 3:16".into(),
            duration_ms: 450,
        };
        round_trip(&e);
        assert_type_tag(&e, "TRANSCRIPTION_COMPLETED");
    }

    #[test]
    fn transcription_failed_round_trip() {
        round_trip(&AppEvent::TranscriptionFailed {
            chunk_id: 7,
            reason: "model timeout".into(),
        });
    }

    // ── Detection ─────────────────────────────────────────────────────────────

    #[test]
    fn scripture_reference_detected_round_trip() {
        let refs = vec![
            BibleReference::new("John", 3).with_verse(16),
            BibleReference::new("Romans", 8).with_range(1, 4),
        ];
        let e = AppEvent::ScriptureReferenceDetected {
            references: refs,
            source_text: "John 3:16 and Romans 8:1-4".into(),
        };
        round_trip(&e);
        assert_type_tag(&e, "SCRIPTURE_REFERENCE_DETECTED");
    }

    #[test]
    fn no_reference_found_round_trip() {
        round_trip(&AppEvent::NoReferenceFound {
            source_text: "hello world".into(),
        });
    }

    // ── Bible ─────────────────────────────────────────────────────────────────

    #[test]
    fn verse_loaded_round_trip() {
        let e = AppEvent::VerseLoaded {
            reference: BibleReference::new("John", 3).with_verse(16),
            text: "For God so loved the world…".into(),
            translation: "ESV".into(),
        };
        round_trip(&e);
        assert_type_tag(&e, "VERSE_LOADED");
    }

    #[test]
    fn verse_load_failed_round_trip() {
        round_trip(&AppEvent::VerseLoadFailed {
            reference: BibleReference::new("Esdras", 99),
            reason: "book not found".into(),
        });
    }

    // ── AI ────────────────────────────────────────────────────────────────────

    #[test]
    fn ai_query_started_round_trip() {
        round_trip(&AppEvent::AiQueryStarted { query_id: 1 });
    }

    #[test]
    fn ai_response_received_round_trip() {
        let e = AppEvent::AiResponseReceived {
            query_id: 1,
            response: "This verse speaks of God's love…".into(),
        };
        round_trip(&e);
        assert_type_tag(&e, "AI_RESPONSE_RECEIVED");
    }

    #[test]
    fn ai_query_failed_round_trip() {
        round_trip(&AppEvent::AiQueryFailed {
            query_id: 2,
            reason: "context limit".into(),
        });
    }

    // ── Display ───────────────────────────────────────────────────────────────

    #[test]
    fn verse_displayed_round_trip() {
        round_trip(&AppEvent::VerseDisplayed {
            reference: BibleReference::new("Psalm", 23).with_verse(1),
        });
    }

    #[test]
    fn display_cleared_round_trip() {
        round_trip(&AppEvent::DisplayCleared);
        assert_type_tag(&AppEvent::DisplayCleared, "DISPLAY_CLEARED");
    }

    // ── System ────────────────────────────────────────────────────────────────

    #[test]
    fn app_started_round_trip() {
        round_trip(&AppEvent::AppStarted {
            version: "0.1.0".into(),
        });
    }

    #[test]
    fn app_shutdown_round_trip() {
        round_trip(&AppEvent::AppShutdown);
    }

    #[test]
    fn update_available_with_notes_round_trip() {
        round_trip(&AppEvent::UpdateAvailable {
            version: "0.2.0".into(),
            release_notes: Some("Bug fixes".into()),
        });
    }

    #[test]
    fn update_available_no_notes_round_trip() {
        round_trip(&AppEvent::UpdateAvailable {
            version: "0.2.0".into(),
            release_notes: None,
        });
    }

    #[test]
    fn update_downloaded_round_trip() {
        round_trip(&AppEvent::UpdateDownloaded {
            version: "0.2.0".into(),
        });
    }

    #[test]
    fn update_installed_round_trip() {
        round_trip(&AppEvent::UpdateInstalled {
            version: "0.2.0".into(),
        });
    }

    #[test]
    fn onboarding_completed_round_trip() {
        round_trip(&AppEvent::OnboardingCompleted);
    }

    // ── Watchdog ──────────────────────────────────────────────────────────────

    #[test]
    fn health_check_passed_round_trip() {
        round_trip(&AppEvent::HealthCheckPassed {
            component: "audio".into(),
        });
    }

    #[test]
    fn health_check_failed_round_trip() {
        round_trip(&AppEvent::HealthCheckFailed {
            component: "transcription".into(),
            reason: "process unresponsive".into(),
        });
    }

    #[test]
    fn process_restarted_round_trip() {
        round_trip(&AppEvent::ProcessRestarted {
            component: "transcription".into(),
            restart_count: 3,
        });
    }

    // ── Database ──────────────────────────────────────────────────────────────

    #[test]
    fn database_ready_round_trip() {
        round_trip(&AppEvent::DatabaseReady);
    }

    #[test]
    fn database_migrated_round_trip() {
        round_trip(&AppEvent::DatabaseMigrated {
            from_version: 1,
            to_version: 2,
        });
    }

    // ── Config ────────────────────────────────────────────────────────────────

    #[test]
    fn config_loaded_round_trip() {
        round_trip(&AppEvent::ConfigLoaded);
    }

    #[test]
    fn config_updated_round_trip() {
        round_trip(&AppEvent::ConfigUpdated {
            key: "audio.device_id".into(),
        });
    }

    // ── BibleReference helpers ────────────────────────────────────────────────

    #[test]
    fn bible_reference_display() {
        assert_eq!(
            BibleReference::new("John", 3).with_verse(16).to_string(),
            "John 3:16"
        );
        assert_eq!(
            BibleReference::new("Romans", 8)
                .with_range(1, 4)
                .to_string(),
            "Romans 8:1-4"
        );
        assert_eq!(BibleReference::new("Genesis", 1).to_string(), "Genesis 1");
    }

    #[test]
    fn bible_reference_round_trip() {
        let r = BibleReference::new("Revelation", 22).with_range(20, 21);
        let json = serde_json::to_string(&r).unwrap();
        let restored: BibleReference = serde_json::from_str(&json).unwrap();
        assert_eq!(r, restored);
    }

    #[test]
    fn json_type_field_is_screaming_snake_case() {
        let cases = [
            (AppEvent::AudioCaptureStopped, "AUDIO_CAPTURE_STOPPED"),
            (AppEvent::DatabaseReady, "DATABASE_READY"),
            (AppEvent::DisplayCleared, "DISPLAY_CLEARED"),
            (AppEvent::OnboardingCompleted, "ONBOARDING_COMPLETED"),
            (AppEvent::AppShutdown, "APP_SHUTDOWN"),
            (AppEvent::ConfigLoaded, "CONFIG_LOADED"),
        ];
        for (event, expected) in cases {
            assert_type_tag(&event, expected);
        }
    }
}
