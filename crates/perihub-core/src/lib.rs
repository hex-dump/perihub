//! PeriHub: configure the SteelSeries Apex 3 TKL and Logitech G502 HERO without GG or G HUB.

pub mod apex;
pub mod app_profiles;
pub mod battery;
pub mod color;
pub mod config;
pub mod confirmed;
pub mod conflicts;
pub mod corsair;
pub mod effects;
pub mod error;
pub mod focus;
pub mod g300;
pub mod games;
pub mod hid_descriptor;
pub mod hidpp;
pub mod kblayout;
pub mod keyboard;
pub mod logitech;
pub mod manager;
pub mod mouse;
pub mod peripheral;
pub mod quick;
#[cfg(windows)]
pub mod rawinput;
pub mod replay;
pub mod report_devices;
pub mod softmacro;
pub mod sync;
pub mod worker;
pub mod writes;

pub use color::Rgb;
pub use error::{Error, Result};
pub use manager::{DeviceStatus, Manager};
