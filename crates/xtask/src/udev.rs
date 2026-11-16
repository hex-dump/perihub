//! packaging/linux/70-perihub.rules: one rule per USB id the device list opens.

use std::fmt::Write;

use crate::catalog::{Access, Device, Level};

pub const PATH: &str = "packaging/linux/70-perihub.rules";

const PREAMBLE: &str = r#"# PeriHub: give the user with the active session access to the HID configuration interfaces.
# Manual install:
#   sudo cp 70-perihub.rules /etc/udev/rules.d/ && sudo udevadm control --reload && sudo udevadm trigger

# Interface numbers (most systems already import them in 60-persistent-hidraw.rules).
KERNEL=="hidraw*", SUBSYSTEMS=="usb", ENV{ID_USB_INTERFACE_NUM}=="", IMPORT{builtin}="usb_id""#;

/// What each section of the rule says about the interfaces it opens. A new family needs one: the
/// generator stops until it is written.
fn header(family: &str) -> &'static str {
    match family {
        "apex" => {
            r#"SteelSeries Apex 3 TKL: only the configuration interface (lighting, brightness, key mapping).
The interfaces that report key presses stay closed: see 71-perihub-keys.rules."#
        }
        "logitech-onboard" => r#"Logitech mice (the G502 HERO is tested, the others are read-only until they are)"#,
        "logitech-direct" => {
            r#"Logitech keyboards and headsets with HID++ on their own USB connection, without a receiver (read only,
untested): only the interface with their HID++ collections (usage page ff43). A keyboard whose HID++
collections share an interface with what is typed on it is left out: one hidraw node holds both."#
        }
        "lightspeed" => {
            r#"LIGHTSPEED receivers, for wireless Logitech mice (read-only until they are tested): only the
receiver's own HID++ interface (02). Movements and typing arrive on its other interfaces, which
stay closed, and so do the devices that Linux creates for the mice paired with it."#
        }
        "bolt" => r#"Logi Bolt receiver: PeriHub only reads what is paired with it, for test reports."#,
        "unifying" => {
            r#"Unifying receivers (c52b, c532) are left out on purpose. Linux puts them in DJ mode, where what is
typed on the keyboards paired with them arrives on the same interface as HID++ (02): opening it
to the user would let any program running as the user read that typing. PeriHub offers the
optional 73-perihub-unifying.rules, with that warning, and reads them where another rule already
opens them, such as Solaar's."#
        }
        "g300" => {
            r#"Logitech G300 (read only): interface 1, which holds its profiles. It also has a keyboard collection,
which sends only the keys assigned to the mouse's own buttons."#
        }
        "corsair-mouse" => {
            r#"Corsair mice (experimental, untested), on their USB cable: only the control interface; the others report movements and clicks."#
        }
        "corsair-keyboard" => {
            r#"Corsair keyboards (experimental, untested), on their USB cable: only the control interface (1). Key presses arrive on
interface 0, and in software mode on interface 2 (OpenLinkHub): both stay closed."#
        }
        "corsair-receiver" => r#"Corsair wireless receivers (experimental, untested): only the control interface."#,
        "asus" => {
            r#"ASUS ROG and TUF mice (experimental, untested; protocol from libratbag). The configuration collection
shares its interface with the mouse's movements and the keys assigned to its buttons, as on the
Logitech mice above. Where no source names that interface, the whole mouse is opened. The ROG Omni
receiver (0b05:1ace) is left out: it is shared with other devices and may carry a keyboard's typing."#
        }
        "steelseries-mice" => {
            r#"SteelSeries mice (experimental, untested; commands from rivalcfg): only the interface rivalcfg sends
to. On the mice where that is interface 0 it also carries the mouse's movements and clicks, as on
the Logitech mice above. The 2.4 GHz receivers listed are the mice's own, not shared ones."#
        }
        "razer" => {
            r#"Razer mice (experimental, untested; facts from OpenRazer): only the interface OpenRazer sends to (0, or 3
on some newer mice). Interface 0 also carries the mouse's movements and clicks, as on the Logitech mice
above. Receivers and docks are left out: some can be shared with a keyboard."#
        }
        "glorious" => {
            r#"Glorious mice (experimental, untested; protocol from libratbag): only the ids that belong to
Glorious, not the generic SinoWealth ones, which other brands' devices share. No source names the
interface, so the whole mouse is opened, as for the ASUS mice above. Besides the configuration,
it carries the mouse's movements, clicks and the keys assigned to its own buttons, never what is
typed on a keyboard."#
        }
        "g600" => {
            r#"Logitech G600 (experimental, untested; protocol from libratbag, interface from OpenRGB): interface 1,
which holds its profiles. It also carries the keys the mouse's own side buttons send, never what is
typed on a keyboard."#
        }
        "logitech-hidpp10" => {
            r#"Logitech HID++ 1.0 mice (experimental, untested; protocol from libratbag, interface from Solaar):
interface 1, with their HID++ registers and onboard memory. It may also carry the keys and macros
the mouse's own buttons send, never what is typed on a keyboard."#
        }
        "roccat" => {
            r#"Roccat mice (read only, untested; protocol from libratbag): interface 0, the boot mouse interface,
where the kernel's own Roccat drivers find their configuration. Interface 1, with the keys the
mouse's buttons can send, stays closed."#
        }
        "pulsar" => {
            r#"Pulsar mice on their cable (experimental, untested; protocol from pulsar-mouse-linux): interface 3,
which holds their configuration. Interface 1, with the keys a button bound to a key or a macro types,
is never opened."#
        }
        "coolermaster" => {
            r#"Cooler Master mice, their lighting (experimental, untested; protocol from captures of Cooler Master's app):
interface 1, a vendor collection only. Interface 2, with the keys a button bound to a key types, is never opened."#
        }
        "nordic" => {
            r#"Pulsar CrazyLight and Lamzu Atlantis on their cable (experimental, untested; protocol from pulsar-mouse-linux
and lamzu-cfg): interface 1, which carries their configuration and the mouse's own input, never a keyboard's typing."#
        }
        "evga" => {
            r#"EVGA mice on their cable (lighting; facts from OpenRGB, untested): the X12's interface 1 (its LED collection) and
the X20's interface 2 (a vendor collection), which may also carry the keys their own buttons send, never a
keyboard's typing. The Z15 and Z20 keyboards are left out."#
        }
        _ => panic!("70-perihub.rules: write a header for the device family {family:?} in crates/xtask/src/udev.rs"),
    }
}

fn rule(vendor_id: u16, product_id: u16, access: Access) -> Option<String> {
    let start =
        format!(r#"KERNEL=="hidraw*", ATTRS{{idVendor}}=="{vendor_id:04x}", ATTRS{{idProduct}}=="{product_id:04x}""#);
    Some(match access {
        Access::Interface(n) => format!(r#"{start}, ENV{{ID_USB_INTERFACE_NUM}}=="{n:02}", TAG+="uaccess""#),
        Access::Whole => format!(r#"{start}, TAG+="uaccess""#),
        Access::ReceiverHidpp => {
            format!(
                r#"KERNEL=="hidraw*", KERNELS=="0003:{vendor_id:04X}:{product_id:04X}.*", ENV{{ID_USB_INTERFACE_NUM}}=="02", TAG+="uaccess""#
            )
        }
        Access::Closed => return None,
    })
}

pub fn render(devices: &[Device]) -> String {
    let mut out = format!("{PREAMBLE}\n");
    let mut families: Vec<&str> = Vec::new();
    for d in devices {
        if !families.contains(&d.family) {
            families.push(d.family);
        }
    }
    for family in families {
        let members: Vec<&Device> = devices.iter().filter(|d| d.family == family).collect();
        let opened: Vec<&&Device> =
            members.iter().filter(|d| d.access != Access::Closed && !d.product_ids.is_empty()).collect();
        out.push('\n');
        for line in header(family).lines() {
            out.push_str(if line.is_empty() { "#" } else { "# " });
            out.push_str(line);
            out.push('\n');
        }
        // A single device is named by the header already, and so are receivers that all have the same name.
        let named = opened.len() > 1 && !opened.iter().all(|d| d.name == opened[0].name);
        for d in opened {
            if named {
                let read_only = if d.level == Level::ReadOnly { " (read only)" } else { "" };
                writeln!(out, "# {} {}{read_only}", d.brand, d.name).unwrap();
            }
            for &id in &d.product_ids {
                if let Some(rule) = rule(d.vendor_id, id, d.access) {
                    writeln!(out, "{rule}").unwrap();
                }
            }
        }
    }
    out
}
