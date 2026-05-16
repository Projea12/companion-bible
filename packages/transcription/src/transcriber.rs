use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use companion_audio::SlidingWindow;

use crate::model::WhisperModel;
use crate::transcript::{TranscribeOptions, TranscriptionSegment};

// ─── Constants ────────────────────────────────────────────────────────────────

/// Minimum new audio required before triggering a Whisper run.
pub const NEW_AUDIO_SECS: u64 = 5;

/// Audio span sent to Whisper each run — includes overlap for context.
pub const TRANSCRIBE_WINDOW_SECS: u64 = 15;

/// How long to remember emitted text to filter overlapping-window duplicates.
const DEDUP_MEMORY_SECS: u64 = 30;

/// Polling interval for the transcription loop.
const POLL_MS: u64 = 100;

// ─── EmittedSet ───────────────────────────────────────────────────────────────

/// Tracks recently emitted segment text to deduplicate overlapping windows.
///
/// Each entry stores (when_emitted, normalized_text).  Old entries are pruned
/// lazily on every [`insert`] call.
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
/// WhisperModel::transcribe
///     │  Vec<TranscriptionSegment>
///     ▼
/// EmittedSet deduplication
///     │  only new segments
///     ▼
/// mpsc::Sender  →  downstream scripture-detection channel
/// ```
///
/// ## Usage
/// ```rust,ignore
/// let (transcriber, rx) = WhisperTranscriber::new(window, TranscribeOptions::church());
/// transcriber.start(model);         // hands model ownership to background thread
/// for batch in rx {
///     // `batch` contains only non-duplicate segments
/// }
/// ```
pub struct WhisperTranscriber {
    window: Arc<Mutex<SlidingWindow>>,
    options: TranscribeOptions,
    stop_flag: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
    sender: mpsc::SyncSender<Vec<TranscriptionSegment>>,
}

impl WhisperTranscriber {
    /// Create a transcriber and return the receiver for new-segment batches.
    ///
    /// The internal channel is bounded to 32 batches; a slow consumer causes
    /// the transcription thread to block rather than accumulate unbounded memory.
    pub fn new(
        window: Arc<Mutex<SlidingWindow>>,
        options: TranscribeOptions,
    ) -> (Self, mpsc::Receiver<Vec<TranscriptionSegment>>) {
        let (sender, receiver) = mpsc::sync_channel(32);
        let stop_flag = Arc::new(AtomicBool::new(true));
        (Self { window, options, stop_flag, handle: None, sender }, receiver)
    }

    /// Start the transcription loop, taking ownership of `model`.
    ///
    /// If the transcriber is already running it is stopped first, then
    /// restarted with the new model.
    pub fn start(&mut self, model: WhisperModel) {
        self.stop_join();

        self.stop_flag.store(false, Ordering::Release);

        let window = Arc::clone(&self.window);
        let options = self.options.clone();
        let stop_flag = Arc::clone(&self.stop_flag);
        let sender = self.sender.clone();

        self.handle = Some(
            std::thread::Builder::new()
                .name("whisper-transcriber".into())
                .spawn(move || {
                    transcription_loop(model, window, options, stop_flag, sender);
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
    stop_flag: Arc<AtomicBool>,
    sender: mpsc::SyncSender<Vec<TranscriptionSegment>>,
) {
    let new_audio_dur = Duration::from_secs(NEW_AUDIO_SECS);
    let window_dur = Duration::from_secs(TRANSCRIBE_WINDOW_SECS);
    let poll = Duration::from_millis(POLL_MS);
    let mut emitted = EmittedSet::new(Duration::from_secs(DEDUP_MEMORY_SECS));
    let mut last_run = Instant::now() - new_audio_dur; // allow first run immediately

    while !stop_flag.load(Ordering::Acquire) {
        std::thread::sleep(poll);

        // Throttle: require at least NEW_AUDIO_SECS of real time since last run.
        if last_run.elapsed() < new_audio_dur {
            continue;
        }

        // Snapshot the last TRANSCRIBE_WINDOW_SECS of clean audio.
        let audio = {
            match window.lock() {
                Ok(w) if !w.is_empty() => w.last(window_dur).samples,
                _ => continue,
            }
        };

        // Run Whisper.  The call is blocking and may take several seconds.
        let raw = match model.transcribe(&audio, &options) {
            Ok(segs) => segs,
            Err(_e) => {
                // Don't spam on repeated errors — just advance the clock.
                last_run = Instant::now();
                continue;
            }
        };

        // Deduplicate against previously emitted text.
        let mut new_segs: Vec<TranscriptionSegment> = Vec::new();
        for mut seg in raw {
            if emitted.contains(&seg.text) {
                seg.is_duplicate = true;
                // Duplicates are not forwarded — drop them here.
            } else {
                emitted.insert(seg.text.clone());
                new_segs.push(seg);
            }
        }

        // Emit the batch (drop silently if receiver has hung up).
        if !new_segs.is_empty() {
            let _ = sender.send(new_segs);
        }

        // Trim the oldest NEW_AUDIO_SECS from the window so it doesn't fill up.
        if let Ok(mut w) = window.lock() {
            w.trim_front(new_audio_dur);
        }

        last_run = Instant::now();
    }
}
