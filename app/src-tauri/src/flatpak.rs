//! Running as a Flatpak (Flathub). The sandbox changes two things:
//! - autostart goes through the Background portal, which writes the entry on the host: the
//!   sandbox's own `~/.config/autostart` is not the one the desktop reads;
//! - system files cannot be installed from inside it, so the optional udev rules are installed
//!   by hand, with the commands the interface shows (see [`crate::key_access::as_root`]).

use std::path::{Path, PathBuf};
use std::process::Command;

/// PeriHub runs inside a Flatpak sandbox.
pub fn active() -> bool {
    Path::new("/.flatpak-info").exists()
}

/// Marks that autostart was asked of the portal: the portal does not say afterwards whether the
/// entry is there.
fn autostart_mark() -> PathBuf {
    perihub_core::config::config_dir().join("autostart-requested")
}

pub fn autostart_requested() -> bool {
    autostart_mark().exists()
}

/// The portal's `RequestBackground` options: start at sign-in or not, hidden in the tray.
fn background_options(enabled: bool) -> String {
    format!(
        "{{'autostart': <{enabled}>, 'commandline': <['perihub', '{}']>, \
         'reason': <'Start PeriHub at sign-in, in the system tray'>}}",
        crate::HIDDEN_ARG
    )
}

/// Asks the Background portal to start PeriHub at sign-in, or to stop doing so. The desktop may
/// ask the user first; its answer arrives later, so the setting keeps what was asked.
pub fn request_autostart(enabled: bool) -> Result<(), String> {
    let output = Command::new("gdbus")
        .args([
            "call",
            "--session",
            "--dest",
            "org.freedesktop.portal.Desktop",
            "--object-path",
            "/org/freedesktop/portal/desktop",
            "--method",
            "org.freedesktop.portal.Background.RequestBackground",
            "",
            &background_options(enabled),
        ])
        .output()
        .map_err(|e| format!("cannot ask the desktop: {e}"))?;
    if !output.status.success() {
        return Err(format!("the desktop refused: {}", String::from_utf8_lossy(&output.stderr).trim()));
    }
    let mark = autostart_mark();
    if enabled {
        if let Some(dir) = mark.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(&mark, b"").map_err(|e| e.to_string())
    } else {
        match std::fs::remove_file(&mark) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_portal_is_asked_for_a_hidden_start() {
        assert_eq!(
            background_options(true),
            "{'autostart': <true>, 'commandline': <['perihub', '--hidden']>, \
             'reason': <'Start PeriHub at sign-in, in the system tray'>}"
        );
        assert!(background_options(false).starts_with("{'autostart': <false>"));
    }
}
