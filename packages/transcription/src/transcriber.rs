use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use companion_audio::{SlidingWindow, SAMPLE_RATE};

use crate::channel::{segment_channel, SegmentReceiver, SegmentSender};
use crate::model::WhisperModel;
use crate::transcript::{TranscribeOptions, TranscriptionSegment};

// ─── Constants ────────────────────────────────────────────────────────────────

/// Minimum new audio required before triggering a Whisper run.
pub const NEW_AUDIO_SECS: u64 = 5;

/// Audio span sent to Whisper each run — includes overlap for context.
pub const TRANSCRIBE_WINDOW_SECS: u64 = 15;

/// How long to remember emitted text to filter text-based duplicates.
const DEDUP_MEMORY_SECS: u64 = 30;

/// Polling interval for the transcription loop.
const POLL_MS: u64 = 100;

/// Samples consumed per run (used for absolute timestamp tracking).
const SAMPLES_PER_RUN: u64 = NEW_AUDIO_SECS * SAMPLE_RATE as u64;

// ─── EmittedSet ───────────────────────────────────────────────────────────────

/// Text-based deduplication memory for the overlap zone.
///
/// Stores `(when_emitted, normalised_text)`.  Used as a fallback when
/// timestamp-based dedup cannot definitively rule a segment out — for example,
/// when a segment straddles the old/new audio boundary.
pub(crate) struct EmittedSet {
    entries: VecDeque<(Instant, String)>,
    prune_after: Duration,
}

impl EmittedSet {
    pub(crate) fn new(prune_after: Duration) -> Self {
        Self { entries: VecDeque::new(), prune_after }
    }

    /// Remove entries older than `prune_after`.
    pub(crate) fn prune(&mut self) {
        let cutoff = Instant::now() - self.prune_after;
        while self.entries.front().map(|(t, _)| *t < cutoff).unwrap_or(false) {
            self.entries.pop_front();
        }
    }

    /// `true` if `text` (after normalisation) has already been emitted.
    pub(crate) fn contains(&self, text: &str) -> bool {
        let key = normalize(text);
        self.entries.iter().any(|(_, t)| t == &key)
    }

    /// Record `text` as emitted and prune stale entries.
    pub(crate) fn insert(&mut self, text: String) {
        self.prune();
        self.entries.push_back((Instant::now(), normalize(&text)));
    }
}

// ─── Text normalisation ───────────────────────────────────────────────────────

/// Normalise segment text for duplicate detection: lowercase + collapse spaces.
pub(crate) fn normalize(text: &str) -> String {
    text.split_whitespace()
        .map(|w| w.to_lowercase())
        .collect::<Vec<_>>()
        .join(" ")
}

// ─── WhisperTranscriber ───────────────────────────────────────────────────────

/// Drives the sliding-window transcription loop.
///
/// ## Data flow
/// ```text
/// SlidingWindow (30 s audio)
///     │  last 15 s every 5 s
///     ▼
/// build_prompt(book_context)   ← updated by scripture-detection layer
///     │
/// WhisperModel::transcribe
///     │  Vec<TranscriptionSegment>
///     ▼
/// Deduplication (timestamp-primary, text-fallback)
///     │  only new segments
///     ▼
/// mpsc::SyncSender  →  downstream scripture-detection channel
/// ```
///
/// ## Deduplication strategy
///
/// Each Whisper run covers a 15-second window that overlaps 10 seconds with
/// the previous run.  Deduplication uses two complementary checks:
///
/// 1. **Timestamp-primary**: a segment whose absolute audio start is earlier
///    than the absolute end of the last emitted segment is in the overlap zone
///    and is checked against the text memory.
/// 2. **Text-fallback**: if the segment's text was already emitted (regardless
///    of timestamp), it is a duplicate.  This covers the case where a segment
///    straddles the exact boundary.
///
/// Segments that are new by both checks are emitted and added to the text
/// memory.
///
/// ## Book context
///
/// The `book_context` handle is shared with the scripture-detection layer.
/// Whenever the detector identifies the current book, it writes to the handle
/// and every subsequent Whisper run gets a prompt that specifically names the
/// book, improving accuracy for bare references like "chapter 3 verse 16".
///
/// ## Usage
/// ```rust,ignore
/// let (mut transcriber, rx) = WhisperTranscriber::new(window, TranscribeOptions::church());
/// let book_ctx = transcriber.book_context_handle();
/// transcriber.start(model);
///
/// // Later, when detection identifies "Romans":
/// *book_ctx.lock().unwrap() = Some("Romans".into());
///
/// for batch in rx {
///     // `batch` contains only non-duplicate segments
/// }
/// ```
pub struct WhisperTranscriber {
    window: Arc<Mutex<SlidingWindow>>,
    options: TranscribeOptions,
    book_context: Arc<Mutex<Option<String>>>,
    stop_flag: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
    sender: SegmentSender,
}

impl WhisperTranscriber {
    /// Create a transcriber and return the receiver for new-segment batches.
    ///
    /// Uses a bounded ring-buffer channel (capacity [`CHANNEL_CAPACITY`]).  A
    /// slow consumer causes the oldest batches to be dropped rather than
    /// blocking the transcription thread.
    pub fn new(
        window: Arc<Mutex<SlidingWindow>>,
        options: TranscribeOptions,
    ) -> (Self, SegmentReceiver) {
        let (sender, receiver) = segment_channel();
        let stop_flag = Arc::new(AtomicBool::new(true));
        let book_context = Arc::new(Mutex::new(None::<String>));
        (Self { window, options, book_context, stop_flag, handle: None, sender }, receiver)
    }

    // ── Book context ──────────────────────────────────────────────────────────

    /// Shared handle to the current book context.
    ///
    /// Write `Some("Romans")` to improve Whisper's accuracy for the current
    /// passage; write `None` to revert to the generic prompt.
    pub fn book_context_handle(&self) -> Arc<Mutex<Option<String>>> {
        Arc::clone(&self.book_context)
    }

    /// Convenience wrapper: set the current book context.
    pub fn set_book_context(&self, book: Option<String>) {
        if let Ok(mut g) = self.book_context.lock() {
            *g = book;
        }
    }

    // ── Lifecycle ─────────────────────────────────────────────────────────────

    /// Start the transcription loop, taking ownership of `model`.
    ///
    /// If already running, the loop is stopped first and restarted with the
    /// new model.
    pub fn start(&mut self, model: WhisperModel) {
        self.stop_join();

        self.stop_flag.store(false, Ordering::Release);

        let window = Arc::clone(&self.window);
        let options = self.options.clone();
        let book_context = Arc::clone(&self.book_context);
        let stop_flag = Arc::clone(&self.stop_flag);
        let sender = self.sender.clone();

        self.handle = Some(
            std::thread::Builder::new()
                .name("whisper-transcriber".into())
                .spawn(move || {
                    transcription_loop(model, window, options, book_context, stop_flag, sender);
                })
                .expect("failed to spawn whisper-transcriber thread"),
        );
    }

    /// Stop the transcription loop and wait for the thread to exit.
    pub fn stop(&mut self) {
        self.stop_join();
    }

    /// `true` while the transcription thread is running.
    pub fn is_running(&self) -> bool {
        self.handle.is_some()
    }

    // ── Internal ──────────────────────────────────────────────────────────────

    fn stop_join(&mut self) {
        self.stop_flag.store(true, Ordering::Release);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for WhisperTranscriber {
    fn drop(&mut self) {
        self.stop_flag.store(true, Ordering::Release);
        self.handle.take();
    }
}

// ─── Transcription loop ───────────────────────────────────────────────────────

fn transcription_loop(
    model: WhisperModel,
    window: Arc<Mutex<SlidingWindow>>,
    options: TranscribeOptions,
    book_context: Arc<Mutex<Option<String>>>,
    stop_flag: Arc<AtomicBool>,
    sender: SegmentSender,
) {
    let new_audio_dur = Duration::from_secs(NEW_AUDIO_SECS);
    let window_dur = Duration::from_secs(TRANSCRIBE_WINDOW_SECS);
    let poll = Duration::from_millis(POLL_MS);

    let mut emitted = EmittedSet::new(Duration::from_secs(DEDUP_MEMORY_SECS));
    let mut last_run = Instant::now() - new_audio_dur; // trigger immediately on first poll

    // ── Absolute audio timeline ───────────────────────────────────────────────
    // `samples_trimmed` counts total samples removed from the window front.
    // Dividing by SAMPLE_RATE converts to seconds, giving the absolute start
    // time of the current Whisper window in the audio timeline.
    let mut samples_trimmed: u64 = 0;
    // Absolute end (ms) of the last segment we emitted.  Segments that start
    // before this mark are in the overlap zone and undergo text-based dedup.
    let mut last_emitted_abs_end_ms: u64 = 0;

    while !stop_flag.load(Ordering::Acquire) {
        std::thread::sleep(poll);

        // Throttle: require at least NEW_AUDIO_SECS of real time since last run.
        if last_run.elapsed() < new_audio_dur {
            continue;
        }

        // Snapshot the last TRANSCRIBE_WINDOW_SECS of clean audio.
        let audio = match window.lock() {
            Ok(w) if !w.is_empty() => w.last(window_dur).samples,
            _ => continue,
        };

        // Build a prompt with the current book context (read without holding lock).
        let current_book = book_context.lock().ok().and_then(|g| g.clone());
        let mut run_opts = options.clone();
        run_opts.initial_prompt = TranscribeOptions::build_prompt(current_book.as_deref());

        // Run Whisper.  Blocking; may take several seconds on CPU.
        let raw = match model.transcribe(&audio, &run_opts) {
            Ok(segs) => segs,
            Err(_) => {
                last_run = Instant::now();
                continue;
            }
        };

        // Absolute start of this Whisper window in the audio timeline (ms).
        let window_start_abs_ms = (samples_trimmed / SAMPLE_RATE as u64) * 1_000;

        // ── Deduplication ─────────────────────────────────────────────────────
        let mut new_segs: Vec<TranscriptionSegment> = Vec::new();
        for mut seg in raw {
            let abs_start = window_start_abs_ms + seg.audio_start_ms;
            let abs_end = window_start_abs_ms + seg.audio_end_ms;

            // Primary: is this segment entirely within the overlap zone?
            let in_overlap = abs_start < last_emitted_abs_end_ms;

            if in_overlap && emitted.contains(&seg.text) {
                // Overlap zone AND text already seen — definite duplicate.
                seg.is_duplicate = true;
            } else {
                // New zone, OR new text in the overlap zone (partial-overlap case).
                emitted.insert(seg.text.clone());
                last_emitted_abs_end_ms = last_emitted_abs_end_ms.max(abs_end);
                new_segs.push(seg);
            }
        }

        // Emit batch (silently drop if receiver has disconnected).
        if !new_segs.is_empty() {
            let _ = sender.send(new_segs);
        }

        // Advance the window: trim the oldest NEW_AUDIO_SECS and record it.
        if let Ok(mut w) = window.lock() {
            w.trim_front(new_audio_dur);
        }
        samples_trimmed += SAMPLES_PER_RUN;

        last_run = Instant::now();
    }
}
