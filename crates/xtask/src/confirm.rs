//! cargo xtask confirm: a tester's report showed a device works, so it becomes supported.
//!
//!   cargo xtask confirm 1532:00b7 --issue 40            a device of another brand, or a Corsair device
//!   cargo xtask confirm --memory-format 4:255 --issue 40    a Logitech memory format (profile format:sector size)
//!
//! It refuses what PeriHub does not know or has confirmed already, adds the entry to
//! `perihub-core/src/confirmed.rs` (sorted, one per line), runs `cargo xtask gen` so the catalogue,
//! README, docs and website follow, and adds the CHANGELOG line under "## Unreleased".
//!
//!   cargo xtask confirm 1532:00b7 --issue 40 --report ISSUE_BODY.md
//!
//! With `--report`, the file holds the issue's text, and the transcript at the end of its test report becomes a
//! replay fixture when it can (see transcript.rs); when it cannot, the confirmation goes on and says why.

use std::fs;
use std::process::Command;

use perihub_core::confirmed;
use perihub_core::mouse::{WRITABLE_PROFILE_FORMATS, WRITABLE_SECTOR_SIZES};

use crate::catalog::{self, Device, Source};
use crate::transcript;

const TABLE_PATH: &str = "crates/perihub-core/src/confirmed.rs";
const CHANGELOG_PATH: &str = "CHANGELOG.md";
const USB_TABLE: &str = "pub const USB: &[UsbConfirmation] = &[";
const MEMORY_TABLE: &str = "pub const MEMORY: &[MemoryConfirmation] = &[";
const USAGE: &str = "usage: cargo xtask confirm VENDOR:PRODUCT --issue N\n       cargo xtask confirm --memory-format PROFILE_FORMAT:SECTOR_SIZE --issue N";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Usb { vendor_id: u16, product_id: u16 },
    Memory { profile_format: u8, sector_size: u16 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Request {
    pub target: Target,
    pub issue: u32,
}

fn pair(text: &str) -> Option<(&str, &str)> {
    text.split_once(':')
}

pub fn parse(args: &[String]) -> Result<Request, String> {
    let mut usb = None;
    let mut memory = None;
    let mut issue = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--issue" => issue = Some(it.next().ok_or("--issue needs a number")?.as_str()),
            "--memory-format" => {
                memory = Some(it.next().ok_or("--memory-format needs PROFILE_FORMAT:SECTOR_SIZE")?.as_str())
            }
            other if !other.starts_with('-') && usb.is_none() => usb = Some(other),
            other => return Err(format!("unexpected argument {other:?}\n{USAGE}")),
        }
    }
    let issue: u32 = issue
        .ok_or_else(|| format!("--issue is required\n{USAGE}"))?
        .parse()
        .ok()
        .filter(|&n| n > 0)
        .ok_or("--issue must be a positive number")?;
    let target = match (usb, memory) {
        (Some(id), None) => {
            let (v, p) = pair(id).ok_or("the USB id must be vvvv:pppp")?;
            let parse = |s: &str| (s.len() == 4).then(|| u16::from_str_radix(s, 16).ok()).flatten();
            Target::Usb {
                vendor_id: parse(v).ok_or("the USB id must be vvvv:pppp")?,
                product_id: parse(p).ok_or("the USB id must be vvvv:pppp")?,
            }
        }
        (None, Some(format)) => {
            let (f, s) = pair(format).ok_or("the memory format must be PROFILE_FORMAT:SECTOR_SIZE, such as 4:255")?;
            Target::Memory {
                profile_format: f.parse().map_err(|_| "the profile format must be a number")?,
                sector_size: s.parse().map_err(|_| "the sector size must be a number")?,
            }
        }
        _ => return Err(format!("give either a USB id or --memory-format\n{USAGE}")),
    };
    Ok(Request { target, issue })
}

/// What is being confirmed, in words, or why it cannot be.
/// `memory_tested(profile_format, sector_size)` says which memory formats are confirmed already.
pub fn check(request: &Request, devices: &[Device], memory_tested: &dyn Fn(u8, u16) -> bool) -> Result<String, String> {
    match request.target {
        Target::Usb { vendor_id, product_id } => {
            let id = format!("{vendor_id:04x}:{product_id:04x}");
            let device = devices
                .iter()
                .find(|d| d.vendor_id == vendor_id && d.product_ids.contains(&product_id))
                .ok_or_else(|| format!("PeriHub does not know {id}: add the device first"))?;
            let label = format!("{} {}", device.brand, device.name);
            match device.model {
                Source::Peripheral(_) | Source::Corsair => {}
                _ => {
                    return Err(format!(
                        "{label} ({id}) is not confirmed by USB id: a Logitech mouse by --memory-format, the rest by hand"
                    ));
                }
            }
            if device.level == catalog::Level::Supported {
                return Err(format!("{label} ({id}) is supported already"));
            }
            Ok(label)
        }
        Target::Memory { profile_format, sector_size } => {
            let what = format!("profile format {profile_format} in {sector_size}-byte sectors");
            if !WRITABLE_PROFILE_FORMATS.contains(&profile_format)
                || !WRITABLE_SECTOR_SIZES.contains(&(sector_size as usize))
            {
                return Err(format!(
                    "PeriHub does not write {what}: only profile formats {WRITABLE_PROFILE_FORMATS:?} in sectors of {WRITABLE_SECTOR_SIZES:?} bytes"
                ));
            }
            if memory_tested(profile_format, sector_size) {
                return Err(format!("{what} is tested already"));
            }
            Ok(what)
        }
    }
}

/// The CHANGELOG line for a confirmation.
pub fn changelog_line(request: &Request, label: &str) -> String {
    let link = format!("[#{0}](https://github.com/hex-dump/perihub/issues/{0})", request.issue);
    match request.target {
        Target::Usb { .. } => format!("- {label} confirmed on real hardware by a tester ({link}): now supported."),
        Target::Memory { .. } => {
            format!("- Logitech mice with {label} confirmed on real hardware by a tester ({link}): now supported.")
        }
    }
}

/// The numbers before the entry's last argument, to sort by: `usb(0x1532, 0x00b7, 40)`.
fn sort_key(line: &str) -> Result<Vec<u64>, String> {
    let args = line.trim().split_once('(').and_then(|(_, rest)| rest.split_once(')')).map(|(args, _)| args);
    let numbers = args
        .and_then(|a| {
            a.split(',')
                .map(|n| {
                    let n = n.trim();
                    match n.strip_prefix("0x") {
                        Some(hex) => u64::from_str_radix(hex, 16).ok(),
                        None => n.parse().ok(),
                    }
                })
                .collect::<Option<Vec<_>>>()
        })
        .ok_or_else(|| format!("{TABLE_PATH}: cannot read the entry {line:?}"))?;
    Ok(numbers.into_iter().take(2).collect())
}

/// `source` with `entry` added to the table that starts at the line `table`, in order.
pub fn add_entry(source: &str, table: &str, entry: &str) -> Result<String, String> {
    let lines: Vec<&str> = source.lines().collect();
    let start = lines
        .iter()
        .position(|l| l.trim() == table)
        .ok_or_else(|| format!("{TABLE_PATH}: no table starts with {table:?}"))?
        + 1;
    let end = lines[start..].iter().position(|l| l.trim() == "];").ok_or("the table does not end")? + start;
    let mut entries: Vec<String> = lines[start..end].iter().map(|l| l.to_string()).collect();
    let key = sort_key(entry)?;
    for existing in &entries {
        if sort_key(existing)? == key {
            return Err(format!("{TABLE_PATH}: already has an entry for {entry:?}"));
        }
    }
    entries.push(entry.to_string());
    let mut keyed = entries.into_iter().map(|e| sort_key(&e).map(|k| (k, e))).collect::<Result<Vec<_>, _>>()?;
    keyed.sort();
    let mut out: Vec<String> = lines[..start].iter().map(|l| l.to_string()).collect();
    out.extend(keyed.into_iter().map(|(_, e)| e));
    out.extend(lines[end..].iter().map(|l| l.to_string()));
    Ok(out.join("\n") + "\n")
}

/// `text` (CHANGELOG.md) with `line` added under "### New devices" of "## Unreleased", where the
/// section is made first if there is none.
pub fn add_changelog_line(text: &str, line: &str) -> Result<String, String> {
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let unreleased = lines.iter().position(|l| l == "## Unreleased").ok_or("CHANGELOG.md has no '## Unreleased'")?;
    let end =
        lines[unreleased + 1..].iter().position(|l| l.starts_with("## ")).map_or(lines.len(), |i| i + unreleased + 1);
    let section = lines[unreleased + 1..end].iter().position(|l| l == "### New devices").map(|i| i + unreleased + 1);
    match section {
        Some(at) => {
            let next = lines[at + 1..end].iter().position(|l| l.starts_with("### ")).map_or(end, |i| i + at + 1);
            let mut last = next;
            while last > at + 1 && lines[last - 1].trim().is_empty() {
                last -= 1;
            }
            lines.insert(last, line.to_string());
        }
        None => {
            // New devices comes first of the sections: before the first one there is, else at the end.
            match lines[unreleased + 1..end].iter().position(|l| l.starts_with("### ")).map(|i| i + unreleased + 1) {
                Some(first) => {
                    lines.splice(first..first, ["### New devices".to_string(), line.to_string(), String::new()]);
                }
                None => {
                    let mut last = end;
                    while last > unreleased + 1 && lines[last - 1].trim().is_empty() {
                        last -= 1;
                    }
                    // The blank lines that followed the summary stay after the new section.
                    lines.splice(last..last, [String::new(), "### New devices".to_string(), line.to_string()]);
                }
            }
        }
    }
    Ok(lines.join("\n") + "\n")
}

/// Runs `cargo xtask gen` as a new process: the tables it reads are compiled in, and one of them
/// has just changed.
fn regenerate() -> Result<(), String> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let status = Command::new(cargo)
        .args(["run", "--quiet", "--package", "xtask", "--", "gen"])
        .current_dir(crate::root())
        .status()
        .map_err(|e| format!("cargo xtask gen: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err("cargo xtask gen failed: the entry and the CHANGELOG line are written, run it again".into())
    }
}

/// `args` without `--report FILE`, and the file.
fn split_report(args: &[String]) -> Result<(Vec<String>, Option<String>), String> {
    let mut rest = Vec::new();
    let mut report = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        if arg == "--report" {
            report = Some(it.next().ok_or("--report needs a file")?.clone());
        } else {
            rest.push(arg.clone());
        }
    }
    Ok((rest, report))
}

pub fn run(args: &[String]) -> Result<(), String> {
    let (args, report) = split_report(args)?;
    let request = parse(&args)?;
    let label = check(&request, &catalog::devices(), &confirmed::memory_tested)?;
    let (table, entry) = match request.target {
        Target::Usb { vendor_id, product_id } => {
            (USB_TABLE, format!("    usb({vendor_id:#06x}, {product_id:#06x}, {}),", request.issue))
        }
        Target::Memory { profile_format, sector_size } => {
            (MEMORY_TABLE, format!("    memory({profile_format}, {sector_size}, {}),", request.issue))
        }
    };
    let root = crate::root();
    let read = |path: &str| fs::read_to_string(root.join(path)).map_err(|e| format!("{path}: {e}"));
    let table_source = add_entry(&read(TABLE_PATH)?.replace("\r\n", "\n"), table, &entry)?;
    let changelog =
        add_changelog_line(&read(CHANGELOG_PATH)?.replace("\r\n", "\n"), &changelog_line(&request, &label))?;
    fs::write(root.join(TABLE_PATH), table_source).map_err(|e| format!("{TABLE_PATH}: {e}"))?;
    fs::write(root.join(CHANGELOG_PATH), changelog).map_err(|e| format!("{CHANGELOG_PATH}: {e}"))?;
    regenerate()?;
    println!(
        "confirmed {label} (issue #{}): {TABLE_PATH}, {CHANGELOG_PATH} and the generated files are updated",
        request.issue
    );
    if let Some(report) = report {
        let target = match request.target {
            Target::Usb { vendor_id, product_id } => transcript::For::Usb(vendor_id, product_id),
            Target::Memory { profile_format, sector_size } => transcript::For::Memory { profile_format, sector_size },
        };
        match transcript::add(&report, request.issue, target) {
            Ok(path) => println!("replay fixture: {}", path.display()),
            Err(why) => println!("no replay fixture: {why}"),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Level;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn usb(vendor_id: u16, product_id: u16) -> Request {
        Request { target: Target::Usb { vendor_id, product_id }, issue: 40 }
    }

    #[test]
    fn arguments() {
        assert_eq!(parse(&args(&["1532:00B7", "--issue", "40"])), Ok(usb(0x1532, 0x00b7)));
        assert_eq!(
            parse(&args(&["--issue", "7", "--memory-format", "4:255"])),
            Ok(Request { target: Target::Memory { profile_format: 4, sector_size: 255 }, issue: 7 })
        );
        for bad in [
            &["1532:00b7"][..],
            &["--issue", "40"],
            &["1532:00b7", "--issue", "0"],
            &["1532:00b7", "--issue", "x"],
            &["1532:b7", "--issue", "40"],
            &["1532:00b7", "--memory-format", "4:255", "--issue", "40"],
            &["--memory-format", "4", "--issue", "40"],
            &["1532:00b7", "--issue", "40", "--force"],
        ] {
            assert!(parse(&args(bad)).is_err(), "{bad:?}");
        }
    }

    /// The memory formats confirmed before issues were used and in issue #15, whatever testers confirm later:
    /// these tests do not depend on what the real table holds.
    fn seeded(profile_format: u8, sector_size: u16) -> bool {
        matches!((profile_format, sector_size), (2, 256) | (3, 255))
    }

    /// The catalogue as it is with nothing confirmed.
    fn unconfirmed() -> Vec<Device> {
        catalog::devices_with(&|_, _| false)
    }

    /// A Razer mouse and a Corsair one from the tables, and what the catalogue says of them.
    fn known() -> (Device, Device) {
        let devices = unconfirmed();
        let razer = devices.iter().find(|d| matches!(d.model, Source::Peripheral(_)) && d.brand == "Razer").unwrap();
        let corsair = devices.iter().find(|d| matches!(d.model, Source::Corsair)).unwrap();
        (razer.clone(), corsair.clone())
    }

    #[test]
    fn only_known_unconfirmed_devices_are_confirmed() {
        let (razer, corsair) = known();
        let devices = unconfirmed();
        for d in [&razer, &corsair] {
            let request = usb(d.vendor_id, d.product_ids[0]);
            assert_eq!(check(&request, &devices, &seeded), Ok(format!("{} {}", d.brand, d.name)));
            // Once confirmed, the catalogue says supported and a second confirmation is refused.
            let id = (d.vendor_id, d.product_ids[0]);
            let after = catalog::devices_with(&|v, p| (v, p) == id);
            assert!(check(&request, &after, &seeded).unwrap_err().contains("supported already"));
        }
        assert!(check(&usb(0x1234, 0x5678), &devices, &seeded).unwrap_err().contains("does not know"));
        // A Logitech mouse is confirmed by its memory format; the G300 is supported already.
        let g502 = devices.iter().find(|d| d.name == "G502 Proteus Core").unwrap();
        assert!(
            check(&usb(g502.vendor_id, g502.product_ids[0]), &devices, &seeded)
                .unwrap_err()
                .contains("--memory-format")
        );
    }

    #[test]
    fn the_report_file_is_not_an_argument_of_the_confirmation() {
        let (rest, report) = split_report(&args(&["1532:00b7", "--report", "body.md", "--issue", "40"])).unwrap();
        assert_eq!(report.as_deref(), Some("body.md"));
        assert_eq!(parse(&rest), Ok(usb(0x1532, 0x00b7)));
        assert!(split_report(&args(&["--report"])).is_err());
        assert_eq!(split_report(&args(&["1532:00b7", "--issue", "40"])).unwrap().1, None);
    }

    #[test]
    fn memory_formats_must_be_writable_and_untested() {
        let memory =
            |profile_format, sector_size| Request { target: Target::Memory { profile_format, sector_size }, issue: 40 };
        assert!(check(&memory(2, 255), &[], &seeded).is_ok());
        assert!(check(&memory(3, 256), &[], &seeded).is_ok());
        assert!(check(&memory(2, 256), &[], &seeded).unwrap_err().contains("tested already"));
        assert!(check(&memory(3, 255), &[], &seeded).unwrap_err().contains("tested already"));
        assert!(check(&memory(4, 255), &[], &seeded).unwrap_err().contains("does not write"));
        assert!(check(&memory(2, 512), &[], &seeded).unwrap_err().contains("does not write"));
    }

    #[test]
    fn a_confirmed_peripheral_or_corsair_id_is_supported_in_the_catalogue() {
        let (razer, corsair) = known();
        let before = unconfirmed();
        for d in [&razer, &corsair] {
            let id = (d.vendor_id, d.product_ids[0]);
            let after = catalog::devices_with(&|v, p| (v, p) == id);
            assert_eq!(before.len(), after.len());
            for (b, a) in before.iter().zip(&after) {
                let same = (b.vendor_id, &b.product_ids) == (d.vendor_id, &d.product_ids) && b.name == d.name;
                assert_eq!(a.level, if same { Level::Supported } else { b.level }, "{} {}", b.brand, b.name);
            }
        }
    }

    const TABLE: &str = "pub const A: u8 = 1;\n\n#[rustfmt::skip]\npub const USB: &[UsbConfirmation] = &[\n    usb(0x1532, 0x00b7, 40),\n    usb(0x1b1c, 0x1b9e, 41),\n];\n\npub const MEMORY: &[MemoryConfirmation] = &[\n];\n";

    #[test]
    fn entries_are_added_in_order() {
        let first = add_entry(TABLE, USB_TABLE, "    usb(0x046d, 0xc09e, 42),").unwrap();
        assert!(first.contains(
            "= &[\n    usb(0x046d, 0xc09e, 42),\n    usb(0x1532, 0x00b7, 40),\n    usb(0x1b1c, 0x1b9e, 41),\n];"
        ));
        let middle = add_entry(TABLE, USB_TABLE, "    usb(0x1532, 0x0100, 43),").unwrap();
        assert!(middle.contains("40),\n    usb(0x1532, 0x0100, 43),\n    usb(0x1b1c"));
        let empty = add_entry(TABLE, MEMORY_TABLE, "    memory(4, 255, 44),").unwrap();
        assert!(empty.ends_with("= &[\n    memory(4, 255, 44),\n];\n"));
        assert!(add_entry(TABLE, USB_TABLE, "    usb(0x1532, 0x00b7, 99),").unwrap_err().contains("already"));
        assert!(add_entry("nothing", USB_TABLE, "    usb(1, 2, 3),").is_err());
    }

    #[test]
    fn the_real_table_takes_an_entry_and_keeps_its_shape() {
        let source = fs::read_to_string(crate::root().join(TABLE_PATH)).unwrap();
        // Ids below every real one go first, whatever the tables hold when this runs.
        let after = add_entry(&source, USB_TABLE, "    usb(0x0001, 0x0001, 40),").unwrap();
        assert!(after.contains("pub const USB: &[UsbConfirmation] = &[\n    usb(0x0001, 0x0001, 40),\n"));
        let after = add_entry(&after, MEMORY_TABLE, "    memory(1, 1, 50),").unwrap();
        assert!(after.contains("pub const MEMORY: &[MemoryConfirmation] = &[\n    memory(1, 1, 50),\n"));
        // Everything that was there still is, in order.
        for line in source.lines().filter(|l| l.trim_start().starts_with("memory(") && l.ends_with("),")) {
            assert!(after.contains(line), "{line}");
        }
    }

    const LOG: &str = "# Changelog\n\n## Unreleased\n\nSummary.\n\n### Added\n- A thing.\n\n### Changed\n- Another.\n\n## 0.8.4 — 2026-11-15\n\n### New devices\n- Old.\n";

    #[test]
    fn the_changelog_line_goes_under_new_devices_of_unreleased() {
        let line = "- X confirmed.";
        // No section yet: made first, before Added.
        let made = add_changelog_line(LOG, line).unwrap();
        assert!(made.contains("Summary.\n\n### New devices\n- X confirmed.\n\n### Added\n"), "{made}");
        // The section exists: the line follows the last one, and the 0.8.4 section is left alone.
        let more = add_changelog_line(&made, "- Y confirmed.").unwrap();
        assert!(more.contains("### New devices\n- X confirmed.\n- Y confirmed.\n\n### Added"), "{more}");
        assert!(more.ends_with("### New devices\n- Old.\n"));
        // Nothing but a summary under Unreleased.
        let bare = "## Unreleased\n\nSummary.\n\n## 0.8.4 — 2026-11-15\n";
        assert_eq!(
            add_changelog_line(bare, line).unwrap(),
            "## Unreleased\n\nSummary.\n\n### New devices\n- X confirmed.\n\n## 0.8.4 — 2026-11-15\n"
        );
        assert!(add_changelog_line("## 0.8.4 — 2026-11-15\n", line).is_err());
    }

    #[test]
    fn the_changelog_lines() {
        assert_eq!(
            changelog_line(&usb(0x1532, 0xb7), "Razer Viper"),
            "- Razer Viper confirmed on real hardware by a tester ([#40](https://github.com/hex-dump/perihub/issues/40)): now supported."
        );
        let memory = Request { target: Target::Memory { profile_format: 4, sector_size: 255 }, issue: 40 };
        assert!(
            changelog_line(&memory, "profile format 4 in 255-byte sectors")
                .starts_with("- Logitech mice with profile format 4")
        );
    }
}
