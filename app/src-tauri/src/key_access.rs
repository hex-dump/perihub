//! Permission to read the keyboard's key presses, needed by reactive lighting.
//!
//! On Linux the default udev rule only opens the keyboard's configuration interface. Reactive lighting
//! needs an extra rule, which lets every program of the user read what is typed on the Apex: it is
//! installed only on request, through the system's password window (polkit), and can be removed.

use serde::Serialize;
use tauri::State;

use crate::AppState;

const RULE: &str = include_str!("../../../packaging/linux/71-perihub-keys.rules");
const RULE_PATH: &str = "/etc/udev/rules.d/71-perihub-keys.rules";
const RELOAD: &str = "udevadm control --reload && udevadm trigger --subsystem-match=hidraw";

#[derive(Serialize)]
pub struct KeyAccess {
    /// PeriHub can read key presses now (`None`: the keyboard is not connected).
    readable: Option<bool>,
    /// The optional rule is installed.
    rule_installed: bool,
    /// The rule can be installed from PeriHub (Linux only).
    supported: bool,
    /// Commands to run in a terminal if the password window cannot be shown.
    manual_install: String,
    manual_remove: String,
}

#[tauri::command]
pub fn kb_key_access(state: State<'_, AppState>) -> KeyAccess {
    KeyAccess {
        readable: state.manager.key_presses_readable(),
        rule_installed: std::path::Path::new(RULE_PATH).exists(),
        supported: cfg!(target_os = "linux"),
        manual_install: format!(
            "curl -fsSL https://raw.githubusercontent.com/hex-dump/perihub/main/packaging/linux/71-perihub-keys.rules \
             | sudo tee {RULE_PATH} >/dev/null && sudo sh -c '{RELOAD}'"
        ),
        manual_remove: format!("sudo rm -f {RULE_PATH} && sudo sh -c '{RELOAD}'"),
    }
}

/// Runs a shell script as root through polkit, which shows the system's password window.
/// `input` goes to the script's standard input, so no file another program could swap is involved.
pub(crate) fn run_as_root(script: &str, input: &str) -> Result<(), String> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut child = Command::new("pkexec")
        .args(["/bin/sh", "-c", script])
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot run pkexec: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(input.as_bytes()).map_err(|e| e.to_string())?;
    }
    match child.wait().map_err(|e| e.to_string())?.code() {
        Some(0) => Ok(()),
        Some(126) => Err("cancelled".into()),
        Some(127) => Err("not authorized: the password window could not be shown (is a polkit agent running?)".into()),
        code => Err(format!("the command failed (exit code {code:?})")),
    }
}

pub(crate) async fn as_root(script: String, input: &'static str) -> Result<(), String> {
    if !cfg!(target_os = "linux") {
        return Err("only needed on Linux".into());
    }
    // The sandbox cannot install system files: the interface shows the commands to run instead.
    #[cfg(target_os = "linux")]
    if crate::flatpak::active() {
        return Err("PeriHub runs as a Flatpak, which cannot install system rules: run this in a terminal".into());
    }
    tauri::async_runtime::spawn_blocking(move || run_as_root(&script, input)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn kb_grant_key_access() -> Result<(), String> {
    as_root(format!("cat > {RULE_PATH} && {RELOAD}"), RULE).await
}

#[tauri::command]
pub async fn kb_revoke_key_access() -> Result<(), String> {
    as_root(format!("rm -f {RULE_PATH} && {RELOAD}"), "").await
}
