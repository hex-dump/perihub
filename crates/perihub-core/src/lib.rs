//! PeriHub: configure the SteelSeries Apex 3 TKL and Logitech G502 HERO without GG or G HUB.

pub mod apex;
pub mod app_profiles;
pub mod color;
pub mod config;
pub mod conflicts;
pub mod corsair;
pub mod effects;
pub mod error;
pub mod focus;
pub mod g300;
pub mod hidpp;
pub mod keyboard;
pub mod logitech;
pub mod manager;
pub mod mouse;
pub mod peripheral;
#[cfg(windows)]
pub mod rawinput;
pub mod report_devices;
pub mod softmacro;
pub mod worker;

pub use color::Rgb;
pub use error::{Error, Result};
pub use manager::{DeviceStatus, Manager};
