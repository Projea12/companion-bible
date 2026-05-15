use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicUsize, Ordering};

// 16 000 Hz × 30 s = 480 000; next power of two = 2^19 = 524 288.
pub const DEFAULT_CAPACITY: usize = 524_288;

/// Lock-free, single-producer / single-consumer ring buffer.
///
/// The write head is advanced only by `write()`; the read head only by
/// `read()`.  When `write()` is called on a full buffer it **overwrites the
/// oldest samples** rather than blocking or dropping the incoming data.
/// This is detected by `read()`, which automatically skips to the oldest
/// valid position when the producer has lapped the consumer.
///
/// # Safety
/// Concurrent calls to `write()` from more than one thread, or concurrent
/// calls to `read()` from more than one thread, are unsound.  The intended
/// usage is one audio-callback thread calling `write()` and one processing
/// thread calling `read()`.
pub struct RingBuffer<T: Copy + Default> {
    buf: Box<[UnsafeCell<T>]>,
    /// Always a power of two.
    capacity: usize,
    /// `capacity - 1`; used for fast modulo via bitwise AND.
    mask: usize,
    /// Monotonically increasing logical index of the next write slot.
    /// Only ever written by `write()`.
    write_head: AtomicUsize,
    /// Monotonically increasing logical index of the next read slot.
    /// Only ever written by `read()`.
    read_head: AtomicUsize,
}

// SPSC invariant: `write()` and `read()` are each called from at most one
// thread at a time, so the `UnsafeCell` accesses cannot alias.
unsafe impl<T: Copy + Default + Send> Send for RingBuffer<T> {}
unsafe impl<T: Copy + Default + Send> Sync for RingBuffer<T> {}

impl<T: Copy + Default> RingBuffer<T> {
    /// Create a ring buffer with the given capacity.
    ///
    /// # Panics
    /// Panics if `capacity` is not a power of two or is zero.
    pub fn new(capacity: usize) -> Self {
        assert!(capacity.is_power_of_two() && capacity > 0, "capacity must be a non-zero power of two");
        let buf: Box<[UnsafeCell<T>]> =
            (0..capacity).map(|_| UnsafeCell::new(T::default())).collect();
        Self {
            buf,
            capacity,
            mask: capacity - 1,
            write_head: AtomicUsize::new(0),
            read_head: AtomicUsize::new(0),
        }
    }

    /// Create a ring buffer sized for 30 seconds of audio at 16 kHz (524 288 samples).
    pub fn with_default_capacity() -> Self {
        Self::new(DEFAULT_CAPACITY)
    }

    /// Total number of slots in the buffer.
    #[inline]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Number of samples available to read.
    ///
    /// If the producer has lapped the consumer the return value is capped at
    /// `capacity` — the oldest samples have been overwritten.
    #[inline]
    pub fn available(&self) -> usize {
        let wh = self.write_head.load(Ordering::Acquire);
        let rh = self.read_head.load(Ordering::Relaxed);
        (wh - rh).min(self.capacity)
    }

    /// `true` when there is nothing to read.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.available() == 0
    }

    /// Append `samples` to the buffer without blocking.
    ///
    /// If `samples` is longer than `capacity`, only the **newest** `capacity`
    /// samples are kept.  If the buffer is full, the oldest unread samples are
    /// silently overwritten.
    pub fn write(&self, samples: &[T]) {
        if samples.is_empty() {
            return;
        }

        // If the caller provides more samples than the buffer can ever hold,
        // keep only the newest `capacity` samples.
        let (skip, n) = if samples.len() > self.capacity {
            (samples.len() - self.capacity, self.capacity)
        } else {
            (0, samples.len())
        };
        let samples = &samples[skip..];

        // write_head is owned by this function — Relaxed load is fine.
        let wh = self.write_head.load(Ordering::Relaxed);

        // Write each sample into its slot.  Slots beyond the current
        // read_head + capacity are "owned" by the writer and will be read
        // later; slots that haven't been consumed yet are overwritten
        // (drop-oldest behaviour).
        for (i, &s) in samples.iter().enumerate() {
            let idx = (wh + i) & self.mask;
            // Safety: idx is unique to this write window within a SPSC contract.
            unsafe { *self.buf[idx].get() = s };
        }

        // Release-store so the consumer sees the new samples.
        self.write_head.store(wh + n, Ordering::Release);
    }

    /// Remove and return up to `count` samples.
    ///
    /// Returns however many samples are actually available, up to `count`.
    /// If the producer has lapped the consumer (overwritten unread data),
    /// the read cursor is silently advanced to the oldest valid position
    /// before reading.
    /// Remove and return up to `count` samples.
    ///
    /// Returns however many samples are actually available, up to `count`.
    /// If the producer has lapped the consumer (overwritten unread data),
    /// the read cursor is silently advanced to the oldest valid position
    /// before reading.
    ///
    /// If the producer laps the consumer **during** the read (i.e. the
    /// producer wrote `capacity` more samples between the initial
    /// `write_head` snapshot and the end of the copy loop), the partially
    /// read data is discarded and an empty `Vec` is returned.  The caller
    /// should retry immediately; the buffer will contain fresh data.
    pub fn read(&self, count: usize) -> Vec<T> {
        // Acquire-load so we see all samples written before write_head was stored.
        let wh = self.write_head.load(Ordering::Acquire);
        let rh = self.read_head.load(Ordering::Relaxed);

        // If the producer lapped us, skip to the oldest valid data.
        let effective_rh = if wh.saturating_sub(rh) > self.capacity {
            wh - self.capacity
        } else {
            rh
        };

        let available = wh - effective_rh;
        let n = count.min(available);
        if n == 0 {
            return Vec::new();
        }

        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            let idx = (effective_rh + i) & self.mask;
            // Safety: write_head was Acquire-loaded above; any slot before
            // write_head has a completed Release-store from the writer.
            out.push(unsafe { *self.buf[idx].get() });
        }

        // Guard: if the writer advanced write_head by a full capacity since
        // we took our snapshot, it has overwritten slots we just read.
        // Detect this by re-loading write_head (Acquire keeps ordering).
        // Since write_head is monotonic, if it now exceeds
        // effective_rh + capacity the writer must have lapped us.
        let wh_after = self.write_head.load(Ordering::Acquire);
        if wh_after.saturating_sub(effective_rh) > self.capacity {
            // Data integrity cannot be guaranteed; let the caller retry.
            return Vec::new();
        }

        // Publish the new read cursor.
        self.read_head.store(effective_rh + n, Ordering::Release);
        out
    }
}
