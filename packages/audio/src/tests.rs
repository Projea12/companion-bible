use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::device::{infer_device_type, AudioDevice, DeviceType};
use crate::error::AudioError;
use crate::input::AudioInput;
use crate::ring_buffer::{RingBuffer, DEFAULT_CAPACITY};
use crate::preprocess::{
    AmplitudeNormalizer, AudioPipeline, AudioPreprocessor, NoiseGate, NoiseSuppressor,
    CHUNK_100MS, RNNOISE_FRAME_SIZE,
};
use crate::vad::{VadDecision, VoiceActivityDetector, CHUNK_SIZE, DEFAULT_THRESHOLD};
use crate::capture::{AudioCapture, CaptureConfig, CaptureEvent};

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

// ─── NoiseSuppressor ──────────────────────────────────────────────────────────

#[test]
fn noise_suppressor_chunk_100ms_constant() {
    assert_eq!(CHUNK_100MS, 1600);
}

#[test]
fn noise_suppressor_returns_same_length_as_input() {
    let mut ns = NoiseSuppressor::new();
    let chunk = vec![0.3f32; CHUNK_100MS];
    let out = ns.process(&chunk);
    assert_eq!(out.len(), CHUNK_100MS, "output length must match input");
}

#[test]
fn noise_suppressor_returns_same_length_for_short_chunk() {
    let mut ns = NoiseSuppressor::new();
    let chunk = vec![0.3f32; 100];
    let out = ns.process(&chunk);
    assert_eq!(out.len(), 100);
}

#[test]
fn noise_suppressor_returns_same_length_for_exact_rnnoise_frame() {
    let mut ns = NoiseSuppressor::new();
    let chunk = vec![0.4f32; RNNOISE_FRAME_SIZE];
    let out = ns.process(&chunk);
    assert_eq!(out.len(), RNNOISE_FRAME_SIZE);
}

#[test]
fn noise_suppressor_output_is_normalised_range() {
    let mut ns = NoiseSuppressor::new();
    for _ in 0..3 {
        ns.process(&vec![0.6f32; CHUNK_100MS]);
    }
    let out = ns.process(&vec![0.6f32; CHUNK_100MS]);
    for &s in &out {
        assert!(s.abs() <= 1.0, "sample {s} outside [-1, 1]");
    }
}

#[test]
fn noise_suppressor_reduces_constant_noise() {
    let mut ns = NoiseSuppressor::new();
    // Warm up RNN state.
    for _ in 0..5 {
        ns.process(&vec![0.3f32; CHUNK_100MS]);
    }
    let input_rms = 0.3f32;
    let out = ns.process(&vec![0.3f32; CHUNK_100MS]);
    let out_rms = rms_of(&out);
    assert!(
        out_rms < input_rms,
        "NoiseSuppressor should attenuate constant noise: {input_rms:.3} → {out_rms:.3}"
    );
}

#[test]
fn noise_suppressor_flush_after_process_is_empty() {
    // process() eagerly drains staging, so flush() afterward returns nothing.
    let mut ns = NoiseSuppressor::new();
    let partial = vec![0.2f32; 200];
    let out = ns.process(&partial);
    // process() returns same length — staging was already drained internally.
    assert_eq!(out.len(), 200);
    let flushed = ns.flush();
    assert!(flushed.is_empty(), "nothing left in staging after eager-flush process()");
}

// ─── AmplitudeNormalizer ──────────────────────────────────────────────────────

#[test]
fn normalizer_new_gain_starts_at_one() {
    let n = AmplitudeNormalizer::new(0.1);
    assert_eq!(n.current_gain(), 1.0);
}

#[test]
fn normalizer_target_rms_stored_correctly() {
    let n = AmplitudeNormalizer::new(0.2);
    assert_eq!(n.target_rms(), 0.2);
}

#[test]
fn normalizer_target_rms_clamped() {
    let n = AmplitudeNormalizer::new(1.5);
    assert_eq!(n.target_rms(), 1.0);
}

#[test]
fn normalizer_does_not_modify_silence() {
    // Silence (RMS < 1e-6) must pass through unchanged.
    let mut n = AmplitudeNormalizer::new(0.1);
    let mut samples = vec![0.0f32; 512];
    n.process(&mut samples);
    assert!(samples.iter().all(|&s| s == 0.0), "silence should not be modified");
}

#[test]
fn normalizer_boosts_quiet_signal_toward_target() {
    let target = 0.3f32;
    let mut n = AmplitudeNormalizer::new(target);
    // Input at RMS ≈ 0.05 (much quieter than target).
    let mut samples = vec![0.05f32; 1024];
    // Run many iterations to let the IIR converge.
    for _ in 0..50 {
        n.process(&mut samples);
        samples = vec![0.05f32; 1024]; // reset input each time
    }
    let out_rms = rms_of(&samples);
    // After convergence the gain should bring output above the initial RMS.
    assert!(
        out_rms > 0.05,
        "normalizer should boost quiet signal; got rms {out_rms:.4}"
    );
}

#[test]
fn normalizer_attenuates_loud_signal_toward_target() {
    let target = 0.1f32;
    let mut n = AmplitudeNormalizer::new(target);
    let input_amplitude = 0.9f32;
    let mut samples = vec![input_amplitude; 1024];
    for _ in 0..50 {
        n.process(&mut samples);
        samples = vec![input_amplitude; 1024];
    }
    let out_rms = rms_of(&samples);
    assert!(
        out_rms < input_amplitude,
        "normalizer should attenuate loud signal; got rms {out_rms:.4}"
    );
}

#[test]
fn normalizer_gain_changes_smoothly() {
    // Gain must change gradually, not instantaneously.
    let mut n = AmplitudeNormalizer::new(0.5);
    // Start with a very quiet signal → large target gain.
    let quiet = vec![0.01f32; 512];
    let mut samples = quiet.clone();
    n.process(&mut samples);
    let gain_after_1 = n.current_gain();

    samples = quiet.clone();
    n.process(&mut samples);
    let gain_after_2 = n.current_gain();

    // Gain should increase over successive calls (moving toward target).
    assert!(gain_after_2 > gain_after_1, "gain must increase smoothly");
    // But it must not jump all the way in one step (smoothing < 1.0; default max_gain = 10.0).
    assert!(gain_after_1 < 10.0, "gain must not jump to max in one step");
}

#[test]
fn normalizer_output_clamped_to_minus_one_plus_one() {
    // Even with a large max_gain, output samples must be in [-1, 1].
    let mut n = AmplitudeNormalizer::new(0.9);
    n.set_max_gain(100.0);
    let mut samples = vec![0.01f32; 512];
    for _ in 0..200 {
        n.process(&mut samples);
        samples = vec![0.01f32; 512];
    }
    for &s in &samples {
        assert!(s.abs() <= 1.0, "output {s} exceeded [-1, 1]");
    }
}

#[test]
fn normalizer_set_smoothing_affects_convergence_rate() {
    // A higher smoothing coefficient → faster convergence.
    let target = 0.5f32;
    let input_amplitude = 0.05f32;

    let gain_after_one_step = |smoothing: f32| -> f32 {
        let mut n = AmplitudeNormalizer::new(target);
        n.set_smoothing(smoothing);
        let mut samples = vec![input_amplitude; 512];
        n.process(&mut samples);
        n.current_gain()
    };

    let slow = gain_after_one_step(0.01);
    let fast = gain_after_one_step(0.5);
    assert!(fast > slow, "higher smoothing should yield faster gain increase; slow={slow:.3} fast={fast:.3}");
}

// ─── AudioPipeline ────────────────────────────────────────────────────────────

#[test]
fn pipeline_returns_same_length_as_input() {
    let mut p = AudioPipeline::new(0.02, 0.1);
    let chunk = vec![0.5f32; CHUNK_100MS];
    let out = p.process(&chunk);
    assert_eq!(out.len(), CHUNK_100MS);
}

#[test]
fn pipeline_gates_silent_input() {
    // Gate threshold 0.5 — a near-silent chunk (0.01) should be zeroed by gate
    // before reaching the suppressor and normalizer.
    let mut p = AudioPipeline::new(0.5, 0.1);
    let quiet = vec![0.01f32; CHUNK_100MS];
    let out = p.process(&quiet);
    // After gating at 0.5 the chunk is all-zeros → normalizer passes zeros through.
    assert!(
        out.iter().all(|&s| s == 0.0),
        "pipeline should gate and zero near-silent input"
    );
}

#[test]
fn pipeline_passes_loud_speech_through() {
    // Speech-like input well above gate threshold should come out non-silent.
    let mut p = AudioPipeline::new(0.02, 0.3);
    let speech = make_speech_like();
    // Warm up RNN state.
    for _ in 0..3 {
        p.process(&speech);
    }
    let out = p.process(&speech);
    let out_rms = rms_of(&out);
    assert!(out_rms > 0.0, "speech should survive the pipeline");
}

#[test]
fn pipeline_output_in_normalised_range() {
    let mut p = AudioPipeline::new(0.02, 0.3);
    let speech = make_speech_like();
    for _ in 0..5 {
        let out = p.process(&speech);
        for &s in &out {
            assert!(s.abs() <= 1.0, "pipeline output {s} outside [-1, 1]");
        }
    }
}

#[test]
fn pipeline_gate_mut_updates_threshold() {
    let mut p = AudioPipeline::new(0.05, 0.1);
    p.gate_mut().set_threshold(0.2);
    assert_eq!(p.gate_mut().threshold(), 0.2);
}

#[test]
fn pipeline_normalizer_mut_updates_target_rms() {
    let mut p = AudioPipeline::new(0.05, 0.1);
    p.normalizer_mut().set_target_rms(0.4);
    assert_eq!(p.normalizer_mut().target_rms(), 0.4);
}

// ─── Noisy audio sample tests ─────────────────────────────────────────────────
//
// Each test models a real-world interference source encountered in Nigerian
// church environments and verifies that the pipeline attenuates it.
//
// "Attenuation" is measured as output_rms < input_rms after a warm-up period
// that lets RNNoise's RNN state converge.  We do not assert a specific dB
// reduction because nnnoiseless model quality varies slightly across platforms.

// ── Generator hum ─────────────────────────────────────────────────────────────

/// 50 Hz mains / generator hum at three harmonic levels typical of Nigerian
/// petrol generators feeding an unfiltered PA system.
fn make_generator_hum(amplitude: f32) -> Vec<f32> {
    const SR: f32 = 16_000.0;
    // Fundamental 50 Hz + 2nd (100 Hz) + 3rd (150 Hz) harmonics.
    let harmonics = [(50.0f32, 1.0f32), (100.0, 0.6), (150.0, 0.3)];
    (0..CHUNK_100MS)
        .map(|i| {
            harmonics
                .iter()
                .map(|&(f, rel)| {
                    amplitude * rel * (2.0 * std::f32::consts::PI * f * i as f32 / SR).sin()
                })
                .sum::<f32>()
        })
        .collect()
}

#[test]
fn generator_hum_rms_is_measurable() {
    // Sanity: ensure the synthesised hum has non-trivial energy before testing attenuation.
    let hum = make_generator_hum(0.15);
    let r = rms_of(&hum);
    assert!(r > 0.05, "hum RMS {r:.4} too low — generator synthesis broken");
}

#[test]
fn noise_suppressor_attenuates_generator_hum() {
    let mut ns = NoiseSuppressor::new();
    let hum = make_generator_hum(0.15);
    let input_rms = rms_of(&hum);

    // Warm up: let RNNoise learn the steady-state hum.
    for _ in 0..10 {
        ns.process(&hum);
    }
    let out = ns.process(&hum);
    let output_rms = rms_of(&out);

    assert!(
        output_rms < input_rms,
        "RNNoise should suppress generator hum: {input_rms:.4} → {output_rms:.4}"
    );
}

#[test]
fn pipeline_attenuates_generator_hum_below_gate() {
    // Gate threshold 0.3 is above a 0.15-amplitude hum after RNNoise suppression.
    // After warm-up the suppressor should reduce the hum below the gate threshold.
    let mut p = AudioPipeline::new(0.3, 0.1);
    let hum = make_generator_hum(0.15);

    // Warm up.
    for _ in 0..15 {
        p.process(&hum);
    }
    let out = p.process(&hum);
    let output_rms = rms_of(&out);

    assert!(
        output_rms < rms_of(&hum),
        "pipeline should reduce generator hum; input_rms={:.4} output_rms={output_rms:.4}",
        rms_of(&hum)
    );
}

#[test]
fn speech_plus_hum_preserves_speech_energy() {
    // When speech is mixed with generator hum the pipeline must not suppress the
    // speech below the gate — verifies the gate is tuned to hum amplitude, not
    // speech amplitude.
    let speech = make_speech_like();
    let hum = make_generator_hum(0.05); // hum at 1/8 the speech RMS

    // Mix: speech dominates.
    let mixed: Vec<f32> = speech.iter().zip(hum.iter()).map(|(s, h)| s + h).collect();
    let input_rms = rms_of(&mixed);

    // Gate just above hum level.
    let mut p = AudioPipeline::new(0.08, 0.2);
    for _ in 0..5 {
        p.process(&mixed);
    }
    let out = p.process(&mixed);
    let output_rms = rms_of(&out);

    assert!(
        output_rms > 0.0,
        "speech must survive when mixed with hum (input_rms={input_rms:.4}, output_rms={output_rms:.4})"
    );
}

// ── Crowd noise ───────────────────────────────────────────────────────────────

/// Broadband crowd noise: band-limited white noise approximated by summing
/// many incoherent sinusoids spread across 200 Hz – 4 kHz (congregation murmur).
fn make_crowd_noise(amplitude: f32) -> Vec<f32> {
    const SR: f32 = 16_000.0;
    // 20 incoherent components with deterministic pseudo-random phases.
    let freqs: Vec<f32> = (0..20)
        .map(|i| 200.0 + i as f32 * 190.0) // 200, 390, 580, … Hz
        .collect();
    (0..CHUNK_100MS)
        .map(|i| {
            freqs
                .iter()
                .enumerate()
                .map(|(k, &f)| {
                    // Deterministic per-component phase offset avoids coherent cancellation.
                    let phase = k as f32 * 1.2345;
                    amplitude / (freqs.len() as f32).sqrt()
                        * (2.0 * std::f32::consts::PI * f * i as f32 / SR + phase).sin()
                })
                .sum::<f32>()
        })
        .collect()
}

#[test]
fn crowd_noise_rms_is_measurable() {
    let noise = make_crowd_noise(0.2);
    let r = rms_of(&noise);
    assert!(r > 0.02, "crowd noise RMS {r:.4} — synthesis broken");
}

#[test]
fn noise_suppressor_attenuates_crowd_noise() {
    let mut ns = NoiseSuppressor::new();
    let noise = make_crowd_noise(0.2);
    let input_rms = rms_of(&noise);

    for _ in 0..10 {
        ns.process(&noise);
    }
    let out = ns.process(&noise);
    let output_rms = rms_of(&out);

    assert!(
        output_rms < input_rms,
        "RNNoise should suppress crowd noise: {input_rms:.4} → {output_rms:.4}"
    );
}

#[test]
fn speech_survives_crowd_noise_after_suppression() {
    // The pastor's voice (RMS ≈ 0.63) should remain detectable even when mixed
    // with crowd noise at 25% of speech amplitude.
    let speech = make_speech_like();
    let noise = make_crowd_noise(0.1);
    let mixed: Vec<f32> = speech.iter().zip(noise.iter()).map(|(s, n)| s + n).collect();

    let mut p = AudioPipeline::new(0.02, 0.2);
    for _ in 0..5 {
        p.process(&mixed);
    }
    let out = p.process(&mixed);

    assert!(
        rms_of(&out) > 0.0,
        "speech must survive crowd noise through the pipeline"
    );
}

#[test]
fn gate_suppresses_crowd_noise_alone() {
    // Crowd noise alone (no speech) should be below the gate threshold after
    // RNNoise suppression.  Gate at 0.15, noise at 0.2 → RNNoise should
    // reduce noise RMS below 0.15 after warm-up.
    let mut p = AudioPipeline::new(0.15, 0.1);
    let noise = make_crowd_noise(0.2);

    for _ in 0..15 {
        p.process(&noise);
    }
    let out = p.process(&noise);
    let output_rms = rms_of(&out);

    assert!(
        output_rms < rms_of(&noise),
        "crowd noise should be attenuated by pipeline; noise_rms={:.4} output_rms={output_rms:.4}",
        rms_of(&noise)
    );
}

// ── Reverb handling ───────────────────────────────────────────────────────────

/// Simple first-order feedback comb filter — approximates early reflections in
/// a large concrete church hall (common in West African church buildings).
///
/// `delay_samples` controls the reflection delay; `decay` is the per-bounce
/// amplitude reduction (< 1.0 for stability).
fn apply_reverb(input: &[f32], delay_samples: usize, decay: f32) -> Vec<f32> {
    let mut out = input.to_vec();
    for i in delay_samples..out.len() {
        out[i] += out[i - delay_samples] * decay;
    }
    // Normalise to prevent clipping.
    let peak = out.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
    if peak > 1.0 {
        out.iter_mut().for_each(|s| *s /= peak);
    }
    out
}

#[test]
fn reverb_produces_nonzero_output_and_changes_waveform() {
    // Sanity: apply_reverb must produce non-zero output and alter the waveform
    // (peak-normalization means RMS is not a reliable energy proxy here).
    let speech = make_speech_like();
    let reverbed = apply_reverb(&speech, 160, 0.4); // 10 ms delay at 16 kHz

    assert!(rms_of(&reverbed) > 0.0, "reverbed signal must be non-zero");
    // At least some samples should differ from the original (reflections added).
    let diffs = speech.iter().zip(reverbed.iter()).filter(|(&a, &b)| (a - b).abs() > 1e-6).count();
    assert!(diffs > 0, "reverb should alter the waveform");
}

#[test]
fn noise_suppressor_handles_reverberant_speech_without_panic() {
    let mut ns = NoiseSuppressor::new();
    let reverbed = apply_reverb(&make_speech_like(), 160, 0.5);
    // Must not panic and must return same-length output.
    let out = ns.process(&reverbed);
    assert_eq!(out.len(), reverbed.len());
}

#[test]
fn noise_suppressor_output_in_range_for_reverberant_input() {
    let mut ns = NoiseSuppressor::new();
    let reverbed = apply_reverb(&make_speech_like(), 240, 0.6); // 15 ms delay
    for _ in 0..5 {
        ns.process(&reverbed);
    }
    let out = ns.process(&reverbed);
    for &s in &out {
        assert!(s.abs() <= 1.0, "reverberant output {s} outside [-1, 1]");
    }
}

#[test]
fn pipeline_does_not_silence_reverberant_speech() {
    // Reverberant speech must not be gated out — the reverb tail keeps the RMS
    // high enough to clear the gate threshold.
    // 20 ms delay, decay 0.5 — typical concrete hall reflection.
    let reverbed = apply_reverb(&make_speech_like(), 320, 0.5);
    let reverb_rms = rms_of(&reverbed);

    let mut p = AudioPipeline::new(0.02, 0.2);
    for _ in 0..5 {
        p.process(&reverbed);
    }
    let out = p.process(&reverbed);

    assert!(
        rms_of(&out) > 0.0,
        "reverberant speech (rms={reverb_rms:.4}) must not be silenced by pipeline"
    );
}

#[test]
fn pipeline_attenuates_reverb_tail_of_silence() {
    // After speech ends, the decaying reverb tail on silence should be suppressed.
    // Model: silence convolved with a decaying echo.
    let silence = vec![0.0f32; CHUNK_100MS];
    // A tiny impulse at the start creates a decaying echo — rest is silence.
    let mut with_echo = silence.clone();
    with_echo[0] = 0.3;
    let echo_tail = apply_reverb(&with_echo, 80, 0.5);

    let input_rms = rms_of(&echo_tail);
    let mut p = AudioPipeline::new(0.02, 0.1);
    for _ in 0..5 {
        p.process(&echo_tail);
    }
    let out = p.process(&silence); // pure silence after echo source stops
    let output_rms = rms_of(&out);

    assert!(
        output_rms <= input_rms,
        "pipeline should not amplify reverb tail; input={input_rms:.4} output={output_rms:.4}"
    );
}

// ─── Performance — full pipeline under 20 ms per chunk ───────────────────────

#[test]
fn pipeline_full_chain_under_20ms_per_100ms_chunk() {
    // Budget: 20 ms of wall time to process 100 ms of audio (5× real-time).
    // This leaves enough headroom for the main thread to run VAD and UI logic
    // concurrently on a mid-range ARM SoC (e.g. Raspberry Pi 4, M1 MacBook).
    let mut p = AudioPipeline::new(0.02, 0.2);
    let chunk = make_noisy_speech(); // realistic worst case: speech + noise

    // Extend to CHUNK_100MS so we exercise a full 100 ms processing cycle.
    let chunk: Vec<f32> = chunk
        .iter()
        .cloned()
        .cycle()
        .take(CHUNK_100MS)
        .collect();

    // Warm up: prime RNN and IIR state.
    for _ in 0..50 {
        p.process(&chunk);
    }

    const ITERS: u32 = 2_000;
    let start = std::time::Instant::now();
    for _ in 0..ITERS {
        p.process(&chunk);
    }
    let total = start.elapsed();

    let per_chunk_us = total.as_micros() as f64 / ITERS as f64;

    assert!(
        per_chunk_us < 20_000.0,
        "AudioPipeline::process() averaged {per_chunk_us:.1} µs — exceeds 20 ms budget"
    );

    println!(
        "AudioPipeline full chain (gate+suppress+normalize): {per_chunk_us:.0} µs/chunk \
         ({:.1} ms) over {ITERS} iters — budget 20 ms",
        per_chunk_us / 1_000.0
    );
}

#[test]
fn noise_suppressor_process_under_15ms_per_100ms_chunk() {
    // NoiseSuppressor alone (RNNoise only) must complete well within 15 ms
    // for a 100 ms chunk (1600 samples × 480-sample RNNoise frames = ~3 frames).
    let mut ns = NoiseSuppressor::new();
    let chunk: Vec<f32> = make_noisy_speech()
        .iter()
        .cloned()
        .cycle()
        .take(CHUNK_100MS)
        .collect();

    for _ in 0..50 {
        ns.process(&chunk);
    }

    const ITERS: u32 = 2_000;
    let start = std::time::Instant::now();
    for _ in 0..ITERS {
        ns.process(&chunk);
    }
    let total = start.elapsed();

    let per_chunk_us = total.as_micros() as f64 / ITERS as f64;

    assert!(
        per_chunk_us < 15_000.0,
        "NoiseSuppressor::process() averaged {per_chunk_us:.1} µs — exceeds 15 ms budget"
    );

    println!(
        "NoiseSuppressor (RNNoise only): {per_chunk_us:.0} µs/chunk over {ITERS} iters"
    );
}

// ─── AudioCapture — mock infrastructure ──────────────────────────────────────
//
// MockAudioInput is a synchronous stand-in for a real cpal device.
// The test holds shared flags that drive the mock's behaviour:
//   - driver.drive(samples): push samples through the registered callback
//     (simulates the cpal callback thread).
//   - fire_on_start: when true, the mock fires the callback once in start()
//     itself (simulates a device that produces audio immediately on reconnect).
//   - available: when false, start() returns an error (simulates a missing device).

struct MockAudioInput {
    callback: Arc<Mutex<Option<Box<dyn Fn(Vec<f32>) + Send + 'static>>>>,
    available: Arc<AtomicBool>,
    fire_on_start: Arc<AtomicBool>,
}

struct MockDriver {
    callback: Arc<Mutex<Option<Box<dyn Fn(Vec<f32>) + Send + 'static>>>>,
}

impl MockAudioInput {
    fn new() -> (Self, MockDriver) {
        let cb: Arc<Mutex<Option<Box<dyn Fn(Vec<f32>) + Send + 'static>>>> =
            Arc::new(Mutex::new(None));
        let driver = MockDriver { callback: Arc::clone(&cb) };
        let mock = Self {
            callback: cb,
            available: Arc::new(AtomicBool::new(true)),
            fire_on_start: Arc::new(AtomicBool::new(false)),
        };
        (mock, driver)
    }
}

impl MockDriver {
    fn drive(&self, samples: Vec<f32>) {
        if let Some(cb) = self.callback.lock().unwrap().as_ref() {
            cb(samples);
        }
    }
}

impl AudioInput for MockAudioInput {
    fn available_devices(&self) -> Result<Vec<AudioDevice>, crate::error::AudioError> {
        Ok(vec![])
    }

    fn select_device(&mut self, _: &str) -> Result<(), crate::error::AudioError> {
        Ok(())
    }

    fn start(
        &mut self,
        callback: Box<dyn Fn(Vec<f32>) + Send + 'static>,
    ) -> Result<(), crate::error::AudioError> {
        if !self.available.load(Ordering::Relaxed) {
            return Err(crate::error::AudioError::DeviceNotFound(
                "mock unavailable".into(),
            ));
        }
        if self.fire_on_start.load(Ordering::Relaxed) {
            // Simulate device producing audio immediately — used by reconnect tests.
            callback(vec![0.5f32; 512]);
        }
        *self.callback.lock().unwrap() = Some(callback);
        Ok(())
    }

    fn stop(&mut self) {
        *self.callback.lock().unwrap() = None;
    }

    fn current_level(&self) -> f32 {
        0.0
    }
}

/// Fast config for deterministic tests: 20 ms monitor ticks, 1-tick threshold,
/// 80 ms reconnect interval.
fn fast_config() -> CaptureConfig {
    CaptureConfig {
        zero_ticks_threshold: 1,
        monitor_interval: std::time::Duration::from_millis(20),
        reconnect_interval: std::time::Duration::from_millis(80),
    }
}

fn make_buffer() -> Arc<RingBuffer<f32>> {
    Arc::new(RingBuffer::new(65536))
}

// ─── AudioCapture — structural tests ─────────────────────────────────────────

#[test]
fn capture_new_is_not_connected() {
    let (mock, _driver) = MockAudioInput::new();
    let cap = AudioCapture::new(Box::new(mock), make_buffer());
    assert!(!cap.is_connected());
    assert_eq!(cap.current_level(), 0.0);
}

#[test]
fn capture_subscribe_returns_receiver_once() {
    let (mock, _driver) = MockAudioInput::new();
    let mut cap = AudioCapture::new(Box::new(mock), make_buffer());
    assert!(cap.subscribe().is_some(), "first subscribe should return Some");
    assert!(cap.subscribe().is_none(), "second subscribe should return None");
}

#[test]
fn capture_start_sets_connected() {
    let (mock, _driver) = MockAudioInput::new();
    let mut cap = AudioCapture::with_config(Box::new(mock), make_buffer(), fast_config());
    cap.start().expect("start failed");
    assert!(cap.is_connected());
    cap.stop();
}

#[test]
fn capture_stop_clears_connected_and_level() {
    let (mock, driver) = MockAudioInput::new();
    let mut cap = AudioCapture::with_config(Box::new(mock), make_buffer(), fast_config());
    cap.start().expect("start failed");
    driver.drive(vec![0.8f32; 512]);
    cap.stop();
    assert!(!cap.is_connected());
    assert_eq!(cap.current_level(), 0.0);
}

#[test]
fn capture_start_twice_does_not_panic() {
    let (mock, _driver) = MockAudioInput::new();
    let mut cap = AudioCapture::with_config(Box::new(mock), make_buffer(), fast_config());
    cap.start().expect("first start");
    cap.start().expect("second start");
    cap.stop();
}

// ─── AudioCapture — level monitoring ─────────────────────────────────────────

#[test]
fn capture_level_reflects_driven_audio() {
    let (mock, driver) = MockAudioInput::new();
    let mut cap = AudioCapture::with_config(Box::new(mock), make_buffer(), fast_config());
    cap.start().expect("start failed");

    // Drive with loud audio so the level rises.
    for _ in 0..5 {
        driver.drive(vec![0.9f32; 512]);
    }

    // Wait for at least one monitor tick to smooth the level.
    std::thread::sleep(std::time::Duration::from_millis(60));

    assert!(
        cap.current_level() > 0.0,
        "level should be > 0 after driving audio; got {}",
        cap.current_level()
    );
    cap.stop();
}

#[test]
fn capture_level_stays_zero_without_audio() {
    let (mock, _driver) = MockAudioInput::new();
    let mut cap = AudioCapture::with_config(Box::new(mock), make_buffer(), fast_config());
    cap.start().expect("start failed");

    std::thread::sleep(std::time::Duration::from_millis(60));

    // No samples were driven; the callback was never fired.
    assert_eq!(cap.current_level(), 0.0);
    cap.stop();
}

#[test]
fn capture_written_to_ring_buffer() {
    let buffer = make_buffer();
    let (mock, driver) = MockAudioInput::new();
    let mut cap =
        AudioCapture::with_config(Box::new(mock), Arc::clone(&buffer), fast_config());
    cap.start().expect("start failed");

    let samples = vec![0.5f32; 256];
    driver.drive(samples.clone());

    std::thread::sleep(std::time::Duration::from_millis(20));
    let read = buffer.read(256);
    assert_eq!(read, samples, "samples must appear in the ring buffer");
    cap.stop();
}

// ─── AudioCapture — disconnect detection ─────────────────────────────────────

#[test]
fn capture_emits_audio_input_lost_on_silence() {
    // No audio driven → level stays zero → monitor declares disconnect.
    let (mock, _driver) = MockAudioInput::new();
    let mut cap = AudioCapture::with_config(Box::new(mock), make_buffer(), fast_config());
    let events = cap.subscribe().unwrap();

    cap.start().expect("start failed");

    // Grace period (2 × 20 ms) + 1 tick (20 ms) + margin = ~100 ms.
    let event = events
        .recv_timeout(std::time::Duration::from_millis(500))
        .expect("expected AudioInputLost within 500 ms");
    assert_eq!(event, CaptureEvent::AudioInputLost);

    cap.stop();
}

#[test]
fn capture_is_connected_false_after_disconnect() {
    let (mock, _driver) = MockAudioInput::new();
    let mut cap = AudioCapture::with_config(Box::new(mock), make_buffer(), fast_config());
    let events = cap.subscribe().unwrap();
    cap.start().expect("start failed");

    events
        .recv_timeout(std::time::Duration::from_millis(500))
        .expect("expected AudioInputLost");

    assert!(!cap.is_connected(), "is_connected should be false after disconnect");
    cap.stop();
}

#[test]
fn capture_no_spurious_disconnect_while_audio_flowing() {
    // Continuously drive audio; no AudioInputLost should appear.
    let (mock, driver) = MockAudioInput::new();
    let mut cap = AudioCapture::with_config(Box::new(mock), make_buffer(), fast_config());
    let events = cap.subscribe().unwrap();
    cap.start().expect("start failed");

    let stop_driving = Arc::new(AtomicBool::new(false));
    let stop_driving2 = Arc::clone(&stop_driving);

    let driver_thread = std::thread::spawn(move || {
        while !stop_driving2.load(Ordering::Relaxed) {
            driver.drive(vec![0.5f32; 512]);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    });

    // Run for 400 ms while audio flows; expect no disconnect event.
    let result = events.recv_timeout(std::time::Duration::from_millis(400));
    stop_driving.store(true, Ordering::Relaxed);
    driver_thread.join().unwrap();
    cap.stop();

    assert!(
        result.is_err(),
        "AudioInputLost must not fire while audio is flowing"
    );
}

// ─── AudioCapture — reconnection ─────────────────────────────────────────────

#[test]
fn capture_emits_audio_input_restored_after_reconnect() {
    // 1. Start with a mock that doesn't fire the callback (silence → disconnect).
    // 2. Set fire_on_start = true so the reconnect attempt sees audio.
    // 3. Expect AudioInputLost then AudioInputRestored.

    let (mock, _driver) = MockAudioInput::new();
    let fire_on_start = Arc::clone(&mock.fire_on_start);

    let mut cap = AudioCapture::with_config(Box::new(mock), make_buffer(), fast_config());
    let events = cap.subscribe().unwrap();
    cap.start().expect("start failed");

    // Wait for disconnect.
    let first = events
        .recv_timeout(std::time::Duration::from_millis(500))
        .expect("expected AudioInputLost");
    assert_eq!(first, CaptureEvent::AudioInputLost);

    // Now simulate the device coming back.
    fire_on_start.store(true, Ordering::Relaxed);

    // Reconnect interval (80 ms) + probe wait (2 × 20 ms) + margin.
    let second = events
        .recv_timeout(std::time::Duration::from_millis(600))
        .expect("expected AudioInputRestored");
    assert_eq!(second, CaptureEvent::AudioInputRestored);

    cap.stop();
}

#[test]
fn capture_is_connected_true_after_restore() {
    let (mock, _driver) = MockAudioInput::new();
    let fire_on_start = Arc::clone(&mock.fire_on_start);

    let mut cap = AudioCapture::with_config(Box::new(mock), make_buffer(), fast_config());
    let events = cap.subscribe().unwrap();
    cap.start().expect("start failed");

    events
        .recv_timeout(std::time::Duration::from_millis(500))
        .expect("expected AudioInputLost");
    fire_on_start.store(true, Ordering::Relaxed);
    events
        .recv_timeout(std::time::Duration::from_millis(600))
        .expect("expected AudioInputRestored");

    assert!(cap.is_connected(), "is_connected should be true after restore");
    cap.stop();
}

#[test]
fn capture_no_restore_when_device_stays_unavailable() {
    // Device is unavailable (start() fails) — only AudioInputLost, no Restored.
    let (mock, _driver) = MockAudioInput::new();
    // Make the device unavailable so reconnect attempts always fail.
    mock.available.store(false, Ordering::Relaxed);
    // But we need initial start() to succeed, so temporarily allow it.
    mock.available.store(true, Ordering::Relaxed);

    let available = Arc::clone(&mock.available);
    let mut cap = AudioCapture::with_config(Box::new(mock), make_buffer(), fast_config());
    let events = cap.subscribe().unwrap();
    cap.start().expect("initial start");

    // Mark unavailable before disconnect happens so reconnect attempts fail.
    available.store(false, Ordering::Relaxed);

    let first = events
        .recv_timeout(std::time::Duration::from_millis(500))
        .expect("expected AudioInputLost");
    assert_eq!(first, CaptureEvent::AudioInputLost);

    // Wait two reconnect intervals; Restored must NOT appear.
    let second = events.recv_timeout(std::time::Duration::from_millis(300));
    assert!(
        second.is_err(),
        "AudioInputRestored must not fire when device stays unavailable"
    );

    cap.stop();
}
