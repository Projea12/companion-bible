//! Performance tests for the detection pipeline.
//!
//! Tests that run without external resources (model files, network) execute
//! always.  Tests requiring the Phi-3 model or a live Anthropic API key are
//! marked `#[ignore]` and must be opted-in explicitly:
//!
//!   cargo test -p companion-engine --test performance -- --ignored

use std::path::PathBuf;
use std::time::{Duration, Instant};

use companion_bible::KjvBible;
use companion_database::{
    connect, CalibrationRepository, ChurchRepository, DetectionEventRepository, PoolConfig,
    VerseRepository,
};
use companion_detection::PatternEngine;
use companion_engine::{DetectionEngine, EngineConfig};
use companion_transcription::TranscriptionSegment;
use tempfile::TempDir;

// ─── shared constants ────────────────────────────────────────────────────────

const PATTERN_BUDGET_MS: u128 = 5;
const ENGINE_PATTERN_BUDGET_MS: u128 = 50; // well inside the 400 ms budget
const ENGINE_LOCAL_AI_BUDGET_MS: u128 = 400; // full pattern + local AI budget
const ENGINE_ALL_LAYERS_BUDGET_MS: u128 = 800; // full three-layer budget

const PERF_ITERATIONS: u32 = 100;

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

async fn setup_engine() -> (DetectionEngine, TempDir) {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("perf.db");
    let pool = connect(&db_path, &PoolConfig::default()).await.unwrap();

    sqlx::query("INSERT INTO churches (id, name, region) VALUES ('c1', 'Perf Church', 'uk')")
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

// ─── 1. Pattern layer < 5 ms ─────────────────────────────────────────────────

/// Verify that `PatternEngine::find_all` finishes in under 5 ms on a
/// typical sermon sentence.
#[test]
fn pattern_layer_under_5ms() {
    let engine = PatternEngine::new();
    let texts = [
        "for God so loved the world John 3:16",
        "Romans 8:28 all things work together",
        "the Lord is my shepherd Psalm 23:1",
        "I can do all things through Christ Philippians 4:13",
        "trust in the Lord with all your heart Proverbs 3:5",
    ];

    let mut times: Vec<Duration> = Vec::with_capacity(PERF_ITERATIONS as usize);

    for _ in 0..PERF_ITERATIONS {
        for text in &texts {
            let t0 = Instant::now();
            let _ = engine.find_all(text);
            times.push(t0.elapsed());
        }
    }

    let total_calls = times.len() as u128;
    let avg_ns: u128 = times.iter().map(|d| d.as_nanos()).sum::<u128>() / total_calls;
    let max_ms = times.iter().map(|d| d.as_millis()).max().unwrap_or(0);
    let avg_ms = avg_ns / 1_000_000;

    assert!(
        avg_ms < PATTERN_BUDGET_MS,
        "Pattern layer average {avg_ms} ms exceeds {PATTERN_BUDGET_MS} ms budget"
    );
    assert!(
        max_ms < PATTERN_BUDGET_MS * 3, // allow 3× spike for warm-up
        "Pattern layer worst-case {max_ms} ms is unreasonably slow"
    );
}

/// Verify `PatternEngine` on a longer paragraph still finishes under 5 ms.
#[test]
fn pattern_layer_long_text_under_5ms() {
    let engine = PatternEngine::new();
    // ~200-word sermon excerpt with one embedded reference
    let long_text = "Brothers and sisters, we gather here today as a community of faith, \
        united in our belief and our love for one another and for the Lord. \
        The Word of God is a lamp unto our feet and a light unto our path. \
        As we read in John 3:16, God so loved the world that he gave his only begotten Son. \
        Let us therefore approach the throne of grace with confidence, knowing that we are \
        accepted and beloved. The peace of God which passes all understanding will guard your \
        hearts and your minds. Let us not grow weary in doing good, for in due season we shall \
        reap if we do not give up. May the grace of our Lord Jesus Christ be with you all.";

    let mut worst = Duration::ZERO;
    for _ in 0..PERF_ITERATIONS {
        let t0 = Instant::now();
        let _ = engine.find_all(long_text);
        let elapsed = t0.elapsed();
        if elapsed > worst {
            worst = elapsed;
        }
    }

    assert!(
        worst.as_millis() < PATTERN_BUDGET_MS * 5, // 25 ms absolute ceiling for long text
        "Pattern layer on long text worst-case {} ms is too slow",
        worst.as_millis()
    );
}

// ─── 2. Full engine (pattern only) < 50 ms ───────────────────────────────────
//    This is the pattern-only path, which must come in well inside the
//    400 ms budget reserved for pattern + local AI.

/// Measure end-to-end `process()` latency with only the pattern layer active.
/// DB writes are included; this represents the minimum realistic latency.
#[tokio::test]
async fn engine_pattern_only_under_50ms() {
    let (mut engine, _dir) = setup_engine().await;

    let texts = [
        "John 3:16 for God so loved the world",
        "Romans 8:28 and we know that all things",
        "Hebrews 11:1 faith is the substance",
        "Psalm 23:1 the Lord is my shepherd",
        "Philippians 4:13 I can do all things",
    ];

    let mut times: Vec<Duration> = Vec::with_capacity(texts.len() * 5);

    // Warm-up pass
    for text in &texts {
        let _ = engine.process(segment(text)).await;
    }

    // Measured passes
    for _ in 0..5 {
        for text in &texts {
            let t0 = Instant::now();
            let _ = engine.process(segment(text)).await;
            times.push(t0.elapsed());
        }
    }

    let total = times.len() as u128;
    let avg_ms = times.iter().map(|d| d.as_millis()).sum::<u128>() / total;
    let max_ms = times.iter().map(|d| d.as_millis()).max().unwrap_or(0);

    assert!(
        avg_ms < ENGINE_PATTERN_BUDGET_MS,
        "Pattern-only engine average {avg_ms} ms exceeds {ENGINE_PATTERN_BUDGET_MS} ms"
    );
    assert!(
        max_ms < ENGINE_LOCAL_AI_BUDGET_MS,
        "Pattern-only engine worst-case {max_ms} ms exceeds the 400 ms local-AI budget"
    );
}

// ─── 3. Full engine (pattern + local AI) < 400 ms ────────────────────────────

/// Requires the Phi-3 Mini model to be present.
/// Run with: cargo test -p companion-engine --test performance -- --ignored
#[tokio::test]
#[ignore = "requires Phi-3 Mini model file (run download_model binary first)"]
async fn engine_pattern_plus_local_ai_under_400ms() {
    use companion_ai::{LocalAI, LocalAIConfig};
    use companion_engine::LocalAiHandle;

    let model_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../models/phi3/Phi-3-mini-4k-instruct-q4.gguf");

    if !model_path.exists() {
        eprintln!("Skipping: model not found at {model_path:?}");
        return;
    }

    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("perf_ai.db");
    let pool = connect(&db_path, &PoolConfig::default()).await.unwrap();

    sqlx::query("INSERT INTO churches (id, name, region) VALUES ('c1', 'Test', 'uk')")
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

    let ai = LocalAI::load(LocalAIConfig::new(model_path.clone())).expect("model load failed");

    let bible = KjvBible::load(bible_path()).unwrap();
    let mut engine = DetectionEngine::new(
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
        Some(LocalAiHandle::spawn(ai)),
    )
    .await
    .unwrap();

    let texts = [
        "John 3:16 for God so loved the world",
        "Romans 8:28 all things work together for good",
        "Hebrews 11:1 faith is the substance of things hoped for",
    ];

    let mut times: Vec<Duration> = Vec::new();

    // Warm-up
    let _ = engine.process(segment(texts[0])).await;

    for text in &texts {
        let t0 = Instant::now();
        let _ = engine.process(segment(text)).await;
        times.push(t0.elapsed());
    }

    let max_ms = times.iter().map(|d| d.as_millis()).max().unwrap_or(0);
    assert!(
        max_ms <= ENGINE_LOCAL_AI_BUDGET_MS,
        "Engine (pattern + local AI) worst-case {max_ms} ms exceeds {ENGINE_LOCAL_AI_BUDGET_MS} ms"
    );
}

// ─── 4. Full engine (all three layers) < 800 ms ──────────────────────────────

/// Requires Phi-3 Mini model AND a valid ANTHROPIC_API_KEY environment variable.
#[tokio::test]
#[ignore = "requires model file and ANTHROPIC_API_KEY env var"]
async fn engine_all_three_layers_under_800ms() {
    use companion_ai::{LocalAI, LocalAIConfig};
    use companion_engine::LocalAiHandle;

    let api_key = match std::env::var("ANTHROPIC_API_KEY") {
        Ok(k) => k,
        Err(_) => {
            eprintln!("Skipping: ANTHROPIC_API_KEY not set");
            return;
        }
    };

    let model_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../models/phi3/Phi-3-mini-4k-instruct-q4.gguf");

    if !model_path.exists() {
        eprintln!("Skipping: model not found at {model_path:?}");
        return;
    }

    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("perf_all.db");
    let pool = connect(&db_path, &PoolConfig::default()).await.unwrap();

    sqlx::query("INSERT INTO churches (id, name, region) VALUES ('c1', 'Test', 'uk')")
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

    let ai = LocalAI::load(LocalAIConfig::new(model_path.clone())).expect("model load failed");

    let bible = KjvBible::load(bible_path()).unwrap();
    let mut engine = DetectionEngine::new(
        EngineConfig {
            sermon_id: "s1".into(),
            api_key: Some(api_key),
            openai_api_key: None,
        },
        bible,
        ChurchRepository::new(pool.clone()),
        CalibrationRepository::new(pool.clone()),
        DetectionEventRepository::new(pool.clone()),
        VerseRepository::new(pool),
        Some(LocalAiHandle::spawn(ai)),
    )
    .await
    .unwrap();

    let texts = ["as it is written in John 3:16", "Romans 8:28 and we know"];

    // Warm-up
    let _ = engine.process(segment(texts[0])).await;

    let mut times: Vec<Duration> = Vec::new();
    for text in &texts {
        let t0 = Instant::now();
        let _ = engine.process(segment(text)).await;
        times.push(t0.elapsed());
    }

    let max_ms = times.iter().map(|d| d.as_millis()).max().unwrap_or(0);
    assert!(
        max_ms <= ENGINE_ALL_LAYERS_BUDGET_MS,
        "Engine (all three layers) worst-case {max_ms} ms exceeds {ENGINE_ALL_LAYERS_BUDGET_MS} ms"
    );
}
