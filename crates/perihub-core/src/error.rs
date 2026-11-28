use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("HID error: {0}")]
    Hid(#[from] hidapi::HidError),
    #[error("the mouse rejected the command: {0}")]
    Hidpp(HidppErrorCode),
    #[error("the device did not respond in time")]
    Timeout,
    /// A mouse behind a LIGHTSPEED receiver, or on Bluetooth, that is turned off or asleep.
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
    /// Another device now answers where PeriHub knew a different one (a receiver slot paired again):
    /// what was read about the old one no longer applies.
    #[error("a different device answers in this receiver slot")]
    OtherDevice,
    /// Read-only mode is on: nothing that changes a device is sent (see [`crate::writes`]).
    #[error("Read-only mode is on")]
    ReadOnly,
    #[error("file error: {0}")]
    Io(#[from] std::io::Error),
    #[error("format error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    /// A write that Windows did not finish within hidapi's time limit (1 second in hidapi 0.15, the version the
    /// `hidapi` crate 2.6 builds): hidapi gives up waiting with this message but does not cancel the write, which
    /// stays pending on the handle and keeps the handle's only write slot (issues #40 and #156). Only closing the
    /// handle cancels it.
    pub fn is_write_stall(&self) -> bool {
        matches!(self, Error::Hid(hidapi::HidError::HidApiError { message }) if message.contains("hid_write/WaitForSingleObject"))
    }
}

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
