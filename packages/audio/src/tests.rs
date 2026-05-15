use std::sync::{Arc, Mutex};

use crate::device::{infer_device_type, AudioDevice, DeviceType};
use crate::error::AudioError;
use crate::input::AudioInput;

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
