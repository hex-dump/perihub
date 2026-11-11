//! The Linux udev rule must grant access to exactly the mice and receivers PeriHub recognizes.

use perihub_core::hidpp::models::MODELS;
use perihub_core::hidpp::receiver::{RECEIVERS, UNIFYING};

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
    let in_list = MODELS
        .iter()
        .filter(|m| m.has_cable())
        .map(|m| format!("{:04x}", m.product_id))
        .chain([format!("{:04x}", perihub_core::g300::PRODUCT_ID)])
        // The G600, from the other brands' tables (checked there).
        .chain(perihub_core::peripheral::logitech_g600::MODELS.iter().map(|m| format!("{:04x}", m.product_ids[0])))
        .collect();
    assert_eq!(sorted(in_rule), sorted(in_list));
}

#[test]
fn udev_rule_opens_only_the_hidpp_interface_of_the_receivers() {
    let lines: Vec<&str> = RULE.lines().filter(|line| line.contains(r#"KERNELS=="0003:046D:"#)).collect();
    for line in &lines {
        assert!(line.contains(r#"ENV{ID_USB_INTERFACE_NUM}=="02""#), "{line}");
    }
    let in_rule = lines.iter().map(|line| line.split("0003:046D:").nth(1).unwrap()[..4].to_lowercase()).collect();
    let in_list = RECEIVERS.iter().filter(|id| !UNIFYING.contains(id)).map(|id| format!("{id:04x}")).collect();
    assert_eq!(sorted(in_rule), sorted(in_list));
}

const UNIFYING_RULE: &str = include_str!("../../../packaging/linux/73-perihub-unifying.rules");

#[test]
fn the_optional_unifying_rule_opens_only_their_hidpp_interface() {
    let lines: Vec<&str> = UNIFYING_RULE.lines().filter(|line| line.contains("TAG+=")).collect();
    for line in &lines {
        assert!(line.contains(r#"ENV{ID_USB_INTERFACE_NUM}=="02""#), "{line}");
        assert!(line.contains(r#"KERNELS=="0003:046D:"#), "{line}");
    }
    let in_rule = lines.iter().map(|line| line.split("0003:046D:").nth(1).unwrap()[..4].to_lowercase()).collect();
    let in_list = UNIFYING.iter().map(|id| format!("{id:04x}")).collect();
    assert_eq!(sorted(in_rule), sorted(in_list));
    assert!(UNIFYING_RULE.contains("WARNING"), "the rule explains what it opens");
}

#[test]
fn udev_rule_never_opens_a_unifying_receiver() {
    // In DJ mode, which Linux turns on for Unifying receivers, what is typed on the keyboards
    // paired with them comes through the same interface as HID++: opening it to the user would let
    // any program read it.
    for id in UNIFYING {
        let upper = format!("{id:04X}");
        let lower = format!("{id:04x}");
        for line in RULE.lines().filter(|line| !line.trim_start().starts_with('#')) {
            assert!(!line.contains(&upper) && !line.contains(&lower), "{line}");
        }
    }
}

#[test]
fn udev_rule_opens_only_the_control_interface_of_the_corsair_devices() {
    use perihub_core::corsair::models::MODELS as CORSAIR;
    let lines: Vec<&str> = RULE.lines().filter(|line| line.contains(r#"ATTRS{idVendor}=="1b1c""#)).collect();
    for line in &lines {
        assert!(line.contains(r#"ENV{ID_USB_INTERFACE_NUM}=="01""#), "{line}");
    }
    let in_rule =
        lines.iter().map(|line| line.split(r#"ATTRS{idProduct}==""#).nth(1).unwrap()[..4].to_string()).collect();
    let in_list = CORSAIR
        .iter()
        .map(|m| format!("{:04x}", m.product_id))
        .chain(perihub_core::corsair::models::RECEIVERS.iter().map(|(id, _)| format!("{id:04x}")))
        .collect();
    assert_eq!(sorted(in_rule), sorted(in_list));
}

#[test]
fn udev_rule_opens_only_interface_1_of_the_g300() {
    let lines: Vec<&str> = RULE.lines().filter(|line| line.contains(r#"ATTRS{idProduct}=="c246""#)).collect();
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains(r#"ENV{ID_USB_INTERFACE_NUM}=="01""#));
}

#[test]
fn udev_rule_matches_the_other_brands_tables() {
    use perihub_core::peripheral::models;
    for vendor in perihub_core::peripheral::vendors() {
        let tag = format!(r#"ATTRS{{idVendor}}=="{vendor:04x}""#);
        let lines: Vec<&str> = RULE.lines().filter(|l| !l.trim_start().starts_with('#') && l.contains(&tag)).collect();
        // Devices of the same vendor handled by PeriHub's own drivers (the Apex 3 TKL).
        let others = ["1622"];
        let mut in_rule: Vec<String> = lines
            .iter()
            .map(|l| l.split(r#"ATTRS{idProduct}==""#).nth(1).unwrap()[..4].to_string())
            .filter(|id| !(vendor == 0x1038 && others.contains(&id.as_str())))
            .collect();
        let mut in_tables = Vec::new();
        for model in models().filter(|m| m.vendor_id == vendor) {
            for id in model.product_ids {
                let id = format!("{id:04x}");
                let line = lines.iter().find(|l| l.contains(&format!(r#"ATTRS{{idProduct}}=="{id}""#))).unwrap();
                match model.interface {
                    // Only the interface a source names.
                    Some(n) => assert!(line.contains(&format!(r#"ENV{{ID_USB_INTERFACE_NUM}}=="{n:02}""#)), "{line}"),
                    None => assert!(!line.contains("ID_USB_INTERFACE_NUM"), "{line}"),
                }
                in_tables.push(id);
            }
        }
        // Logitech's other lines are the HID++ mice and receivers, checked against their own lists.
        if vendor == 0x046D {
            in_rule.retain(|id| in_tables.contains(id));
        }
        in_rule.sort();
        in_tables.sort();
        assert_eq!(in_rule, in_tables, "vendor {vendor:04x}");
    }
}

#[test]
fn udev_rule_never_opens_the_asus_omni_receiver() {
    for line in RULE.lines().filter(|l| !l.trim_start().starts_with('#')) {
        assert!(!line.contains(r#"ATTRS{idProduct}=="1ace""#), "{line}");
    }
}
