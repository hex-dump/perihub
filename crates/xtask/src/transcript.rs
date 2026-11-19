//! The replay fixture a test report can bring (docs/replay.md): the transcript at the end of the report,
//! checked and kept in `perihub-core/tests/fixtures/transcripts`, for `cargo xtask confirm --report FILE`.
//!
//! Only the "Test report" answer of the issue is read, and only a transcript that is whole (not cut), is
//! read-only, is of the device being confirmed, and that the core itself reads. Anything else gives
//! no fixture, and the confirmation goes on without one. What the transcript says is never run: it is
//! parsed as JSON and written back by the core's own serializer. A person reads it in the pull request
//! (and, as docs/replay.md says, looks for anything that tells the unit apart) before merging.

use std::fs;
use std::path::PathBuf;

use perihub_core::replay::{Access, Family, Transcript};

const HEADING: &str = "### Test report";
const MARKER: &str = "Transcript (every request and answer";
const CUT: &str = "cut here: too long for a test report";
const DIR: &str = "crates/perihub-core/tests/fixtures/transcripts";

/// The answer of the form field "Test report" in the body of an issue.
fn test_report_answer(body: &str) -> Option<String> {
    let body = body.replace("\r\n", "\n");
    let start = body.lines().position(|l| l.trim_end() == HEADING)? + 1;
    let rest: Vec<&str> = body.lines().skip(start).take_while(|l| !l.starts_with("### ")).collect();
    Some(rest.join("\n"))
}

/// The transcript at the end of a report, as the core reads it, or why there is none.
pub fn extract(body: &str) -> Result<Transcript, String> {
    let answer = test_report_answer(body).ok_or("the issue has no Test report answer")?;
    let at = answer.find(MARKER).ok_or("the report has no transcript")?;
    let after = &answer[at..];
    let start = after.find('{').ok_or("the transcript has no JSON")?;
    let mut values = serde_json::Deserializer::from_str(&after[start..]).into_iter::<serde_json::Value>();
    let value =
        values.next().ok_or("the transcript has no JSON")?.map_err(|e| format!("the transcript is not whole: {e}"))?;
    let text = value.to_string();
    if text.contains(CUT) {
        return Err("the transcript was cut: it cannot replay the whole reading".into());
    }
    Transcript::from_json(&text).map_err(|e| e.to_string())
}

/// A file name: lower-case letters and digits, one dash between words.
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_string()
}

/// What a fixture is for: the USB id of a device confirmed by it, or a Logitech memory format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum For {
    Usb(u16, u16),
    Memory { profile_format: u8, sector_size: u16 },
}

/// What the replay checks in what the code read, so far as the confirmation goes: no problem or read error
/// where the report had none, or the memory format of the mouse. Whoever reads the pull request adds the
/// values the tester confirmed (docs/replay.md): a replay without any `expect` checks nothing and is refused.
fn expect(family: Family, target: For) -> serde_json::Value {
    match (family, target) {
        (_, For::Memory { profile_format, sector_size }) => {
            serde_json::json!({"onboard": {"profile_format": profile_format, "sector_size": sector_size}})
        }
        (Family::Peripheral, _) => serde_json::json!({"problem": null}),
        _ => serde_json::json!({"read_errors": []}),
    }
}

/// The fixture for `t`, as it is kept: where it comes from (`issue`), and a note on what it lacks. It must be
/// read-only, and of the device being confirmed (a Logitech mouse, for a memory format).
pub fn fixture(mut t: Transcript, issue: u32, target: For) -> Result<(String, String), String> {
    if t.access != Access::ReadOnly {
        return Err(
            "the transcript has a command that changes something: it is not added without a person looking".into()
        );
    }
    let found = t.ids().map_err(|e| e.to_string())?;
    match target {
        For::Usb(vendor, product) if (vendor, product) != found => {
            return Err(format!(
                "the transcript is of {:04x}:{:04x}, not of {vendor:04x}:{product:04x}",
                found.0, found.1
            ));
        }
        For::Memory { .. } if t.family != Family::HidppMouse || found.0 != 0x046d => {
            return Err("the transcript is not of a Logitech mouse".into());
        }
        _ => {}
    }
    let name = slug(&t.device.name);
    if name.is_empty() {
        return Err("the transcript names no device".into());
    }
    t.source = Some(format!("issue #{issue}"));
    t.notes = vec![
        "Added with the confirmation of this device by cargo xtask confirm, from the transcript of its test report. Its `expect` holds only what the confirmation is about: add the values the tester confirmed (docs/replay.md).".into(),
    ];
    t.expect = Some(expect(t.family, target));
    Ok((format!("{name}.json"), t.to_json()))
}

/// Writes the fixture of the report in `report_file` and says where, or why not. Never overwrites.
pub fn add(report_file: &str, issue: u32, target: For) -> Result<PathBuf, String> {
    let body = fs::read_to_string(report_file).map_err(|e| format!("{report_file}: {e}"))?;
    let (file, json) = fixture(extract(&body)?, issue, target)?;
    let path = crate::root().join(DIR).join(file);
    if path.exists() {
        return Err(format!("{} is there already", path.display()));
    }
    fs::write(&path, json).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use perihub_core::replay::{Device, Event};

    fn transcript(access: Access) -> Transcript {
        let device = Device { vendor_id: "0b05".into(), product_id: "195c".into(), name: "ASUS ROG Keris".into() };
        let mut t = Transcript::new(device, Family::Peripheral, vec![Event::mark("start")]);
        t.access = access;
        t
    }

    fn body(t: &Transcript, extra: &str) -> String {
        format!(
            "### Device\n\nx\n\n### Test report\n\n```text\nPeriHub 0.9.5 on linux\nDevice: ASUS ROG Keris (mouse), USB ID 0b05:195c\n{MARKER} for a replay test):\n{}\nWrites this session: 0 sent, 0 refused\n```\n\n### Messages and errors\n\n{extra}\n",
            t.to_json()
        )
    }

    #[test]
    fn the_transcript_of_the_test_report_answer_is_read_and_nothing_after_it() {
        let t = transcript(Access::ReadOnly);
        assert_eq!(extract(&body(&t, "_No response_")).unwrap(), t);
    }

    #[test]
    fn one_in_another_answer_does_not_count() {
        let t = transcript(Access::ReadOnly);
        let other = format!(
            "### Test report\n\nPeriHub 0.9.5 on linux\n\n### What is wrong or missing\n\n{MARKER} x):\n{}",
            t.to_json()
        );
        assert!(extract(&other).unwrap_err().contains("no transcript"));
        assert!(extract("nothing").is_err());
    }

    #[test]
    fn a_cut_or_broken_transcript_is_refused() {
        let mut t = transcript(Access::ReadOnly);
        t.events.push(Event::mark(CUT));
        assert!(extract(&body(&t, "")).unwrap_err().contains("cut"));
        let broken = format!("### Test report\n\n{MARKER} x):\n{{\"format\": \"perihub-transcript\", \"events\": [");
        assert!(extract(&broken).is_err());
        let wrong = "### Test report\n\nTranscript (every request and answer):\n{\"format\": \"other\"}";
        assert!(extract(wrong).is_err());
    }

    #[test]
    fn the_fixture_says_where_it_is_from_and_expects_what_the_confirmation_is_about() {
        let mut t = transcript(Access::ReadOnly);
        t.expect = Some(serde_json::json!({"firmware": "x"}));
        let (file, json) = fixture(t, 44, For::Usb(0x0b05, 0x195c)).unwrap();
        assert_eq!(file, "asus-rog-keris.json");
        let back = Transcript::from_json(&json).unwrap();
        assert_eq!(back.source.as_deref(), Some("issue #44"));
        assert_eq!(back.expect, Some(serde_json::json!({"problem": null})));
        assert_eq!(back.notes.len(), 1);
    }

    #[test]
    fn only_a_read_only_transcript_of_the_confirmed_device_is_kept() {
        let usb = For::Usb(0x0b05, 0x195c);
        assert!(fixture(transcript(Access::ReadWrite), 44, usb).unwrap_err().contains("changes something"));
        assert!(fixture(transcript(Access::ReadOnly), 44, For::Usb(0x0b05, 0x1111)).unwrap_err().contains("not of"));
        // A Logitech mouse is confirmed by its memory format: a Peripheral transcript is not its.
        let memory = For::Memory { profile_format: 2, sector_size: 255 };
        assert!(fixture(transcript(Access::ReadOnly), 44, memory).unwrap_err().contains("Logitech mouse"));
        let mut mouse = transcript(Access::ReadOnly);
        mouse.family = Family::HidppMouse;
        mouse.device =
            Device { vendor_id: "046d".into(), product_id: "c08f".into(), name: "Logitech G403 HERO".into() };
        let (file, json) = fixture(mouse, 44, memory).unwrap();
        assert_eq!(file, "logitech-g403-hero.json");
        let expect = Transcript::from_json(&json).unwrap().expect.unwrap();
        assert_eq!(expect, serde_json::json!({"onboard": {"profile_format": 2, "sector_size": 255}}));
    }

    #[test]
    fn file_names() {
        assert_eq!(slug("Wraith W60"), "wraith-w60");
        assert_eq!(slug("  ../Etc/Passwd "), "etc-passwd");
        assert_eq!(slug("--"), "");
    }
}
