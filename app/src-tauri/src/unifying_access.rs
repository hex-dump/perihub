//! Permission to open the Unifying receivers on Linux.
//!
//! Linux puts them in DJ mode, where the typing of the keyboards paired with them arrives on the
//! same interface as HID++: the default udev rule leaves them closed. This optional rule opens them
//! to every program of the user, so it is installed only on request, through the system's password
//! window (polkit), and can be removed.

use serde::Serialize;

use crate::key_access::as_root;

const RULE: &str = include_str!("../../../packaging/linux/73-perihub-unifying.rules");
const RULE_PATH: &str = "/etc/udev/rules.d/73-perihub-unifying.rules";
const RELOAD: &str = "udevadm control --reload && udevadm trigger --subsystem-match=hidraw";

#[derive(Serialize)]
pub struct UnifyingAccess {
    /// The optional rule is installed.
    rule_installed: bool,
    /// The rule can be installed from PeriHub (Linux only).
    supported: bool,
    /// Commands to run in a terminal if the password window cannot be shown.
    manual_install: String,
    manual_remove: String,
}

#[tauri::command]
pub fn unifying_access() -> UnifyingAccess {
    UnifyingAccess {
        rule_installed: std::path::Path::new(RULE_PATH).exists(),
        supported: cfg!(target_os = "linux"),
        manual_install: format!(
            "curl -fsSL https://raw.githubusercontent.com/hex-dump/perihub/main/packaging/linux/73-perihub-unifying.rules \
             | sudo tee {RULE_PATH} >/dev/null && sudo sh -c '{RELOAD}'"
        ),
        manual_remove: format!("sudo rm -f {RULE_PATH} && sudo sh -c '{RELOAD}'"),
    }
}

#[tauri::command]
pub async fn unifying_grant_access() -> Result<(), String> {
    as_root(format!("cat > {RULE_PATH} && {RELOAD}"), RULE).await
}

#[tauri::command]
pub async fn unifying_revoke_access() -> Result<(), String> {
    as_root(format!("rm -f {RULE_PATH} && {RELOAD}"), "").await
}
