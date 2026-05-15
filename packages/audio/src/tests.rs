use std::sync::{Arc, Mutex};

use crate::device::{infer_device_type, AudioDevice, DeviceType};
use crate::error::AudioError;
use crate::input::AudioInput;
use crate::ring_buffer::{RingBuffer, DEFAULT_CAPACITY};

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
