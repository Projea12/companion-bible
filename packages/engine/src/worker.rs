//! Pre-spawned worker thread that owns `LocalAI`.
//!
//! `LocalAI` wraps llama-cpp native pointers that are not `Send`, so it must
//! stay on its own OS thread for its entire lifetime.  `LocalAiHandle` owns a
//! channel to that thread; callers submit jobs and receive results through it.

use std::sync::mpsc::{self, RecvTimeoutError, SyncSender};
use std::time::Duration;

use companion_ai::{LocalAI, LocalAIResult};

// ─── wire types ──────────────────────────────────────────────────────────────

struct Job {
    segment_text: String,
    active_book: Option<String>,
    active_chapter: Option<u8>,
    recent_transcript: String,
    reply: SyncSender<LocalAIResult>,
}

// ─── LocalAiHandle ────────────────────────────────────────────────────────────

/// `Send + Sync` handle to a pre-spawned `LocalAI` worker thread.
pub struct LocalAiHandle {
    tx: SyncSender<Job>,
}

impl LocalAiHandle {
    /// Spawn a dedicated thread that owns `ai` and loops over incoming jobs.
    pub fn spawn(mut ai: LocalAI) -> Self {
        let (tx, rx) = mpsc::sync_channel::<Job>(1);

        std::thread::Builder::new()
            .name("local-ai-worker".into())
            .spawn(move || {
                for job in rx {
                    let result = ai.inference(
                        &job.segment_text,
                        job.active_book.as_deref(),
                        job.active_chapter,
                        &job.recent_transcript,
                    );
                    let _ = job.reply.try_send(result);
                }
            })
            .expect("failed to spawn local-ai-worker thread");

        Self { tx }
    }

    /// Submit a job.  Returns a receiver for the result, or `None` if the
    /// worker is already busy with another segment (bounded channel = 1).
    pub fn try_submit(
        &self,
        segment_text: String,
        active_book: Option<String>,
        active_chapter: Option<u8>,
        recent_transcript: String,
    ) -> Option<mpsc::Receiver<LocalAIResult>> {
        let (reply_tx, reply_rx) = mpsc::sync_channel(1);
        let job = Job {
            segment_text,
            active_book,
            active_chapter,
            recent_transcript,
            reply: reply_tx,
        };
        match self.tx.try_send(job) {
            Ok(_) => Some(reply_rx),
            Err(_) => None, // worker busy — caller degrades gracefully
        }
    }
}

/// `LocalAiHandle` only holds `SyncSender`, which is `Send + Sync`.
/// Safety: the actual `LocalAI` lives entirely on its worker thread.
unsafe impl Send for LocalAiHandle {}
unsafe impl Sync for LocalAiHandle {}

// ─── collect helper ───────────────────────────────────────────────────────────

/// Wait up to `timeout_ms` for a result from the local AI channel.
pub fn collect_local_ai(
    rx: mpsc::Receiver<LocalAIResult>,
    timeout_ms: u64,
) -> Option<LocalAIResult> {
    match rx.recv_timeout(Duration::from_millis(timeout_ms)) {
        Ok(r) => Some(r),
        Err(RecvTimeoutError::Timeout) | Err(RecvTimeoutError::Disconnected) => None,
    }
}
