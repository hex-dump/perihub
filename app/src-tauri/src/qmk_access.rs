//! Permission to open the raw HID interface of a QMK keyboard (VIA, Vial) on Linux.
//!
//! QMK keyboards have no USB ids PeriHub could list in its default rule. This optional rule names
//! each keyboard the user allows, and only the interface whose report descriptor PeriHub read in
//! sysfs and found to hold QMK's raw HID collection and nothing else: nothing typed on the keyboard
//! arrives there. It is written only on request, through the system's password window (polkit).

use perihub_core::peripheral::qmk::{self, RawInterface};
use serde::Serialize;

use crate::key_access::as_root;

const RULE_PATH: &str = "/etc/udev/rules.d/74-perihub-qmk.rules";
const RELOAD: &str = "udevadm control --reload && udevadm trigger --subsystem-match=hidraw";
const SYSFS: &str = "/sys/class/hidraw";

#[derive(Serialize)]
pub struct QmkAccess {
    /// The keyboards ("vvvv:pppp") the optional rule opens.
    allowed: Vec<String>,
    /// Linux: a rule is needed, and can be installed from PeriHub (in the Flatpak, with the commands).
    supported: bool,
    /// What the rule file would hold with this keyboard added, by key, for a terminal when the
    /// password window cannot be shown.
    manual_install: Vec<(String, String)>,
    manual_remove: String,
}

/// The keyboards in the installed rule file, as (vendor id, product id, interface, name).
fn installed() -> Vec<(u16, u16, u8, String)> {
    let Ok(text) = std::fs::read_to_string(RULE_PATH) else { return Vec::new() };
    let mut name = String::new();
    let mut found = Vec::new();
    for line in text.lines() {
        if let Some(comment) = line.strip_prefix("# ") {
            name = comment.to_string();
            continue;
        }
        let value = |attr: &str| {
            let start = line.find(attr)? + attr.len();
            let end = line[start..].find('"')? + start;
            Some(line[start..end].to_string())
        };
        let (Some(vendor), Some(product), Some(interface)) =
            (value("ATTRS{idVendor}==\""), value("ATTRS{idProduct}==\""), value("ENV{ID_USB_INTERFACE_NUM}==\""))
        else {
            continue;
        };
        if let (Ok(v), Ok(p), Ok(i)) =
            (u16::from_str_radix(&vendor, 16), u16::from_str_radix(&product, 16), u8::from_str_radix(&interface, 16))
        {
            found.push((v, p, i, std::mem::take(&mut name)));
        }
    }
    found
}

fn key(vendor_id: u16, product_id: u16) -> String {
    format!("{vendor_id:04x}:{product_id:04x}")
}

/// The raw HID interfaces of the keyboard `key` connected now, checked in sysfs.
fn interfaces_of(key: &str) -> Vec<RawInterface> {
    qmk::raw_interfaces(std::path::Path::new(SYSFS))
        .into_iter()
        .filter(|r| self::key(r.vendor_id, r.product_id) == key)
        .collect()
}

fn with(mut keyboards: Vec<(u16, u16, u8, String)>, add: &[RawInterface]) -> String {
    for r in add {
        keyboards.retain(|(v, p, i, _)| (*v, *p, *i) != (r.vendor_id, r.product_id, r.interface));
        keyboards.push((r.vendor_id, r.product_id, r.interface, r.name.clone()));
    }
    qmk::rules_file(&keyboards)
}

#[tauri::command]
pub fn qmk_access() -> QmkAccess {
    let current = installed();
    let linux = cfg!(target_os = "linux");
    let mut allowed: Vec<String> = current.iter().map(|(v, p, _, _)| key(*v, *p)).collect();
    allowed.dedup();
    let mut manual_install = Vec::new();
    if linux {
        let mut keys: Vec<String> =
            qmk::raw_interfaces(std::path::Path::new(SYSFS)).iter().map(|r| key(r.vendor_id, r.product_id)).collect();
        keys.dedup();
        for k in keys {
            let file = with(current.clone(), &interfaces_of(&k));
            manual_install
                .push((k, format!("sudo tee {RULE_PATH} >/dev/null <<'EOF'\n{file}EOF\nsudo sh -c '{RELOAD}'")));
        }
    }
    QmkAccess {
        allowed,
        supported: linux,
        manual_install,
        manual_remove: format!("sudo rm -f {RULE_PATH} && sudo sh -c '{RELOAD}'"),
    }
}

#[tauri::command]
pub async fn qmk_grant_access(key: String) -> Result<(), String> {
    let found = interfaces_of(&key);
    if found.is_empty() {
        return Err(
            "PeriHub found no interface of this keyboard that holds only QMK's raw HID collection: no rule was written"
                .into(),
        );
    }
    let file = with(installed(), &found);
    as_root(format!("cat > {RULE_PATH} && {RELOAD}"), file).await
}

#[tauri::command]
pub async fn qmk_revoke_access(key: String) -> Result<(), String> {
    let keep: Vec<_> = installed().into_iter().filter(|(v, p, _, _)| self::key(*v, *p) != key).collect();
    if keep.is_empty() {
        as_root(format!("rm -f {RULE_PATH} && {RELOAD}"), "").await
    } else {
        as_root(format!("cat > {RULE_PATH} && {RELOAD}"), qmk::rules_file(&keep)).await
    }
}
