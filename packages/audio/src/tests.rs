use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use crate::device::{infer_device_type, AudioDevice, DeviceType};
use crate::error::AudioError;
use crate::input::AudioInput;
use crate::ring_buffer::{RingBuffer, DEFAULT_CAPACITY};
use crate::preprocess::{AudioPreprocessor, NoiseGate, RNNOISE_FRAME_SIZE};
use crate::vad::{VadDecision, VoiceActivityDetector, CHUNK_SIZE, DEFAULT_THRESHOLD};

// ─── AudioDevice ──────────────────────────────────────────────────────────────

#[test]
fn audio_device_clone_and_eq() {
    let d = AudioDevice {
        id: "mic-1".into(),
        name: "Built-in Microphone".into(),
        device_type: DeviceType::Builtin,
        is_default: true,
    };
    assert_eq!(d.clone(), d);
}

#[test]
fn audio_device_serde_roundtrip() {
    let d = AudioDevice {
        id: "usb-1".into(),
        name: "USB Audio Device".into(),
        device_type: DeviceType::UsbMic,
        is_default: false,
    };
    let json = serde_json::to_string(&d).unwrap();
    let back: AudioDevice = serde_json::from_str(&json).unwrap();
    assert_eq!(d, back);
}

// ─── DeviceType ───────────────────────────────────────────────────────────────

#[test]
fn device_type_serde_roundtrip() {
    for dt in [DeviceType::Mixer, DeviceType::UsbMic, DeviceType::Builtin] {
        let json = serde_json::to_string(&dt).unwrap();
        let back: DeviceType = serde_json::from_str(&json).unwrap();
        assert_eq!(dt, back);
    }
}

// ─── infer_device_type ────────────────────────────────────────────────────────

#[test]
fn infer_usb_device() {
    assert_eq!(infer_device_type("USB Audio Codec"), DeviceType::UsbMic);
    assert_eq!(infer_device_type("Focusrite USB Interface"), DeviceType::UsbMic);
}

#[test]
fn infer_mixer_device() {
    assert_eq!(infer_device_type("Line In (Realtek)"), DeviceType::Mixer);
    assert_eq!(infer_device_type("Behringer Mixer"), DeviceType::Mixer);
    assert_eq!(infer_device_type("Aggregate Device"), DeviceType::Mixer);
}

#[test]
fn infer_builtin_device() {
    assert_eq!(infer_device_type("Built-in Microphone"), DeviceType::Builtin);
    assert_eq!(infer_device_type("Internal Mic"), DeviceType::Builtin);
}

// ─── Device enumeration ───────────────────────────────────────────────────────

fn make_device(id: &str, name: &str, dt: DeviceType, is_default: bool) -> AudioDevice {
    AudioDevice { id: id.into(), name: name.into(), device_type: dt, is_default }
}

#[test]
fn enumeration_returns_all_devices() {
    let devs = vec![
        make_device("d1", "USB Audio", DeviceType::UsbMic, false),
        make_device("d2", "Built-in Mic", DeviceType::Builtin, true),
        make_device("d3", "Line In", DeviceType::Mixer, false),
    ];
    let input = MockInput::new(devs.clone());
    let result = input.available_devices().unwrap();
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].id, "d1");
    assert_eq!(result[1].id, "d2");
    assert_eq!(result[2].id, "d3");
}

#[test]
fn enumeration_identifies_default_device() {
    let devs = vec![
        make_device("d1", "USB Audio", DeviceType::UsbMic, false),
        make_device("d2", "Built-in Mic", DeviceType::Builtin, true),
    ];
    let input = MockInput::new(devs);
    let result = input.available_devices().unwrap();
    let default_devs: Vec<_> = result.iter().filter(|d| d.is_default).collect();
    assert_eq!(default_devs.len(), 1);
    assert_eq!(default_devs[0].id, "d2");
}

#[test]
fn enumeration_empty_returns_error() {
    let input = MockInput::new(vec![]);
    assert!(matches!(input.available_devices(), Err(AudioError::NoDevices)));
}

#[test]
fn enumeration_device_fields_are_correct() {
    let devs = vec![make_device("usb-42", "Focusrite USB", DeviceType::UsbMic, false)];
    let input = MockInput::new(devs);
    let result = input.available_devices().unwrap();
    let d = &result[0];
    assert_eq!(d.id, "usb-42");
    assert_eq!(d.name, "Focusrite USB");
    assert_eq!(d.device_type, DeviceType::UsbMic);
    assert!(!d.is_default);
}

// ─── Device selection ─────────────────────────────────────────────────────────

#[test]
fn selection_valid_device_is_recorded() {
    let mut input = MockInput::new(vec![
        make_device("d1", "USB Audio", DeviceType::UsbMic, false),
        make_device("d2", "Built-in Mic", DeviceType::Builtin, true),
    ]);
    input.select_device("d1").unwrap();
    assert_eq!(input.selected.as_deref(), Some("d1"));
}

#[test]
fn selection_unknown_device_errors() {
    let mut input = MockInput::new(vec![make_device("d1", "USB Audio", DeviceType::UsbMic, false)]);
    let err = input.select_device("ghost").unwrap_err();
    assert!(matches!(err, AudioError::DeviceNotFound(_)));
}

#[test]
fn selection_can_be_changed() {
    let mut input = MockInput::new(vec![
        make_device("d1", "USB Audio", DeviceType::UsbMic, false),
        make_device("d2", "Built-in Mic", DeviceType::Builtin, true),
    ]);
    input.select_device("d1").unwrap();
    assert_eq!(input.selected.as_deref(), Some("d1"));
    input.select_device("d2").unwrap();
    assert_eq!(input.selected.as_deref(), Some("d2"));
}

#[test]
fn selection_persists_after_start_stop() {
    let mut input = MockInput::new(vec![
        make_device("d1", "USB Audio", DeviceType::UsbMic, false),
    ]);
    input.select_device("d1").unwrap();
    input.start(Box::new(|_| {})).unwrap();
    input.stop();
    // selection should still be set after stop
    assert_eq!(input.selected.as_deref(), Some("d1"));
}

// ─── Mock AudioInput ──────────────────────────────────────────────────────────

struct MockInput {
    devices: Vec<AudioDevice>,
    selected: Option<String>,
    running: bool,
    level: f32,
}

impl MockInput {
    fn new(devices: Vec<AudioDevice>) -> Self {
        Self { devices, selected: None, running: false, level: 0.0 }
    }
}

impl AudioInput for MockInput {
    fn available_devices(&self) -> Result<Vec<AudioDevice>, AudioError> {
        if self.devices.is_empty() {
            Err(AudioError::NoDevices)
        } else {
            Ok(self.devices.clone())
        }
    }

    fn select_device(&mut self, device_id: &str) -> Result<(), AudioError> {
        if self.devices.iter().any(|d| d.id == device_id) {
            self.selected = Some(device_id.to_string());
            Ok(())
        } else {
            Err(AudioError::DeviceNotFound(device_id.to_string()))
        }
    }

    fn start(&mut self, callback: Box<dyn Fn(Vec<f32>) + Send + 'static>) -> Result<(), AudioError> {
        self.running = true;
        callback(vec![0.5, 0.4, 0.3]);
        self.level = 0.5;
        Ok(())
    }

    fn stop(&mut self) {
        self.running = false;
        self.level = 0.0;
    }

    fn current_level(&self) -> f32 {
        self.level
    }
}

#[test]
fn mock_available_devices_empty() {
    let input = MockInput::new(vec![]);
    assert!(matches!(input.available_devices(), Err(AudioError::NoDevices)));
}

#[test]
fn mock_select_unknown_device() {
    let mut input = MockInput::new(vec![AudioDevice {
        id: "dev-1".into(),
        name: "Dev 1".into(),
        device_type: DeviceType::Builtin,
        is_default: true,
    }]);
    let result = input.select_device("ghost");
    assert!(matches!(result, Err(AudioError::DeviceNotFound(_))));
}

#[test]
fn mock_start_invokes_callback() {
    let received = Arc::new(Mutex::new(Vec::<f32>::new()));
    let received_clone = Arc::clone(&received);

    let mut input = MockInput::new(vec![AudioDevice {
        id: "dev-1".into(),
        name: "Dev 1".into(),
        device_type: DeviceType::Builtin,
        is_default: true,
    }]);

    input
        .start(Box::new(move |samples| {
            received_clone.lock().unwrap().extend_from_slice(&samples);
        }))
        .unwrap();

    assert!(!received.lock().unwrap().is_empty());
    assert!(input.current_level() > 0.0);
}

#[test]
fn mock_stop_resets_level() {
    let mut input = MockInput::new(vec![AudioDevice {
        id: "dev-1".into(),
        name: "Dev 1".into(),
        device_type: DeviceType::Builtin,
        is_default: true,
    }]);
    input.start(Box::new(|_| {})).unwrap();
    assert!(input.current_level() > 0.0);
    input.stop();
    assert_eq!(input.current_level(), 0.0);
    assert!(!input.running);
}

// ─── Hardware-gated tests ─────────────────────────────────────────────────────
// These tests require actual audio hardware and are skipped in CI.
// Run with:  AUDIO_HW_TESTS=1 cargo test -p companion-audio

#[test]
#[ignore = "requires audio hardware: AUDIO_HW_TESTS=1"]
fn hardware_builtin_mic_available_devices() {
    use crate::BuiltinMicInput;
    use crate::input::AudioInput;
    let input = BuiltinMicInput::new();
    let devices = input.available_devices().expect("should find at least one device");
    assert!(!devices.is_empty());
    for d in &devices {
        assert_eq!(d.device_type, DeviceType::Builtin);
    }
}

#[test]
#[ignore = "requires audio hardware: AUDIO_HW_TESTS=1"]
fn hardware_builtin_mic_start_stop() {
    use std::time::Duration;
    use crate::BuiltinMicInput;
    use crate::input::AudioInput;

    let received = Arc::new(Mutex::new(0usize));
    let counter = Arc::clone(&received);

    let mut input = BuiltinMicInput::new();
    input
        .start(Box::new(move |samples| {
            *counter.lock().unwrap() += samples.len();
        }))
        .expect("start failed");

    std::thread::sleep(Duration::from_millis(200));
    assert!(*received.lock().unwrap() > 0, "no samples received");

    input.stop();
}

// ─── RingBuffer ───────────────────────────────────────────────────────────────

#[test]
fn ring_buffer_default_capacity_is_30_seconds_at_16khz() {
    assert_eq!(DEFAULT_CAPACITY, 524_288);
    assert!(DEFAULT_CAPACITY >= 16_000 * 30);
    assert!(DEFAULT_CAPACITY.is_power_of_two());
}

#[test]
fn ring_buffer_new_is_empty() {
    let rb: RingBuffer<f32> = RingBuffer::new(16);
    assert!(rb.is_empty());
    assert_eq!(rb.available(), 0);
}

#[test]
fn ring_buffer_write_then_read_roundtrip() {
    let rb: RingBuffer<f32> = RingBuffer::new(16);
    rb.write(&[1.0, 2.0, 3.0]);
    assert_eq!(rb.available(), 3);
    let out = rb.read(3);
    assert_eq!(out, vec![1.0, 2.0, 3.0]);
    assert!(rb.is_empty());
}

#[test]
fn ring_buffer_partial_read() {
    let rb: RingBuffer<f32> = RingBuffer::new(16);
    rb.write(&[1.0, 2.0, 3.0, 4.0]);
    let out = rb.read(2);
    assert_eq!(out, vec![1.0, 2.0]);
    assert_eq!(rb.available(), 2);
    let out2 = rb.read(2);
    assert_eq!(out2, vec![3.0, 4.0]);
    assert!(rb.is_empty());
}

#[test]
fn ring_buffer_read_more_than_available_returns_what_exists() {
    let rb: RingBuffer<f32> = RingBuffer::new(16);
    rb.write(&[1.0, 2.0]);
    let out = rb.read(100);
    assert_eq!(out, vec![1.0, 2.0]);
}

#[test]
fn ring_buffer_read_empty_returns_empty_vec() {
    let rb: RingBuffer<f32> = RingBuffer::new(16);
    let out = rb.read(10);
    assert!(out.is_empty());
}

#[test]
fn ring_buffer_drops_oldest_when_full() {
    let rb: RingBuffer<f32> = RingBuffer::new(4); // capacity = 4
    rb.write(&[1.0, 2.0, 3.0, 4.0]);             // fills buffer
    rb.write(&[5.0, 6.0]);                         // overwrites oldest two

    // Buffer should now contain [3.0, 4.0, 5.0, 6.0] — oldest two dropped.
    assert_eq!(rb.available(), 4);
    let out = rb.read(4);
    assert_eq!(out, vec![3.0, 4.0, 5.0, 6.0]);
}

#[test]
fn ring_buffer_write_larger_than_capacity_keeps_newest() {
    let rb: RingBuffer<f32> = RingBuffer::new(4);
    // Write 6 samples into a capacity-4 buffer — only last 4 kept.
    rb.write(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    assert_eq!(rb.available(), 4);
    let out = rb.read(4);
    assert_eq!(out, vec![3.0, 4.0, 5.0, 6.0]);
}

#[test]
fn ring_buffer_available_never_exceeds_capacity() {
    let rb: RingBuffer<f32> = RingBuffer::new(4);
    rb.write(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0]);
    assert!(rb.available() <= rb.capacity());
}

#[test]
fn ring_buffer_wraps_around_correctly() {
    let rb: RingBuffer<f32> = RingBuffer::new(4);
    rb.write(&[1.0, 2.0, 3.0]);
    rb.read(2); // consume 2 → read_head = 2
    rb.write(&[4.0, 5.0]); // wraps: slots [0]=4.0 [1]=5.0, write_head=5
    // available: 5 - 2 = 3 → [3.0, 4.0, 5.0]
    let out = rb.read(3);
    assert_eq!(out, vec![3.0, 4.0, 5.0]);
}

#[test]
fn ring_buffer_multiple_write_read_cycles() {
    let rb: RingBuffer<i32> = RingBuffer::new(8);
    for batch in 0..10_i32 {
        let samples: Vec<i32> = (batch * 4..(batch + 1) * 4).collect();
        rb.write(&samples);
        let out = rb.read(4);
        assert_eq!(out, samples, "batch {batch}");
    }
    assert!(rb.is_empty());
}

#[test]
fn ring_buffer_write_empty_slice_is_noop() {
    let rb: RingBuffer<f32> = RingBuffer::new(8);
    rb.write(&[]);
    assert!(rb.is_empty());
    rb.write(&[1.0, 2.0]);
    rb.write(&[]);
    assert_eq!(rb.available(), 2);
}

#[test]
fn ring_buffer_capacity_reported_correctly() {
    let rb: RingBuffer<f32> = RingBuffer::new(64);
    assert_eq!(rb.capacity(), 64);
}

#[test]
#[should_panic]
fn ring_buffer_non_power_of_two_panics() {
    let _rb: RingBuffer<f32> = RingBuffer::new(100);
}

// ─── Overflow tests ───────────────────────────────────────────────────────────

#[test]
fn overflow_fill_exact_capacity_saturates_available() {
    let cap = 8;
    let rb: RingBuffer<i32> = RingBuffer::new(cap);
    let samples: Vec<i32> = (0..cap as i32).collect();
    rb.write(&samples);
    assert_eq!(rb.available(), cap, "available should equal capacity when full");
    assert!(!rb.is_empty());
}

#[test]
fn overflow_one_extra_drops_oldest_not_newest() {
    let rb: RingBuffer<i32> = RingBuffer::new(4);
    rb.write(&[10, 20, 30, 40]); // fills buffer
    rb.write(&[50]);              // one overflow: drops 10

    let out = rb.read(4);
    assert_eq!(out, vec![20, 30, 40, 50], "oldest sample (10) must be dropped, not newest (50)");
}

#[test]
fn overflow_two_extra_drops_two_oldest() {
    let rb: RingBuffer<i32> = RingBuffer::new(4);
    rb.write(&[1, 2, 3, 4]);
    rb.write(&[5, 6]); // overwrites 1 and 2

    let out = rb.read(4);
    assert_eq!(out, vec![3, 4, 5, 6]);
}

#[test]
fn overflow_2x_capacity_keeps_only_last_capacity_samples() {
    let cap = 8;
    let rb: RingBuffer<i32> = RingBuffer::new(cap);
    let all: Vec<i32> = (0..cap as i32 * 2).collect(); // 0..16
    rb.write(&all);

    assert_eq!(rb.available(), cap);
    let out = rb.read(cap);
    // Only the last `cap` values should survive.
    assert_eq!(out, vec![8, 9, 10, 11, 12, 13, 14, 15]);
}

#[test]
fn overflow_progressive_wave_always_keeps_newest() {
    // Write in waves that each overflow by half; after each wave the buffer
    // should hold the last `capacity` samples of that wave.
    let cap = 8usize;
    let rb: RingBuffer<i32> = RingBuffer::new(cap);

    for wave in 0..4_i32 {
        let base = wave * cap as i32 * 2;
        let samples: Vec<i32> = (base..base + cap as i32 * 2).collect();
        rb.write(&samples);

        let expected_start = base + cap as i32; // newest cap samples
        let out = rb.read(cap);
        let expected: Vec<i32> = (expected_start..expected_start + cap as i32).collect();
        assert_eq!(out, expected, "wave {wave}");
    }
}

#[test]
fn overflow_available_never_exceeds_capacity_under_heavy_writes() {
    let rb: RingBuffer<f32> = RingBuffer::new(16);
    for _ in 0..10 {
        rb.write(&[1.0; 20]); // 20 > 16
        assert!(rb.available() <= rb.capacity());
    }
}

// ─── Concurrency tests ────────────────────────────────────────────────────────
//
// Guarantees that hold under concurrent SPSC use:
//   1. No deadlock — lock-free atomics cannot deadlock by construction.
//   2. Values read are always within the set of values actually written.
//   3. When the buffer is large enough to prevent lapping, FIFO order is
//      strictly preserved.
//
// What is NOT guaranteed when the producer laps the consumer:
//   - Strict monotonic ordering (the post-read guard returns empty on a
//     concurrent lap, so the reader retries without exposing corrupt data,
//     but gaps in the sequence are expected).

use std::sync::atomic::AtomicBool;

fn done_flag() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}

#[test]
fn concurrent_spsc_completes_without_deadlock() {
    // All-atomic implementation cannot deadlock; verify both threads terminate.
    // The buffer is large enough to hold all written data without any lapping,
    // which means the reader will eventually drain everything.
    let rb = Arc::new(RingBuffer::<i32>::new(65536));
    let writer_rb = Arc::clone(&rb);
    let reader_rb = Arc::clone(&rb);
    let done = done_flag();
    let done_r = Arc::clone(&done);

    const TOTAL: i32 = 10_000;

    let writer = std::thread::spawn(move || {
        for batch_start in (0..TOTAL).step_by(16) {
            let end = (batch_start + 16).min(TOTAL);
            let batch: Vec<i32> = (batch_start..end).collect();
            writer_rb.write(&batch);
        }
        done.store(true, Ordering::Release);
    });

    let reader = std::thread::spawn(move || {
        let mut consumed = 0usize;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            assert!(std::time::Instant::now() < deadline, "timed out — possible livelock");
            let out = reader_rb.read(64);
            consumed += out.len();
            if out.is_empty() {
                if done_r.load(Ordering::Acquire) && reader_rb.is_empty() { break; }
                std::hint::spin_loop();
            }
        }
        consumed
    });

    writer.join().unwrap();
    let consumed = reader.join().unwrap();
    assert_eq!(consumed, TOTAL as usize, "every written sample must be read when no lapping occurs");
}

#[test]
fn concurrent_spsc_no_lapping_preserves_fifo_order() {
    // With a buffer large enough to never lap, every value read must be
    // strictly greater than the previous (FIFO order is a hard guarantee
    // in the no-overwrite path).
    let rb = Arc::new(RingBuffer::<i32>::new(65536));
    let writer_rb = Arc::clone(&rb);
    let reader_rb = Arc::clone(&rb);
    let done = done_flag();
    let done_r = Arc::clone(&done);

    const TOTAL: i32 = 10_000;

    let writer = std::thread::spawn(move || {
        for i in 0..TOTAL {
            writer_rb.write(&[i]);
        }
        done.store(true, Ordering::Release);
    });

    let reader = std::thread::spawn(move || {
        let mut last: Option<i32> = None;
        let mut reads = 0usize;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            assert!(std::time::Instant::now() < deadline, "timed out");
            let out = reader_rb.read(16);
            for &v in &out {
                if let Some(prev) = last {
                    assert!(v > prev, "FIFO violation: got {v} after {prev}");
                }
                last = Some(v);
                reads += 1;
            }
            if out.is_empty() {
                if done_r.load(Ordering::Acquire) && reader_rb.is_empty() { break; }
                std::hint::spin_loop();
            }
        }
        reads
    });

    writer.join().unwrap();
    let reads = reader.join().unwrap();
    assert_eq!(reads, TOTAL as usize);
}

#[test]
fn concurrent_spsc_all_read_values_within_written_range() {
    // Every value returned by read() must be a value that was actually written.
    // The post-read guard in read() discards any batch where the writer lapped
    // us mid-read, so the consumer never sees a torn or phantom value.
    let rb = Arc::new(RingBuffer::<i32>::new(256));
    let writer_rb = Arc::clone(&rb);
    let reader_rb = Arc::clone(&rb);
    let done = done_flag();
    let done_r = Arc::clone(&done);

    const TOTAL: i32 = 20_000;

    let writer = std::thread::spawn(move || {
        for batch_start in (0..TOTAL).step_by(16) {
            let end = (batch_start + 16).min(TOTAL);
            let batch: Vec<i32> = (batch_start..end).collect();
            writer_rb.write(&batch);
        }
        done.store(true, Ordering::Release);
    });

    let reader = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            assert!(std::time::Instant::now() < deadline, "timed out");
            let out = reader_rb.read(32);
            for &v in &out {
                assert!(
                    v >= 0 && v < TOTAL,
                    "value {v} outside [0, {TOTAL}) — was never written"
                );
            }
            if out.is_empty() {
                if done_r.load(Ordering::Acquire) && reader_rb.is_empty() { break; }
                std::hint::spin_loop();
            }
        }
    });

    writer.join().unwrap();
    reader.join().unwrap();
}

// ─── VoiceActivityDetector ────────────────────────────────────────────────────

fn silence_chunk() -> Vec<f32> {
    vec![0.0f32; CHUNK_SIZE]
}

fn loud_chunk() -> Vec<f32> {
    // Full-scale square wave — RMS = 1.0
    vec![1.0f32; CHUNK_SIZE]
}


#[test]
fn vad_default_threshold_is_half() {
    let vad = VoiceActivityDetector::new_energy();
    assert_eq!(vad.threshold(), DEFAULT_THRESHOLD);
    assert_eq!(DEFAULT_THRESHOLD, 0.5);
}

#[test]
fn vad_set_threshold_clamps_to_unit_range() {
    let mut vad = VoiceActivityDetector::new_energy();
    vad.set_threshold(1.5);
    assert_eq!(vad.threshold(), 1.0);
    vad.set_threshold(-0.3);
    assert_eq!(vad.threshold(), 0.0);
    vad.set_threshold(0.7);
    assert_eq!(vad.threshold(), 0.7);
}

#[test]
fn vad_silence_below_threshold_returns_silence() {
    let mut vad = VoiceActivityDetector::new_energy();
    // Near-silent chunk — RMS ≈ 0.01, well below 0.5
    let quiet: Vec<f32> = vec![0.01f32; CHUNK_SIZE];
    for _ in 0..WINDOW_SIZE {
        assert_eq!(vad.detect(&quiet), VadDecision::Silence);
    }
}

#[test]
fn vad_loud_signal_above_threshold_returns_speech() {
    let mut vad = VoiceActivityDetector::new_energy();
    // Fill the window with loud frames
    for _ in 0..WINDOW_SIZE {
        vad.detect(&loud_chunk());
    }
    assert_eq!(vad.detect(&loud_chunk()), VadDecision::Speech);
}

#[test]
fn vad_rolling_window_requires_3_of_5_frames() {
    let mut vad = VoiceActivityDetector::new_energy();
    // 2 speech, 2 silence → not enough yet
    vad.detect(&loud_chunk());   // speech
    vad.detect(&loud_chunk());   // speech
    vad.detect(&silence_chunk()); // silence
    vad.detect(&silence_chunk()); // silence
    let result = vad.detect(&silence_chunk()); // silence → 2/5 → Silence
    assert_eq!(result, VadDecision::Silence);
}

#[test]
fn vad_rolling_window_3_of_5_confirms_speech() {
    let mut vad = VoiceActivityDetector::new_energy();
    vad.detect(&loud_chunk());    // speech
    vad.detect(&loud_chunk());    // speech
    vad.detect(&loud_chunk());    // speech
    vad.detect(&silence_chunk()); // silence
    let result = vad.detect(&silence_chunk()); // silence → 3/5 → Speech
    assert_eq!(result, VadDecision::Speech);
}

#[test]
fn vad_window_size_is_five() {
    let mut vad = VoiceActivityDetector::new_energy();
    assert_eq!(vad.window_len(), 0);
    for i in 1..=7 {
        vad.detect(&silence_chunk());
        assert_eq!(vad.window_len(), i.min(5), "frame {i}");
    }
}

#[test]
fn vad_window_snapshot_reflects_decisions() {
    let mut vad = VoiceActivityDetector::new_energy();
    vad.detect(&loud_chunk());    // speech (true)
    vad.detect(&silence_chunk()); // silence (false)
    vad.detect(&loud_chunk());    // speech (true)
    let snap = vad.window_snapshot();
    assert_eq!(snap, vec![true, false, true]);
}

#[test]
fn vad_reset_clears_window_and_state() {
    let mut vad = VoiceActivityDetector::new_energy();
    for _ in 0..5 {
        vad.detect(&loud_chunk());
    }
    assert_eq!(vad.window_len(), 5);
    vad.reset();
    assert_eq!(vad.window_len(), 0);
}

#[test]
fn vad_reset_resets_accumulated_speech() {
    let mut vad = VoiceActivityDetector::new_energy();
    // Fill window with speech
    for _ in 0..5 { vad.detect(&loud_chunk()); }
    vad.reset();
    // After reset, a single silence frame should be Silence (0/1 in window)
    assert_eq!(vad.detect(&silence_chunk()), VadDecision::Silence);
}

#[test]
fn vad_transition_speech_to_silence() {
    let mut vad = VoiceActivityDetector::new_energy();
    // Fill window with speech
    for _ in 0..5 { vad.detect(&loud_chunk()); }
    assert_eq!(vad.detect(&loud_chunk()), VadDecision::Speech);

    // Flood with silence — after 3 silence frames the vote flips
    for _ in 0..3 { vad.detect(&silence_chunk()); }
    // Window: [speech, speech, silence, silence, silence] → 2 speech < 3
    assert_eq!(vad.detect(&silence_chunk()), VadDecision::Silence);
}

#[test]
fn vad_calibrate_raises_threshold_above_noise_floor() {
    let mut vad = VoiceActivityDetector::new_energy();
    // Noise at RMS ≈ 0.1
    let noise: Vec<f32> = vec![0.1f32; CHUNK_SIZE * 4];
    vad.calibrate(&noise, 2.0);
    // Threshold should be ≈ 0.1 * 2.0 = 0.2
    let t = vad.threshold();
    assert!(t > 0.15 && t < 0.25, "expected ≈ 0.2, got {t}");
}

#[test]
fn vad_calibrate_empty_noise_is_noop() {
    let mut vad = VoiceActivityDetector::new_energy();
    let original = vad.threshold();
    vad.calibrate(&[], 2.0);
    assert_eq!(vad.threshold(), original);
}

#[test]
fn vad_calibrate_clamps_threshold_to_1() {
    let mut vad = VoiceActivityDetector::new_energy();
    // Loud noise (RMS 0.9) × scale 5.0 would overflow — must clamp to 1.0
    let loud_noise: Vec<f32> = vec![0.9f32; CHUNK_SIZE * 2];
    vad.calibrate(&loud_noise, 5.0);
    assert!(vad.threshold() <= 1.0);
}

#[test]
fn vad_short_chunk_does_not_panic() {
    let mut vad = VoiceActivityDetector::new_energy();
    // Shorter-than-standard chunk (128 samples) should not panic
    let short: Vec<f32> = vec![1.0; 128];
    let _ = vad.detect(&short);
}

#[test]
fn vad_from_model_without_feature_returns_energy_detector() {
    // Without the neural-vad feature, from_model() returns the energy backend.
    let result = VoiceActivityDetector::from_model("nonexistent_path.onnx");
    // Should succeed (energy fallback) and behave normally
    assert!(result.is_ok());
    let mut vad = result.unwrap();
    assert_eq!(vad.threshold(), DEFAULT_THRESHOLD);
    vad.detect(&silence_chunk()); // must not panic
}

// Reuse the named constant inside tests to avoid magic numbers.
const WINDOW_SIZE: usize = 5;

// ─── Synthetic audio helpers ──────────────────────────────────────────────────

/// Generates a pure sine wave at `freq_hz` with the given peak amplitude.
/// RMS of a sine = amplitude / √2.
fn make_sine(freq_hz: f32, amplitude: f32) -> Vec<f32> {
    const SR: f32 = 16_000.0;
    (0..CHUNK_SIZE)
        .map(|i| amplitude * (2.0 * std::f32::consts::PI * freq_hz * i as f32 / SR).sin())
        .collect()
}

/// Speech-like signal: voiced-vowel harmonics in the vocal frequency range.
/// Each of 5 harmonics has amplitude 0.4; because they are at different
/// frequencies their RMS adds in quadrature:
///   RMS = 0.4 × √(5/2) ≈ 0.632 — well above the 0.5 threshold.
fn make_speech_like() -> Vec<f32> {
    let fundamentals = [200.0f32, 400.0, 800.0, 1600.0, 3200.0];
    let amplitude_per_harmonic = 0.40f32;
    (0..CHUNK_SIZE)
        .map(|i| {
            fundamentals
                .iter()
                .map(|&f| {
                    amplitude_per_harmonic
                        * (2.0 * std::f32::consts::PI * f * i as f32 / 16_000.0).sin()
                })
                .sum::<f32>()
        })
        .collect()
}

/// Speech with additive background noise (applause / HVAC hum typical of
/// Nigerian church environments).
fn make_noisy_speech() -> Vec<f32> {
    let speech = make_speech_like();
    let noise_amplitude = 0.05f32;
    // Deterministic pseudo-noise — no external rand crate needed.
    speech
        .iter()
        .enumerate()
        .map(|(i, &s)| {
            let noise = noise_amplitude * ((i as f32 * 1.7321 + 0.5).fract() * 2.0 - 1.0);
            s + noise
        })
        .collect()
}

fn rms_of(samples: &[f32]) -> f32 {
    let sq: f32 = samples.iter().map(|s| s * s).sum();
    (sq / samples.len() as f32).sqrt()
}

// ─── Synthetic audio tests ────────────────────────────────────────────────────

#[test]
fn synthetic_pure_silence_is_silence() {
    // All-zero signal: RMS = 0.0, well below the 0.5 threshold.
    let mut vad = VoiceActivityDetector::new_energy();
    let chunk = silence_chunk();
    assert_eq!(rms_of(&chunk), 0.0);
    for _ in 0..WINDOW_SIZE {
        assert_eq!(vad.detect(&chunk), VadDecision::Silence);
    }
}

#[test]
fn synthetic_pure_tone_below_threshold_is_silence() {
    // A 440 Hz sine at amplitude 0.5 has RMS ≈ 0.354, below the 0.5 threshold.
    // The energy backend classifies by amplitude only; the neural backend
    // (--features neural-vad) goes further and rejects ALL pure tones as
    // non-speech regardless of amplitude because Silero VAD is trained on
    // human speech formants, not sinusoids.
    let mut vad = VoiceActivityDetector::new_energy();
    let tone = make_sine(440.0, 0.5);
    let tone_rms = rms_of(&tone);
    // Sanity check: confirm the tone really is below threshold before asserting.
    assert!(
        tone_rms < DEFAULT_THRESHOLD,
        "tone RMS {tone_rms:.3} should be below threshold {DEFAULT_THRESHOLD}"
    );
    for _ in 0..WINDOW_SIZE {
        assert_eq!(
            vad.detect(&tone),
            VadDecision::Silence,
            "pure 440 Hz tone should not be classified as speech"
        );
    }
}

#[test]
fn synthetic_pure_tone_amplitude_is_as_expected() {
    // Mathematical sanity: RMS of sin(x) * A = A / sqrt(2).
    let tone = make_sine(440.0, 0.5);
    let expected_rms = 0.5 / 2.0f32.sqrt();
    let actual_rms = rms_of(&tone);
    assert!(
        (actual_rms - expected_rms).abs() < 0.01,
        "expected RMS ≈ {expected_rms:.3}, got {actual_rms:.3}"
    );
}

#[test]
fn synthetic_speech_sample_is_speech() {
    // Multi-harmonic speech-like signal: RMS should be well above 0.5.
    let mut vad = VoiceActivityDetector::new_energy();
    let speech = make_speech_like();
    let speech_rms = rms_of(&speech);
    assert!(
        speech_rms > DEFAULT_THRESHOLD,
        "speech RMS {speech_rms:.3} should exceed threshold"
    );
    // Prime the window with speech frames.
    for _ in 0..WINDOW_SIZE {
        vad.detect(&speech);
    }
    assert_eq!(vad.detect(&speech), VadDecision::Speech);
}

#[test]
fn synthetic_speech_with_background_noise_is_speech() {
    // Speech + environmental noise (HVAC, crowd murmur).
    // Combined RMS should stay above the threshold so detection is not degraded.
    let mut vad = VoiceActivityDetector::new_energy();
    let noisy_speech = make_noisy_speech();
    let combined_rms = rms_of(&noisy_speech);
    assert!(
        combined_rms > DEFAULT_THRESHOLD,
        "noisy speech RMS {combined_rms:.3} should still exceed threshold"
    );
    for _ in 0..WINDOW_SIZE {
        vad.detect(&noisy_speech);
    }
    assert_eq!(
        vad.detect(&noisy_speech),
        VadDecision::Speech,
        "speech should remain detectable with background noise"
    );
}

#[test]
fn synthetic_speech_survives_calibration_with_realistic_noise() {
    // A common failure mode: calibration sets the threshold too high,
    // causing speech to be classified as silence.
    // scale_factor=2.0 should place the threshold above noise but below speech.
    let noise = make_sine(60.0, 0.06); // 60 Hz HVAC hum, RMS ≈ 0.042
    let mut vad = VoiceActivityDetector::new_energy();
    vad.calibrate(&noise.repeat(8), 2.0);

    let threshold_after_calibration = vad.threshold();
    let speech = make_speech_like();
    let speech_rms = rms_of(&speech);

    assert!(
        speech_rms > threshold_after_calibration,
        "speech RMS {speech_rms:.3} must exceed calibrated threshold {threshold_after_calibration:.3}"
    );

    for _ in 0..WINDOW_SIZE {
        vad.detect(&speech);
    }
    assert_eq!(
        vad.detect(&speech),
        VadDecision::Speech,
        "speech must still be detected after calibration"
    );
}

// ─── Performance test ─────────────────────────────────────────────────────────

#[test]
fn vad_detect_under_1ms_per_chunk() {
    // Budget: 1 ms per 512-sample chunk at 16 kHz = 32 ms of audio.
    // The energy backend is O(CHUNK_SIZE) arithmetic; must be well under budget.
    // The neural backend (--features neural-vad) has a separate benchmark target.
    let mut vad = VoiceActivityDetector::new_energy();
    let chunk = make_speech_like();

    // Warm up: fill cache, JIT (in case of LLVM lazy codegen).
    for _ in 0..200 {
        let _ = vad.detect(&chunk);
    }
    vad.reset();

    const ITERS: u32 = 10_000;
    let start = std::time::Instant::now();
    for _ in 0..ITERS {
        let _ = vad.detect(&chunk);
    }
    let total = start.elapsed();

    let per_chunk_us = total.as_micros() as f64 / ITERS as f64;
    let per_chunk_ns = total.as_nanos() as f64 / ITERS as f64;

    assert!(
        per_chunk_us < 1_000.0,
        "detect() averaged {per_chunk_us:.2} µs — exceeds 1 ms budget"
    );

    // Informational (visible with `cargo test -- --nocapture`).
    println!(
        "vad::detect() energy backend: {per_chunk_ns:.0} ns/chunk \
         ({per_chunk_us:.2} µs) over {ITERS} iterations"
    );
}

// ─── NoiseGate ────────────────────────────────────────────────────────────────

#[test]
fn noise_gate_zeros_chunk_below_threshold() {
    let gate = NoiseGate::new(0.1);
    // RMS of 0.05 constant signal = 0.05, below 0.1
    let mut samples = vec![0.05f32; 64];
    gate.process(&mut samples);
    assert!(samples.iter().all(|&s| s == 0.0), "chunk should be zeroed");
}

#[test]
fn noise_gate_passes_chunk_above_threshold() {
    let gate = NoiseGate::new(0.1);
    // RMS of 0.5 constant signal = 0.5, above 0.1
    let original = vec![0.5f32; 64];
    let mut samples = original.clone();
    gate.process(&mut samples);
    assert_eq!(samples, original, "chunk above threshold must pass through");
}

#[test]
fn noise_gate_threshold_at_boundary_passes() {
    // Exactly at threshold: RMS = threshold → NOT gated (< is strict).
    let gate = NoiseGate::new(0.3);
    let mut samples = vec![0.3f32; 64];
    let original = samples.clone();
    gate.process(&mut samples);
    assert_eq!(samples, original);
}

#[test]
fn noise_gate_set_threshold_updates_correctly() {
    let mut gate = NoiseGate::new(0.1);
    assert_eq!(gate.threshold(), 0.1);
    gate.set_threshold(0.5);
    assert_eq!(gate.threshold(), 0.5);
}

#[test]
fn noise_gate_threshold_clamped_to_unit_range() {
    let mut gate = NoiseGate::new(0.5);
    gate.set_threshold(2.0);
    assert_eq!(gate.threshold(), 1.0);
    gate.set_threshold(-0.5);
    assert_eq!(gate.threshold(), 0.0);
}

#[test]
fn noise_gate_would_gate_predicate_matches_process() {
    let gate = NoiseGate::new(0.2);
    let quiet = vec![0.05f32; 64];
    let loud = vec![0.8f32; 64];
    assert!(gate.would_gate(&quiet));
    assert!(!gate.would_gate(&loud));
}

#[test]
fn noise_gate_empty_slice_does_not_panic() {
    let gate = NoiseGate::new(0.1);
    let mut empty: Vec<f32> = Vec::new();
    gate.process(&mut empty); // must not panic
}

// ─── AudioPreprocessor ────────────────────────────────────────────────────────

fn full_frame(value: f32) -> Vec<f32> {
    vec![value; RNNOISE_FRAME_SIZE]
}

#[test]
fn preprocessor_process_exact_frame_returns_same_length() {
    let mut p = AudioPreprocessor::new();
    let input = full_frame(0.5);
    let output = p.process(&input);
    assert_eq!(output.len(), RNNOISE_FRAME_SIZE);
}

#[test]
fn preprocessor_output_is_normalised_range() {
    // RNNoise output should be in [-1, 1] after our PCM rescaling.
    let mut p = AudioPreprocessor::new();
    let input = full_frame(0.9);
    let output = p.process(&input);
    for &s in &output {
        assert!(s.abs() <= 1.0, "sample {s} out of [-1, 1]");
    }
}

#[test]
fn preprocessor_reduces_noise_amplitude() {
    // Feed pure constant noise; RNNoise should attenuate it.
    let mut p = AudioPreprocessor::new();
    // Send several frames to warm up the RNN state.
    for _ in 0..5 {
        p.process(&full_frame(0.3));
    }
    let input = full_frame(0.3);
    let input_rms = 0.3f32;
    let output = p.process(&input);
    let output_rms = {
        let sq: f32 = output.iter().map(|s| s * s).sum();
        (sq / output.len() as f32).sqrt()
    };
    assert!(
        output_rms < input_rms,
        "RNNoise should reduce constant noise: input_rms={input_rms:.3}, output_rms={output_rms:.3}"
    );
}

#[test]
fn preprocessor_short_chunk_is_buffered() {
    // Feeding fewer than RNNOISE_FRAME_SIZE samples returns nothing yet.
    let mut p = AudioPreprocessor::new();
    let partial = vec![0.5f32; RNNOISE_FRAME_SIZE - 1];
    let out = p.process(&partial);
    assert!(out.is_empty(), "partial frame should be buffered, not returned");
}

#[test]
fn preprocessor_flush_returns_remaining_samples() {
    let mut p = AudioPreprocessor::new();
    let partial = vec![0.4f32; 100]; // less than RNNOISE_FRAME_SIZE
    p.process(&partial);
    let flushed = p.flush();
    assert_eq!(
        flushed.len(),
        RNNOISE_FRAME_SIZE,
        "flush should pad to a full frame and return it"
    );
}

#[test]
fn preprocessor_flush_on_empty_buffer_is_noop() {
    let mut p = AudioPreprocessor::new();
    let out = p.flush();
    assert!(out.is_empty());
}

#[test]
fn preprocessor_multi_chunk_total_length() {
    // 3 frames fed as a single large slice → 3 frames returned.
    let mut p = AudioPreprocessor::new();
    let input = vec![0.2f32; RNNOISE_FRAME_SIZE * 3];
    let output = p.process(&input);
    assert_eq!(output.len(), RNNOISE_FRAME_SIZE * 3);
}

#[test]
fn preprocessor_with_gate_zeros_silent_frames() {
    // A gate threshold of 0.5 should zero a frame whose RMS ≈ 0.05.
    let mut p = AudioPreprocessor::with_gate(0.5);
    // RNNoise will attenuate 0.05 input → gate should then suppress it.
    let quiet = vec![0.05f32; RNNOISE_FRAME_SIZE];
    let output = p.process(&quiet);
    assert!(
        output.iter().all(|&s| s == 0.0),
        "gate should suppress near-silent frame after denoising"
    );
}

#[test]
fn preprocessor_gate_threshold_configured_correctly() {
    let mut p = AudioPreprocessor::with_gate(0.1);
    assert_eq!(p.gate_threshold(), Some(0.1));
    p.set_gate_threshold(0.3);
    assert_eq!(p.gate_threshold(), Some(0.3));
    p.disable_gate();
    assert_eq!(p.gate_threshold(), None);
}

#[test]
fn preprocessor_new_has_no_gate() {
    let p = AudioPreprocessor::new();
    assert_eq!(p.gate_threshold(), None);
}

// ─── Performance ──────────────────────────────────────────────────────────────

#[test]
fn preprocessor_process_under_10ms_per_frame() {
    // Budget: RNNoise frame is 480 samples.  At 16 kHz that is 30 ms of audio.
    // We require processing to complete in < 10 ms (≥ 3× real-time headroom).
    let mut p = AudioPreprocessor::new();
    let frame = full_frame(0.3);

    // Warm up.
    for _ in 0..50 {
        p.process(&frame);
    }

    const ITERS: u32 = 2_000;
    let start = std::time::Instant::now();
    for _ in 0..ITERS {
        p.process(&frame);
    }
    let total = start.elapsed();

    let per_frame_us = total.as_micros() as f64 / ITERS as f64;
    assert!(
        per_frame_us < 10_000.0,
        "process() averaged {per_frame_us:.1} µs — exceeds 10 ms budget"
    );

    println!(
        "AudioPreprocessor::process() RNNoise: {per_frame_us:.0} µs/frame over {ITERS} iters"
    );
}
