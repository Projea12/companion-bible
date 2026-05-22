//! Integration tests: feed known references and non-reference text through the
//! full `DetectionEngine` pipeline (pattern layer only — no AI models needed).

use std::path::PathBuf;

use companion_bible::KjvBible;
use companion_database::{
    connect, CalibrationRepository, ChurchRepository, DetectionEventRepository, PoolConfig,
    VerseRepository,
};
use companion_engine::{DetectionEngine, EngineConfig, ValidationOutcome};
use companion_transcription::TranscriptionSegment;
use tempfile::TempDir;

// ─── helpers ─────────────────────────────────────────────────────────────────

fn bible_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../bible/src/data/kjv.json")
}

fn segment(text: &str) -> TranscriptionSegment {
    TranscriptionSegment {
        text: text.to_string(),
        audio_start_ms: 0,
        audio_end_ms: 3_000,
        whisper_confidence: 0.95,
        is_duplicate: false,
        context_window: String::new(),
    }
}

/// Build a pattern-only engine (no LocalAI, no CloudAI) backed by a temp DB.
async fn setup_engine() -> (DetectionEngine, TempDir) {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = connect(&db_path, &PoolConfig::default()).await.unwrap();

    sqlx::query("INSERT INTO churches (id, name, region) VALUES ('c1', 'Test Church', 'uk')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO sermons (id, church_id, date, started_at) \
         VALUES ('s1', 'c1', '2026-01-01', '2026-01-01T10:00:00Z')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let bible = KjvBible::load(bible_path()).unwrap();
    let engine = DetectionEngine::new(
        EngineConfig {
            sermon_id: "s1".into(),
            api_key: None,
            openai_api_key: None,
        },
        bible,
        ChurchRepository::new(pool.clone()),
        CalibrationRepository::new(pool.clone()),
        DetectionEventRepository::new(pool.clone()),
        VerseRepository::new(pool),
        None,
    )
    .await
    .unwrap();

    (engine, dir)
}

// ─── 20 known references ─────────────────────────────────────────────────────

/// (segment text, expected canonical book, expected chapter, expected verse)
type Case<'a> = (&'a str, &'a str, u8, u8);

const KNOWN_REFERENCES: &[Case<'static>] = &[
    ("for God so loved the world John 3:16", "John", 3, 16),
    (
        "Romans 8:28 all things work together for good",
        "Romans",
        8,
        28,
    ),
    ("Psalm 23:1 the Lord is my shepherd", "Psalms", 23, 1),
    ("in the beginning Genesis 1:1", "Genesis", 1, 1),
    (
        "Blessed are the poor in spirit Matthew 5:3",
        "Matthew",
        5,
        3,
    ),
    ("1 Corinthians 13:4 love is patient", "1 Corinthians", 13, 4),
    ("trust in the Lord Proverbs 3:5", "Proverbs", 3, 5),
    (
        "they shall mount up with wings Isaiah 40:31",
        "Isaiah",
        40,
        31,
    ),
    ("I can do all things Philippians 4:13", "Philippians", 4, 13),
    ("faith is the substance Hebrews 11:1", "Hebrews", 11, 1),
    ("go into all the world Mark 16:15", "Mark", 16, 15),
    ("nothing is impossible Luke 1:37", "Luke", 1, 37),
    ("you shall be my witnesses Acts 1:8", "Acts", 1, 8),
    ("all scripture 2 Timothy 3:16", "2 Timothy", 3, 16),
    ("by grace Ephesians 2:8", "Ephesians", 2, 8),
    ("plans to prosper you Jeremiah 29:11", "Jeremiah", 29, 11),
    ("the fruit of the Spirit Galatians 5:22", "Galatians", 5, 22),
    ("I stand at the door Revelation 3:20", "Revelation", 3, 20),
    (
        "heartily as to the Lord Colossians 3:23",
        "Colossians",
        3,
        23,
    ),
    ("every good gift James 1:17", "James", 1, 17),
];

#[tokio::test]
async fn all_20_known_references_are_detected() {
    let (mut engine, _dir) = setup_engine().await;
    let mut failures: Vec<String> = Vec::new();

    for (text, expected_book, expected_chapter, expected_verse) in KNOWN_REFERENCES {
        let decision = engine.process(segment(text)).await;

        let passed = match &decision.reference {
            Some(r) => {
                r.book == *expected_book
                    && r.chapter == *expected_chapter
                    && r.verse == Some(*expected_verse)
            }
            None => false,
        };

        if !passed {
            failures.push(format!(
                "FAIL: \"{text}\"\n  expected {expected_book} {expected_chapter}:{expected_verse}\n  got {:?}",
                decision.reference
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "Missed references ({}/{}):\n{}",
        failures.len(),
        KNOWN_REFERENCES.len(),
        failures.join("\n")
    );
}

#[tokio::test]
async fn detected_references_pass_kjv_validation() {
    let (mut engine, _dir) = setup_engine().await;

    for (text, _, _, _) in KNOWN_REFERENCES {
        let decision = engine.process(segment(text)).await;
        assert!(
            decision.validation.is_valid(),
            "Expected Valid for \"{text}\", got {:?}",
            decision.validation
        );
    }
}

// ─── false positives ──────────────────────────────────────────────────────────

const NON_VERSE_TEXTS: &[&str] = &[
    "The weather today is beautiful and sunny",
    "Please call extension 316 for further support",
    "Our church has been here for forty years",
    "Meeting every Sunday at nine in the morning",
    "I love reading and learning new things every day",
    "She completed three hundred pages of her dissertation",
    "The fire alarm is located at the end of the hall",
    "We welcomed twenty new members this quarter",
];

#[tokio::test]
async fn no_false_positives_on_non_verse_text() {
    let (mut engine, _dir) = setup_engine().await;
    let mut false_positives: Vec<String> = Vec::new();

    for text in NON_VERSE_TEXTS {
        let decision = engine.process(segment(text)).await;

        if let Some(r) = decision.reference {
            false_positives.push(format!("FALSE POSITIVE: \"{text}\" → {r:?}"));
        }
    }

    assert!(
        false_positives.is_empty(),
        "Unexpected detections on non-verse text:\n{}",
        false_positives.join("\n")
    );
}

#[tokio::test]
async fn false_positive_decisions_have_no_reference_validation() {
    let (mut engine, _dir) = setup_engine().await;

    for text in NON_VERSE_TEXTS {
        let decision = engine.process(segment(text)).await;
        assert_eq!(
            decision.validation,
            ValidationOutcome::NoReference,
            "Expected NoReference for \"{text}\", got {:?}",
            decision.validation
        );
    }
}
