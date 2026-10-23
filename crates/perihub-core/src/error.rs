use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("HID error: {0}")]
    Hid(#[from] hidapi::HidError),
    #[error("the mouse rejected the command: {0}")]
    Hidpp(HidppErrorCode),
    #[error("the device did not respond in time")]
    Timeout,
    /// A mouse behind a LIGHTSPEED receiver that is turned off or asleep.
    #[error("the mouse is turned off or asleep")]
    Unreachable,
    #[error("feature {0:#06x} is not available on this device")]
    FeatureMissing(u16),
    #[error("unexpected response from the device: {0}")]
    Protocol(String),
    #[error("invalid value: {0}")]
    Invalid(String),
    #[error("device not connected")]
    NotConnected,
    #[error("file error: {0}")]
    Io(#[from] std::io::Error),
    #[error("format error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HidppErrorCode(pub u8);

impl std::fmt::Display for HidppErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self.0 {
            0x01 => "unknown error",
            0x02 => "invalid argument",
            0x03 => "out of range",
            0x04 => "hardware error",
            0x05 => "Logitech internal error",
            0x06 => "invalid feature index",
            0x07 => "invalid function",
            0x08 => "device busy",
            0x09 => "unsupported",
            _ => "undocumented code",
        };
        write!(f, "{name} ({:#04x})", self.0)
    }
}
