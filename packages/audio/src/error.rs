use thiserror::Error;

#[derive(Debug, Error)]
pub enum AudioError {
    #[error("no audio host available")]
    NoHost,
    #[error("no audio devices found")]
    NoDevices,
    #[error("device not found: {0}")]
    DeviceNotFound(String),
    #[error("failed to build audio stream: {0}")]
    StreamBuild(String),
    #[error("failed to play audio stream: {0}")]
    StreamPlay(String),
    #[error("cpal device error: {0}")]
    CpalDevice(String),
    #[error("cpal config error: {0}")]
    CpalConfig(String),
}
