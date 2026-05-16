use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use crate::transcript::TranscriptionSegment;

// ─── Capacity ─────────────────────────────────────────────────────────────────

/// Default maximum number of segment batches buffered in the channel.
///
/// With Whisper running every 5 seconds, 10 batches = 50 seconds of buffered
/// transcription.  If the consumer falls behind by more than 50 s, the oldest
/// batches are dropped to keep the channel fresh.
pub const CHANNEL_CAPACITY: usize = 10;

// ─── Internal state ───────────────────────────────────────────────────────────

struct State {
    batches: VecDeque<Vec<TranscriptionSegment>>,
    dropped_count: u64,
    closed: bool,
}

struct ChannelInner {
    state: Mutex<State>,
    condvar: Condvar,
    capacity: usize,
    /// Number of live `SegmentSender` handles.  When it reaches 0 the channel
    /// is closed and waiting receivers are woken.
    sender_count: AtomicUsize,
}

// ─── SegmentSender ────────────────────────────────────────────────────────────

/// Sending end of a segment channel.
///
/// Cloneable — every clone shares the same underlying queue.  The channel
/// closes automatically when every clone is dropped.
pub struct SegmentSender {
    inner: Arc<ChannelInner>,
}

impl Clone for SegmentSender {
    fn clone(&self) -> Self {
        self.inner.sender_count.fetch_add(1, Ordering::AcqRel);
        Self { inner: Arc::clone(&self.inner) }
    }
}

impl SegmentSender {
    /// Push `batch` into the channel.
    ///
    /// If the channel is already at capacity, the **oldest** batch is removed
    /// to make room — the sender never blocks.  Returns `true` when a batch
    /// was dropped.
    pub fn send(&self, batch: Vec<TranscriptionSegment>) -> bool {
        let mut st = self.inner.state.lock().unwrap();
        if st.closed {
            return false;
        }
        let dropped = if st.batches.len() >= self.inner.capacity {
            st.batches.pop_front();
            st.dropped_count += 1;
            true
        } else {
            false
        };
        st.batches.push_back(batch);
        drop(st);
        self.inner.condvar.notify_one();
        dropped
    }

    /// Number of batches dropped due to backpressure since the channel was
    /// created.
    pub fn dropped_count(&self) -> u64 {
        self.inner.state.lock().unwrap().dropped_count
    }
}

impl Drop for SegmentSender {
    fn drop(&mut self) {
        // When the last sender is dropped, close the channel and wake receivers.
        if self.inner.sender_count.fetch_sub(1, Ordering::AcqRel) == 1 {
            self.inner.state.lock().unwrap().closed = true;
            self.inner.condvar.notify_all();
        }
    }
}

// ─── SegmentReceiver ──────────────────────────────────────────────────────────

/// Receiving end of a segment channel.
pub struct SegmentReceiver {
    inner: Arc<ChannelInner>,
}

impl SegmentReceiver {
    /// Block until a batch is available.
    ///
    /// Returns `None` when the channel is closed and all buffered batches have
    /// been consumed.
    pub fn recv(&self) -> Option<Vec<TranscriptionSegment>> {
        let mut st = self.inner.state.lock().unwrap();
        loop {
            if let Some(batch) = st.batches.pop_front() {
                return Some(batch);
            }
            if st.closed {
                return None;
            }
            st = self.inner.condvar.wait(st).unwrap();
        }
    }

    /// Wait at most `timeout` for a batch.
    ///
    /// Returns `None` on timeout **or** when the channel is closed and empty.
    pub fn recv_timeout(&self, timeout: Duration) -> Option<Vec<TranscriptionSegment>> {
        let deadline = std::time::Instant::now() + timeout;
        let mut st = self.inner.state.lock().unwrap();
        loop {
            if let Some(batch) = st.batches.pop_front() {
                return Some(batch);
            }
            if st.closed {
                return None;
            }
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return None;
            }
            let (guard, timed_out) = self.inner.condvar.wait_timeout(st, remaining).unwrap();
            st = guard;
            if timed_out.timed_out() {
                if let Some(batch) = st.batches.pop_front() {
                    return Some(batch);
                }
                return None;
            }
        }
    }

    /// Non-blocking receive.  Returns `None` immediately if the channel is
    /// empty.
    pub fn try_recv(&self) -> Option<Vec<TranscriptionSegment>> {
        self.inner.state.lock().unwrap().batches.pop_front()
    }

    /// Number of batches currently waiting in the channel.
    pub fn len(&self) -> usize {
        self.inner.state.lock().unwrap().batches.len()
    }

    /// `true` when the channel is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Number of batches dropped due to backpressure since the channel was
    /// created.
    pub fn dropped_count(&self) -> u64 {
        self.inner.state.lock().unwrap().dropped_count
    }
}

// ─── Constructors ─────────────────────────────────────────────────────────────

/// Create a segment channel with the default capacity ([`CHANNEL_CAPACITY`]).
pub fn segment_channel() -> (SegmentSender, SegmentReceiver) {
    segment_channel_with_capacity(CHANNEL_CAPACITY)
}

/// Create a segment channel with a custom capacity.
pub fn segment_channel_with_capacity(capacity: usize) -> (SegmentSender, SegmentReceiver) {
    assert!(capacity > 0, "channel capacity must be > 0");
    let inner = Arc::new(ChannelInner {
        state: Mutex::new(State {
            batches: VecDeque::with_capacity(capacity + 1),
            dropped_count: 0,
            closed: false,
        }),
        condvar: Condvar::new(),
        capacity,
        sender_count: AtomicUsize::new(1),
    });
    let rx = SegmentReceiver { inner: Arc::clone(&inner) };
    let tx = SegmentSender { inner };
    (tx, rx)
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn make_batch(id: u64) -> Vec<TranscriptionSegment> {
        vec![TranscriptionSegment {
            text: format!("segment {id}"),
            audio_start_ms: id * 1_000,
            audio_end_ms: id * 1_000 + 500,
            whisper_confidence: 0.9,
            is_duplicate: false,
            context_window: String::new(),
        }]
    }

    // ── Capacity and ordering ─────────────────────────────────────────────────

    #[test]
    fn default_capacity_is_ten() {
        let (tx, rx) = segment_channel();
        for i in 0..CHANNEL_CAPACITY {
            tx.send(make_batch(i as u64));
        }
        assert_eq!(rx.len(), CHANNEL_CAPACITY);
        assert_eq!(tx.dropped_count(), 0);
    }

    #[test]
    fn fifo_order_preserved_within_capacity() {
        let (tx, rx) = segment_channel();
        for i in 0..5u64 {
            tx.send(make_batch(i));
        }
        for i in 0..5u64 {
            let batch = rx.try_recv().unwrap();
            assert_eq!(batch[0].text, format!("segment {i}"));
        }
    }

    // ── Backpressure: drop oldest ─────────────────────────────────────────────

    #[test]
    fn send_when_full_drops_oldest() {
        let (tx, rx) = segment_channel_with_capacity(3);

        tx.send(make_batch(1)); // oldest
        tx.send(make_batch(2));
        tx.send(make_batch(3)); // capacity reached
        let dropped = tx.send(make_batch(4)); // overflows — batch 1 should be dropped
        assert!(dropped, "send must report that a batch was dropped");

        // Batch 1 (oldest) is gone; batches 2, 3, 4 remain.
        let texts: Vec<String> = (0..3)
            .map(|_| rx.try_recv().unwrap()[0].text.clone())
            .collect();
        assert_eq!(texts, ["segment 2", "segment 3", "segment 4"]);
    }

    #[test]
    fn dropped_count_increments_on_overflow() {
        let (tx, rx) = segment_channel_with_capacity(2);
        tx.send(make_batch(1));
        tx.send(make_batch(2));
        assert_eq!(rx.dropped_count(), 0);

        tx.send(make_batch(3)); // drops batch 1
        tx.send(make_batch(4)); // drops batch 2
        assert_eq!(rx.dropped_count(), 2);
        assert_eq!(tx.dropped_count(), 2, "both handles see the same count");
    }

    #[test]
    fn after_overflow_newest_batches_remain() {
        let (tx, rx) = segment_channel_with_capacity(3);
        // Send 10 batches into a capacity-3 channel.
        for i in 1..=10u64 {
            tx.send(make_batch(i));
        }
        assert_eq!(rx.dropped_count(), 7, "7 oldest batches must have been dropped");
        assert_eq!(rx.len(), 3);

        // The 3 remaining must be the newest: 8, 9, 10.
        for expected in [8u64, 9, 10] {
            let batch = rx.try_recv().unwrap();
            assert_eq!(
                batch[0].text,
                format!("segment {expected}"),
                "expected newest batch {expected}"
            );
        }
    }

    // ── Blocking receive ──────────────────────────────────────────────────────

    #[test]
    fn recv_blocks_until_sender_sends() {
        let (tx, rx) = segment_channel();
        let handle = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            tx.send(make_batch(42));
        });
        let batch = rx.recv().unwrap();
        assert_eq!(batch[0].text, "segment 42");
        handle.join().unwrap();
    }

    #[test]
    fn recv_returns_none_when_sender_dropped_and_empty() {
        let (tx, rx) = segment_channel();
        drop(tx);
        assert!(rx.recv().is_none(), "recv must return None after sender drop");
    }

    #[test]
    fn recv_drains_remaining_batches_before_returning_none() {
        let (tx, rx) = segment_channel();
        tx.send(make_batch(1));
        tx.send(make_batch(2));
        drop(tx); // close sender

        assert_eq!(rx.recv().unwrap()[0].text, "segment 1");
        assert_eq!(rx.recv().unwrap()[0].text, "segment 2");
        assert!(rx.recv().is_none(), "must be None once drained");
    }

    // ── recv_timeout ──────────────────────────────────────────────────────────

    #[test]
    fn recv_timeout_returns_none_when_channel_empty() {
        let (_tx, rx) = segment_channel();
        let result = rx.recv_timeout(Duration::from_millis(30));
        assert!(result.is_none(), "recv_timeout must return None on empty channel");
    }

    #[test]
    fn recv_timeout_returns_batch_when_available() {
        let (tx, rx) = segment_channel();
        tx.send(make_batch(7));
        let batch = rx.recv_timeout(Duration::from_millis(100)).unwrap();
        assert_eq!(batch[0].text, "segment 7");
    }

    #[test]
    fn recv_timeout_wakes_when_sender_sends_during_wait() {
        let (tx, rx) = segment_channel();
        let handle = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(40));
            tx.send(make_batch(99));
        });
        let batch = rx.recv_timeout(Duration::from_millis(500)).unwrap();
        assert_eq!(batch[0].text, "segment 99");
        handle.join().unwrap();
    }

    // ── try_recv ──────────────────────────────────────────────────────────────

    #[test]
    fn try_recv_returns_none_on_empty_channel() {
        let (_tx, rx) = segment_channel();
        assert!(rx.try_recv().is_none());
    }

    #[test]
    fn try_recv_returns_batch_without_blocking() {
        let (tx, rx) = segment_channel();
        tx.send(make_batch(5));
        assert_eq!(rx.try_recv().unwrap()[0].text, "segment 5");
        assert!(rx.try_recv().is_none(), "channel must be empty after single recv");
    }

    // ── Clone sender ──────────────────────────────────────────────────────────

    #[test]
    fn cloned_sender_can_send() {
        let (tx, rx) = segment_channel();
        let tx2 = tx.clone();
        tx.send(make_batch(1));
        tx2.send(make_batch(2));
        assert_eq!(rx.len(), 2);
    }

    #[test]
    fn channel_stays_open_while_any_sender_alive() {
        let (tx, rx) = segment_channel();
        let tx2 = tx.clone();
        drop(tx); // drop first clone — channel still open

        tx2.send(make_batch(10));
        assert_eq!(rx.recv().unwrap()[0].text, "segment 10");
        drop(tx2); // now all senders gone

        assert!(rx.recv().is_none());
    }

    // ── Load tests ────────────────────────────────────────────────────────────

    #[test]
    fn load_rapid_sends_keep_newest_batches() {
        let capacity = 10;
        let total_sends = 50u64;
        let (tx, rx) = segment_channel_with_capacity(capacity);

        for i in 0..total_sends {
            tx.send(make_batch(i));
        }

        let expected_dropped = total_sends - capacity as u64;
        assert_eq!(
            rx.dropped_count(),
            expected_dropped,
            "should drop exactly {expected_dropped} oldest batches"
        );

        // Remaining batches must be the newest `capacity` ones.
        let first_expected = total_sends - capacity as u64;
        for i in 0..capacity as u64 {
            let batch = rx.try_recv().unwrap();
            assert_eq!(
                batch[0].text,
                format!("segment {}", first_expected + i),
                "remaining batch must be newest"
            );
        }
        assert!(rx.try_recv().is_none(), "channel must be empty after draining");
    }

    #[test]
    fn load_concurrent_sender_receiver_no_panic() {
        use std::sync::atomic::{AtomicU64, Ordering};

        let (tx, rx) = segment_channel_with_capacity(5);
        let received = Arc::new(AtomicU64::new(0));
        let received_clone = Arc::clone(&received);

        // Fast sender — 100 batches with no delay.
        let sender_handle = std::thread::spawn(move || {
            for i in 0..100u64 {
                tx.send(make_batch(i));
            }
        });

        // Slow receiver — 10 ms between reads.
        let receiver_handle = std::thread::spawn(move || {
            while let Some(_) = rx.recv_timeout(Duration::from_millis(200)) {
                received_clone.fetch_add(1, Ordering::Relaxed);
                std::thread::sleep(Duration::from_millis(10));
            }
        });

        sender_handle.join().unwrap();
        receiver_handle.join().unwrap();

        let got = received.load(Ordering::Relaxed);
        // We can't receive more than we sent, and at least capacity batches
        // must have been received.
        assert!(got <= 100, "received more batches than sent: {got}");
        assert!(got > 0, "receiver got nothing");
        println!("load_concurrent: received {got}/100 batches ({} dropped)", 100 - got);
    }
}
