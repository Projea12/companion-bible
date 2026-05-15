use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceType {
    /// Line-in / sound board mixer
    Mixer,
    /// USB microphone
    UsbMic,
    /// Built-in / on-board microphone
    Builtin,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub device_type: DeviceType,
    pub is_default: bool,
}

/// Infer device type from the device name reported by the OS.
pub(crate) fn infer_device_type(name: &str) -> DeviceType {
    let lower = name.to_lowercase();
    if lower.contains("usb") {
        DeviceType::UsbMic
    } else if lower.contains("line")
        || lower.contains("mixer")
        || lower.contains("board")
        || lower.contains("aggregate")
        || lower.contains("interface")
    {
        DeviceType::Mixer
    } else {
        DeviceType::Builtin
    }
}
