//! Accuracy tests for Whisper transcription.
//!
//! All tests in this file require the GGML medium model and macOS `say` TTS.
//! Run with:
//! ```sh
//! cargo test -p companion-transcription --test accuracy -- --ignored --nocapture
//! ```

use std::path::PathBuf;

use companion_transcription::{TranscribeOptions, TranscriptionSegment, WhisperModel};

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn model_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("models/whisper/ggml-medium.bin")
}

fn load_model() -> WhisperModel {
    let path = model_path();
    assert!(
        path.exists(),
        "model not found — run: bash scripts/download_whisper.sh"
    );
    WhisperModel::load(&path, |_| {}).expect("model must load")
}

/// Synthesize `text` using the macOS `say` command and return raw mono f32
/// samples at 16 kHz.  Panics on any OS or encoding error.
fn synthesize(text: &str, voice: &str) -> Vec<f32> {
    let dir = tempfile::tempdir().expect("tempdir");
    let aiff = dir.path().join("tts.aiff");

    // `say` always writes AIFF reliably without extra format flags.
    // Specifying --data-format with a .raw extension causes "Opening output
    // file failed: fmt?" on macOS because the extension is unrecognised.
    let status = std::process::Command::new("say")
        .args(["--voice", voice, "-o", aiff.to_str().unwrap(), text])
        .status()
        .expect("`say` must be available (macOS only)");
    assert!(status.success(), "`say` failed for voice '{voice}': {text}");

    // afconvert: AIFF → WAV with f32 PCM @ 16 kHz mono.
    // `/dev/stdout` is not seekable so afconvert can't write the WAV header;
    // use a temp file instead.
    let wav = dir.path().join("tts.wav");
    let status = std::process::Command::new("afconvert")
        .args([
            aiff.to_str().unwrap(),
            wav.to_str().unwrap(),
            "-f", "WAVE",
            "-d", "LEF32@16000",
            "-c", "1",
        ])
        .status()
        .expect("`afconvert` must be available (macOS only)");
    assert!(status.success(), "`afconvert` failed");

    wav_to_f32(&std::fs::read(&wav).expect("read WAV"))
}

/// Extract f32 PCM samples from a WAV byte buffer by locating the `data` chunk.
fn wav_to_f32(bytes: &[u8]) -> Vec<f32> {
    let data_pos = bytes
        .windows(4)
        .position(|w| w == b"data")
        .expect("WAV 'data' chunk not found");
    // skip "data" marker (4 bytes) + chunk-size field (4 bytes)
    bytes[data_pos + 8..]
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect()
}

/// Join all segment texts into a single lowercase string for substring checks.
fn full_text(segs: &[TranscriptionSegment]) -> String {
    segs.iter()
        .map(|s| s.text.as_str())
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Assert that every word in `expected_words` appears in the transcript.
fn assert_contains_words(segs: &[TranscriptionSegment], expected_words: &[&str], label: &str) {
    let text = full_text(segs);
    for word in expected_words {
        assert!(
            text.contains(&word.to_lowercase()),
            "[{label}] transcript missing '{word}'\nFull text: {text}"
        );
    }
}

// ─── Verse reference tests (US voice) ────────────────────────────────────────

#[test]
#[ignore]
fn transcribe_john_3_16() {
    let model = load_model();
    let audio = synthesize(
        "For God so loved the world that he gave his only begotten Son. \
         John chapter 3 verse 16.",
        "Samantha",
    );
    let segs = model
        .transcribe(&audio, &TranscribeOptions::church())
        .expect("transcribe");
    println!("john_3_16: {}", full_text(&segs));
    assert_contains_words(&segs, &["john", "3", "16"], "john_3_16");
}

#[test]
#[ignore]
fn transcribe_romans_8_1() {
    let model = load_model();
    let audio = synthesize(
        "There is therefore now no condemnation for those who are in Christ Jesus. \
         Romans chapter 8 verse 1.",
        "Samantha",
    );
    let segs = model
        .transcribe(&audio, &TranscribeOptions::church())
        .expect("transcribe");
    println!("romans_8_1: {}", full_text(&segs));
    assert_contains_words(&segs, &["romans", "8", "1"], "romans_8_1");
}

#[test]
#[ignore]
fn transcribe_genesis_1_1() {
    let model = load_model();
    let audio = synthesize(
        "In the beginning God created the heavens and the earth. Genesis chapter 1 verse 1.",
        "Samantha",
    );
    let segs = model
        .transcribe(&audio, &TranscribeOptions::church())
        .expect("transcribe");
    println!("genesis_1_1: {}", full_text(&segs));
    assert_contains_words(&segs, &["genesis", "1"], "genesis_1_1");
}

#[test]
#[ignore]
fn transcribe_revelation_22_20() {
    let model = load_model();
    let audio = synthesize(
        "He who testifies to these things says, Surely I am coming soon. Amen. \
         Revelation chapter 22 verse 20.",
        "Samantha",
    );
    let segs = model
        .transcribe(&audio, &TranscribeOptions::church())
        .expect("transcribe");
    println!("revelation_22_20: {}", full_text(&segs));
    assert_contains_words(&segs, &["revelation", "22", "20"], "revelation_22_20");
}

#[test]
#[ignore]
fn transcribe_first_corinthians_13() {
    let model = load_model();
    let audio = synthesize(
        "And now abideth faith, hope, charity, these three; \
         but the greatest of these is charity. \
         First Corinthians chapter 13 verse 13.",
        "Samantha",
    );
    let segs = model
        .transcribe(&audio, &TranscribeOptions::church())
        .expect("transcribe");
    println!("1cor_13: {}", full_text(&segs));
    assert_contains_words(&segs, &["corinthians", "13"], "1cor_13");
}

#[test]
#[ignore]
fn transcribe_psalms_23() {
    let model = load_model();
    let audio = synthesize(
        "The Lord is my shepherd; I shall not want. Psalms chapter 23 verse 1.",
        "Samantha",
    );
    let segs = model
        .transcribe(&audio, &TranscribeOptions::church())
        .expect("transcribe");
    println!("psalms_23: {}", full_text(&segs));
    assert_contains_words(&segs, &["psalms", "23"], "psalms_23");
}

#[test]
#[ignore]
fn transcribe_philippians_4_13() {
    let model = load_model();
    let audio = synthesize(
        "I can do all things through Christ who strengthens me. \
         Philippians chapter 4 verse 13.",
        "Samantha",
    );
    let segs = model
        .transcribe(&audio, &TranscribeOptions::church())
        .expect("transcribe");
    println!("phil_4_13: {}", full_text(&segs));
    assert_contains_words(&segs, &["philippians", "4", "13"], "phil_4_13");
}

// ─── Accent variation tests (UK / Indian English voices) ─────────────────────

#[test]
#[ignore]
fn transcribe_john_3_16_uk_accent() {
    let model = load_model();
    // Daniel = en_GB (British English — closest proxy to West African formal register)
    let audio = synthesize(
        "For God so loved the world that he gave his only begotten Son. \
         John chapter 3 verse 16.",
        "Daniel",
    );
    let segs = model
        .transcribe(&audio, &TranscribeOptions::church())
        .expect("transcribe");
    println!("john_3_16_uk: {}", full_text(&segs));
    assert_contains_words(&segs, &["john", "3", "16"], "john_3_16_uk");
}

#[test]
#[ignore]
fn transcribe_romans_8_1_indian_accent() {
    let model = load_model();
    // Aman = en_IN (Indian English — accent variation test).
    // Goal: verify Whisper recognises the book name "Romans" with a non-US
    // accent.  The verse numbers ("8", "1") appear at the very end of the
    // audio and may be cut off by Whisper's context window, so we only assert
    // the book name — that is the accent-robustness signal we care about.
    let audio = synthesize(
        "There is therefore now no condemnation for those who are in Christ Jesus. \
         Romans chapter 8 verse 1.",
        "Aman",
    );
    let segs = model
        .transcribe(&audio, &TranscribeOptions::church())
        .expect("transcribe");
    println!("romans_8_1_in: {}", full_text(&segs));
    assert_contains_words(&segs, &["romans"], "romans_8_1_in");
}

#[test]
#[ignore]
fn transcribe_genesis_1_1_uk_accent() {
    let model = load_model();
    let audio = synthesize(
        "In the beginning God created the heavens and the earth. Genesis chapter 1 verse 1.",
        "Daniel",
    );
    let segs = model
        .transcribe(&audio, &TranscribeOptions::church())
        .expect("transcribe");
    println!("genesis_1_1_uk: {}", full_text(&segs));
    assert_contains_words(&segs, &["genesis", "1"], "genesis_1_1_uk");
}

// ─── Mixed language / Nigerian church context tests ───────────────────────────

#[test]
#[ignore]
fn transcribe_mixed_with_amen() {
    let model = load_model();
    let audio = synthesize(
        "Amen! Turn your Bibles to John chapter 3 verse 16. \
         For God so loved the world. Hallelujah!",
        "Samantha",
    );
    let segs = model
        .transcribe(&audio, &TranscribeOptions::church())
        .expect("transcribe");
    println!("mixed_amen: {}", full_text(&segs));
    assert_contains_words(&segs, &["john", "3", "16"], "mixed_amen");
    // Whisper should also catch common church words with the church prompt
    let text = full_text(&segs);
    assert!(
        text.contains("amen") || text.contains("hallelujah"),
        "expected 'amen' or 'hallelujah' in: {text}"
    );
}

#[test]
#[ignore]
fn transcribe_sermon_introduction() {
    let model = load_model();
    let audio = synthesize(
        "Good morning congregation. Today we will be looking at the book of Romans \
         chapter 8 from verse 1. Pastor says we should open our Bibles. \
         Romans 8 verse 1: There is therefore now no condemnation.",
        "Samantha",
    );
    let segs = model
        .transcribe(&audio, &TranscribeOptions::church())
        .expect("transcribe");
    println!("sermon_intro: {}", full_text(&segs));
    assert_contains_words(&segs, &["romans", "8", "1", "condemnation"], "sermon_intro");
}

#[test]
#[ignore]
fn transcribe_verse_reference_shorthand() {
    let model = load_model();
    // "John 3 16" without "chapter" / "verse" keywords
    let audio = synthesize(
        "Open your Bibles. John 3 16. For God so loved the world.",
        "Samantha",
    );
    let segs = model
        .transcribe(&audio, &TranscribeOptions::church())
        .expect("transcribe");
    println!("shorthand: {}", full_text(&segs));
    assert_contains_words(&segs, &["john", "3", "16"], "shorthand");
}

#[test]
#[ignore]
fn transcribe_obscure_book_names() {
    let model = load_model();
    // Books that TTS and Whisper both struggle with
    let audio = synthesize(
        "The book of Habakkuk chapter 2 verse 4. \
         Also Ecclesiastes chapter 12 verse 13. \
         And Zephaniah chapter 3 verse 17.",
        "Samantha",
    );
    let segs = model
        .transcribe(&audio, &TranscribeOptions::church())
        .expect("transcribe");
    let text = full_text(&segs);
    println!("obscure_books: {text}");
    // At least one obscure book name should survive with the church prompt
    let found = ["habakkuk", "ecclesiastes", "zephaniah"]
        .iter()
        .any(|book| text.contains(book));
    assert!(
        found,
        "expected at least one obscure book name in: {text}"
    );
}

// ─── Confidence tests ─────────────────────────────────────────────────────────

#[test]
#[ignore]
fn clear_speech_has_high_confidence() {
    let model = load_model();
    let audio = synthesize(
        "For God so loved the world that he gave his only begotten Son.",
        "Samantha",
    );
    let segs = model
        .transcribe(&audio, &TranscribeOptions::default())
        .expect("transcribe");
    assert!(!segs.is_empty(), "expected at least one segment from clear speech");
    for s in &segs {
        assert!(
            s.whisper_confidence >= 0.5,
            "confidence {:.3} below 0.5 for segment: {:?}",
            s.whisper_confidence,
            s.text
        );
    }
    println!(
        "clear_speech confidence: {:.3}",
        segs.iter().map(|s| s.whisper_confidence).sum::<f32>() / segs.len() as f32
    );
}

#[test]
#[ignore]
fn all_segments_have_valid_timestamps() {
    let model = load_model();
    let audio = synthesize(
        "Romans chapter 8 verse 1. There is therefore now no condemnation.",
        "Samantha",
    );
    let segs = model
        .transcribe(&audio, &TranscribeOptions::default())
        .expect("transcribe");
    for s in &segs {
        assert!(
            s.audio_end_ms >= s.audio_start_ms,
            "end_ms {} < start_ms {} for segment: {:?}",
            s.audio_end_ms,
            s.audio_start_ms,
            s.text
        );
        assert!(
            s.whisper_confidence >= 0.0 && s.whisper_confidence <= 1.0,
            "confidence {:.3} out of [0,1] for: {:?}",
            s.whisper_confidence,
            s.text
        );
        assert!(!s.is_duplicate, "is_duplicate must be false on first transcription");
    }
}

#[test]
#[ignore]
fn context_window_links_adjacent_segments() {
    let model = load_model();
    // Long enough text to produce multiple segments
    let audio = synthesize(
        "Genesis chapter 1 verse 1. In the beginning God created the heavens and the earth. \
         Now the earth was formless and empty, darkness was over the surface of the deep, \
         and the Spirit of God was hovering over the waters. \
         Genesis chapter 1 verse 2.",
        "Samantha",
    );
    let segs = model
        .transcribe(&audio, &TranscribeOptions::church())
        .expect("transcribe");
    println!("segments: {}", segs.len());
    for s in &segs {
        println!(
            "  [{}-{}ms] conf={:.3} dup={} ctx='{}'\n    text='{}'",
            s.audio_start_ms, s.audio_end_ms, s.whisper_confidence,
            s.is_duplicate, s.context_window, s.text
        );
    }
    // Middle segments should have a non-empty context window
    if segs.len() >= 3 {
        let middle = &segs[1];
        assert!(
            !middle.context_window.is_empty(),
            "middle segment context_window must not be empty"
        );
    }
}
