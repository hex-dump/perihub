//! The G502 Proteus Core of issue #159 (`046d:c07d` with its USB cable, firmware U 88.02.0017): profile format 1
//! in 256-byte sectors, the memory libratbag calls the G402's. PeriHub 0.9.8 read its profile list, profile 1 and
//! factory profile 1 for the test report; the sectors are put back together from that transcript
//! (`fixtures/transcripts/logitech-g502-proteus-core.json`) and checked byte by byte. The tester wrote that the
//! DPI levels, the default level, the profiles and the buttons PeriHub shows match the mouse, not the values:
//! what is checked here is that every field PeriHub reads sits where formats 2 to 4 have it and that the rest
//! of the sector is as the sources place it. Nothing here says that format 1 can be written.

use perihub_core::hidpp::profile::{
    ButtonAction, LedEffect, ProfileSettings, SpecialFunction, crc_ccitt, crc_is_valid, parse_directory, write_crc,
};
use serde_json::Value;

const SECTOR_SIZE: usize = 256;
const BUTTONS: usize = 11;
/// Index of 0x8100 (onboard profiles) in this mouse's feature table, and its read function (5).
const ONBOARD_PROFILES: u8 = 0x0e;
const READ_SECTOR: u8 = 0x5b;

fn hex(text: &str) -> Vec<u8> {
    text.split_whitespace().map(|b| u8::from_str_radix(b, 16).unwrap_or(0)).collect()
}

/// A sector as the mouse sent it: every read of 0x8100 to `id` in the transcript, 16 bytes at their
/// offset (a sector of 256 bytes is 16 reads of 16, with nothing overlapping).
fn sector(id: u16) -> Vec<u8> {
    let path = format!("{}/tests/fixtures/transcripts/logitech-g502-proteus-core.json", env!("CARGO_MANIFEST_DIR"));
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
fn the_crc_is_in_the_last_two_bytes_of_the_256() {
    let (list, profile) = (sector(0), sector(1));
    assert_eq!((list[254], list[255]), (0xda, 0x84));
    assert_eq!((profile[254], profile[255]), (0x28, 0x9d));
    assert_eq!(crc_ccitt(&list[..254]), 0xda84);
    assert_eq!(crc_ccitt(&profile[..254]), 0x289d);
    assert!(crc_is_valid(&list) && crc_is_valid(&profile));
    // Factory sectors have no CRC; profile 1 is factory profile 1 with its CRC.
    let mut factory = sector(0x0101);
    assert!(!crc_is_valid(&factory));
    write_crc(&mut factory);
    assert_eq!(factory, profile);
}

#[test]
fn the_profile_list_enables_the_three_profiles() {
    let list = sector(0);
    // Three entries of four bytes (sector, enabled, 0), then the end marker FF FF 00 00, then blank.
    assert_eq!(list[..16], [0, 1, 1, 0, 0, 2, 1, 0, 0, 3, 1, 0, 0xff, 0xff, 0, 0]);
    assert!(list[16..254].iter().all(|&b| b == 0xff));
    assert_eq!(parse_directory(&list, 3), [true, true, true]);
}

#[test]
fn the_g502_proteus_core_profile_of_issue_159_reads_as_the_mouse_stores_it() {
    let p = ProfileSettings::parse(&sector(1), BUTTONS).unwrap();
    // 1000 Hz; four DPI levels (the fifth slot is 0): 1200, 2400 (default), 3200, 6400; shift on the first.
    assert_eq!(p.report_rate_ms, 1);
    assert_eq!(p.dpi_levels, [1200, 2400, 3200, 6400]);
    assert_eq!((p.default_dpi_index, p.shift_dpi_index), (1, 0));
    let special = |function| ButtonAction::Special { function };
    assert_eq!(
        p.buttons,
        [
            ButtonAction::Mouse { button: 1 },
            ButtonAction::Mouse { button: 2 },
            ButtonAction::Mouse { button: 3 },
            ButtonAction::Mouse { button: 4 },
            ButtonAction::Mouse { button: 5 },
            special(SpecialFunction::DpiShift),
            special(SpecialFunction::DpiDown),
            special(SpecialFunction::DpiUp),
            special(SpecialFunction::ProfileCycle),
            special(SpecialFunction::TiltRight),
            special(SpecialFunction::TiltLeft),
        ]
    );
    assert!(p.gshift_buttons.iter().all(|b| *b == ButtonAction::Disabled));
    // The two LED blocks (208 to 229) are blank: a format 1 profile holds no light settings there.
    let blank = LedEffect::decode(&[0xff; 11]);
    assert_eq!(p.leds, [blank.clone(), blank]);
}

#[test]
fn saving_the_profile_unchanged_leaves_every_byte_as_it_is() {
    // What PeriHub writes on top of the sector (rate, levels, 11 buttons, 11 G-shift slots, two LED blocks, the
    // CRC) puts back what it read: the blank G-shift slots, LED blocks and the five blank button slots stay blank.
    let before = sector(1);
    let mut after = before.clone();
    ProfileSettings::parse(&before, BUTTONS).unwrap().apply(&mut after).unwrap();
    assert_eq!(after, before);
}

#[test]
fn changing_a_dpi_level_changes_its_two_bytes_and_the_crc_only() {
    let before = sector(1);
    let mut settings = ProfileSettings::parse(&before, BUTTONS).unwrap();
    settings.dpi_levels[2] = 3600;
    let mut after = before.clone();
    settings.apply(&mut after).unwrap();
    let changed: Vec<usize> = (0..SECTOR_SIZE).filter(|&i| after[i] != before[i]).collect();
    // Level 3 sits in bytes 7 and 8 (3200 = 0C80, 3600 = 0E10); the CRC is in 254 and 255.
    assert_eq!(changed, [7, 8, 254, 255]);
    assert_eq!(after[7..9], 3600u16.to_le_bytes());
    assert!(crc_is_valid(&after));
}

#[test]
fn what_lies_past_the_fields_perihub_reads_is_as_the_sources_place_it() {
    let s = sector(1);
    // The raw slots, byte for byte: the sixteen button slots (32 to 95) are eleven bindings and five blank slots.
    assert_eq!(
        s[32..96],
        [
            0x80, 0x01, 0x00, 0x01, 0x80, 0x01, 0x00, 0x02, 0x80, 0x01, 0x00, 0x04, 0x80, 0x01, 0x00, 0x08, //
            0x80, 0x01, 0x00, 0x10, 0x90, 0x07, 0xff, 0xff, 0x90, 0x04, 0xff, 0xff, 0x90, 0x03, 0xff, 0xff, //
            0x90, 0x0a, 0xff, 0xff, 0x90, 0x02, 0xff, 0xff, 0x90, 0x01, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, //
            0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        ]
    );
    // The DPI slots (3 to 12, little-endian) and the bytes around them: the colour (13 to 15) and the power
    // mode (16) of the sources are blank, byte 17 (angle snapping in libratbag) is 0, 18 to 31 (the reserved
    // bytes and the two power timeouts) are blank.
    assert_eq!(s[..3], [1, 1, 0]);
    assert_eq!(s[3..13], [0xb0, 0x04, 0x60, 0x09, 0x80, 0x0c, 0x00, 0x19, 0x00, 0x00]);
    assert_eq!(s[13..17], [0xff; 4]);
    assert_eq!(s[17], 0);
    assert!(s[18..32].iter().all(|&b| b == 0xff));
    // Everything from the G-shift layer to the CRC is blank: the G-shift slots (96 to 159), the name (160 to
    // 207), both LED blocks and the alternate ones (208 to 251) and the two bytes after them (252, 253).
    assert!(s[96..254].iter().all(|&b| b == 0xff));
}
