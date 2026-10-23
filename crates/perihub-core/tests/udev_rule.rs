//! The Linux udev rule must grant access to exactly the mice and receivers PeriHub recognizes.

use perihub_core::hidpp::models::MODELS;
use perihub_core::hidpp::receiver::RECEIVERS;

const RULE: &str = include_str!("../../../packaging/linux/70-perihub.rules");

fn sorted(mut ids: Vec<String>) -> Vec<String> {
    ids.sort();
    ids
}

#[test]
fn udev_rule_matches_mouse_list() {
    let in_rule: Vec<String> = RULE
        .lines()
        .filter(|line| line.contains(r#"ATTRS{idVendor}=="046d""#))
        .map(|line| line.split(r#"ATTRS{idProduct}==""#).nth(1).unwrap()[..4].to_string())
        .collect();
    // Mice without a cable are reached through a receiver only.
    let in_list = MODELS.iter().filter(|m| m.has_cable()).map(|m| format!("{:04x}", m.product_id)).collect();
    assert_eq!(sorted(in_rule), sorted(in_list));
}

#[test]
fn udev_rule_opens_only_the_hidpp_interface_of_the_receivers() {
    let lines: Vec<&str> = RULE.lines().filter(|line| line.contains(r#"KERNELS=="0003:046D:"#)).collect();
    for line in &lines {
        assert!(line.contains(r#"ENV{ID_USB_INTERFACE_NUM}=="02""#), "{line}");
    }
    let in_rule = lines.iter().map(|line| line.split("0003:046D:").nth(1).unwrap()[..4].to_lowercase()).collect();
    let in_list = RECEIVERS.iter().map(|id| format!("{id:04x}")).collect();
    assert_eq!(sorted(in_rule), sorted(in_list));
}
