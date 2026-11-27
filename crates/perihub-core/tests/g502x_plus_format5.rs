//! The G502 X PLUS of issue #149 (`046d:c095` through its LIGHTSPEED receiver, firmware MPM 27.00.0016):
//! profile format 5 in 255-byte sectors. PeriHub 0.9.8 read its profile list, profile 1 and factory
//! profile 1 for the test report; the sectors are put back together from that transcript
//! (`fixtures/transcripts/logitech-g502-x-plus-receiver.json`) and checked against what the tester
//! wrote on the issue that profile 1 holds. Nothing here says that format 5 can be written.

use perihub_core::hidpp::profile::{
    ButtonAction, LedEffect, ProfileSettings, SpecialFunction, crc_ccitt, crc_is_valid, parse_directory, write_crc,
};
use serde_json::Value;

const SECTOR_SIZE: usize = 255;
const BUTTONS: usize = 11;
/// Index of 0x8100 (onboard profiles) in this mouse's feature table, and its read function (5).
const ONBOARD_PROFILES: u8 = 0x0b;
const READ_SECTOR: u8 = 0x5b;

fn hex(text: &str) -> Vec<u8> {
    text.split_whitespace().map(|b| u8::from_str_radix(b, 16).unwrap_or(0)).collect()
}

/// A sector as the mouse sent it: every read of 0x8100 to `id` in the transcript, 16 bytes at their
/// offset (the last read, at 0xEF, overlaps the one before it and must agree with it).
fn sector(id: u16) -> Vec<u8> {
    let path = format!("{}/tests/fixtures/transcripts/logitech-g502-x-plus-receiver.json", env!("CARGO_MANIFEST_DIR"));
    let transcript: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let events = transcript["events"].as_array().unwrap();
    let mut bytes: Vec<Option<u8>> = vec![None; SECTOR_SIZE];
    for pair in events.windows(2) {
        let (request, answer) = (&pair[0], &pair[1]);
        if request["op"] != "write" || answer["op"] != "read" {
            continue;
        }
        let mut sent = hex(request["data"].as_str().unwrap());
        sent.resize(8, 0);
        if sent[2..4] != [ONBOARD_PROFILES, READ_SECTOR] || u16::from_be_bytes([sent[4], sent[5]]) != id {
            continue;
        }
        let offset = u16::from_be_bytes([sent[6], sent[7]]) as usize;
        let mut got = hex(answer["data"].as_str().unwrap());
        got.resize(20, 0);
        for (i, &byte) in got[4..20].iter().enumerate().filter(|(i, _)| offset + i < SECTOR_SIZE) {
            let slot = &mut bytes[offset + i];
            assert!(slot.is_none_or(|b| b == byte), "sector {id:#06x}: two reads disagree at {}", offset + i);
            *slot = Some(byte);
        }
    }
    bytes
        .into_iter()
        .enumerate()
        .map(|(i, b)| b.unwrap_or_else(|| panic!("sector {id:#06x}: byte {i} not read")))
        .collect()
}

#[test]
fn the_crc_is_in_the_last_two_bytes_of_the_255() {
    let (list, profile) = (sector(0), sector(1));
    assert_eq!(crc_ccitt(&list[..253]), 0x8232);
    assert_eq!(crc_ccitt(&profile[..253]), 0x94fb);
    assert!(crc_is_valid(&list) && crc_is_valid(&profile));
    // Factory sectors have no CRC; profile 1 is factory profile 1 with its CRC.
    let mut factory = sector(0x0101);
    assert!(!crc_is_valid(&factory));
    write_crc(&mut factory);
    assert_eq!(factory, profile);
}

#[test]
fn the_profile_list_enables_profiles_1_and_2() {
    assert_eq!(parse_directory(&sector(0), 5), [true, true, false, false, false]);
}

#[test]
fn the_g502_x_plus_profile_of_issue_149_reads_as_its_tester_described() {
    let p = ProfileSettings::parse(&sector(1), BUTTONS).unwrap();
    // 1000 Hz; 800 (sniper), 1200, 1600 (default), 2400, 3200.
    assert_eq!(p.report_rate_ms, 1);
    assert_eq!(p.dpi_levels, [800, 1200, 1600, 2400, 3200]);
    assert_eq!((p.default_dpi_index, p.shift_dpi_index), (2, 0));
    let special = |function| ButtonAction::Special { function };
    assert_eq!(
        p.buttons,
        [
            ButtonAction::Mouse { button: 1 },
            ButtonAction::Mouse { button: 2 },
            ButtonAction::Mouse { button: 3 },
            ButtonAction::Mouse { button: 4 },
            special(SpecialFunction::DpiShift),
            ButtonAction::Mouse { button: 5 },
            special(SpecialFunction::TiltLeft),
            special(SpecialFunction::TiltRight),
            special(SpecialFunction::ProfileCycle),
            special(SpecialFunction::DpiUp),
            special(SpecialFunction::DpiDown),
        ]
    );
    assert!(p.gshift_buttons.iter().all(|b| *b == ButtonAction::Disabled));
    // Both LED blocks hold an effect id (0x0F) PeriHub does not know: kept byte for byte. The mouse's one
    // lighting zone is reported through 0x8071, which no profile block is tied to.
    let unknown = LedEffect::Raw { bytes: [0x0f, 0, 0, 0, 0, 0, 0, 0, 0x64, 0, 0] };
    assert_eq!(p.leds, [unknown.clone(), unknown]);
}

#[test]
fn what_lies_past_the_fields_perihub_reads_is_as_the_sources_place_it() {
    let s = sector(1);
    // Power save and power off timeouts (little-endian seconds) before the buttons.
    assert_eq!((u16::from_le_bytes([s[28], s[29]]), u16::from_le_bytes([s[30], s[31]])), (60, 300));
    // The name (160 to 207) is blank; the alternate LED blocks (230 and 241) hold effect 0x10, and
    // byte 252, which the sources call a custom animation byte for format 5, is 3. PeriHub never writes there.
    assert!(s[160..208].iter().all(|&b| b == 0xff));
    assert_eq!(s[230..252], [0x10, 0, 0, 0, 0, 0, 0, 0, 0x64, 0, 0, 0x10, 0, 0, 0, 0, 0, 0, 0, 0x64, 0, 0]);
    assert_eq!(s[252], 0x03);
}
