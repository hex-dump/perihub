//! Corsair mice and keyboards PeriHub recognizes on their USB cable, from the tables in
//! `docs/protocols/corsair-bragi.md`. None has been tested on real hardware yet.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Mouse,
    Keyboard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CorsairModel {
    pub product_id: u16,
    pub name: &'static str,
    pub kind: Kind,
    /// LEDs the lighting data covers; 0 for mice without lighting.
    pub leds: u8,
    /// Packet size in bytes: 64 on most mice, up to 1024 on keyboards.
    pub packet: usize,
}

const fn model(product_id: u16, name: &'static str, leds: u8) -> CorsairModel {
    CorsairModel { product_id, name, kind: Kind::Mouse, leds, packet: super::PACKET }
}

const fn big(product_id: u16, name: &'static str, leds: u8) -> CorsairModel {
    CorsairModel { product_id, name, kind: Kind::Mouse, leds, packet: 128 }
}

const fn keyboard(product_id: u16, name: &'static str, packet: usize, leds: u8) -> CorsairModel {
    CorsairModel { product_id, name, kind: Kind::Keyboard, leds, packet }
}

pub const MODELS: &[CorsairModel] = &[
    model(0x1B93, "KATAR PRO", 1),
    model(0x1BAC, "KATAR PRO XT", 1),
    model(0x1B70, "M55 RGB PRO", 2),
    model(0x1B9E, "M65 RGB ULTRA", 3),
    model(0x1BF0, "M75", 2),
    model(0x1B79, "SABRE RGB PRO", 5),
    model(0x1B7A, "SABRE PRO CS", 1),
    // Wireless mice on their cable.
    model(0x1B4C, "IRONCLAW RGB WIRELESS", 6),
    model(0x2B32, "IRONCLAW WIRELESS SE", 6),
    model(0x1B80, "DARK CORE RGB PRO", 12),
    model(0x1B7E, "DARK CORE RGB PRO SE", 12),
    model(0x1B5E, "HARPOON RGB WIRELESS", 2),
    model(0x2B2E, "HARPOON V2", 2),
    model(0x1BB5, "M65 RGB ULTRA WIRELESS", 2),
    model(0x1BD6, "M65 RGB ULTRA WIRELESS", 2),
    model(0x1BF1, "M75 WIRELESS", 2),
    model(0x2B08, "M75 WIRELESS", 2),
    model(0x1BF2, "M75 AIR WIRELESS", 1),
    model(0x1BB8, "NIGHTSABRE WIRELESS", 15),
    model(0x1BDB, "SCIMITAR RGB ELITE WIRELESS", 4),
    model(0x2B22, "SCIMITAR ELITE WIRELESS SE", 4),
    model(0x1B98, "SABRE RGB PRO WIRELESS", 4),
    model(0x1BB2, "DARKSTAR RGB WIRELESS", 13),
    model(0x2B28, "SABRE V2 PRO", 0),
    model(0x2B29, "SABRE V2 PRO MG", 0),
    model(0x2B25, "SABRE V2 PRO CF", 0),
    // 128-byte packets.
    big(0x1BE3, "SCIMITAR RGB ELITE", 5),
    big(0x2B03, "M55", 1),
    // Keyboards.
    keyboard(0x1BA4, "K55 PRO RGB", 64, 5),
    keyboard(0x1BA1, "K55 RGB PRO XT", 64, 137),
    keyboard(0x1BFE, "K55 CORE RGB", 64, 10),
    keyboard(0x2B05, "K55 CORE RGB", 64, 10),
    keyboard(0x2B20, "K55 CORE TKL RGB", 64, 8),
    keyboard(0x1B6E, "K57 RGB WIRELESS", 64, 137),
    keyboard(0x1BA0, "K60 RGB PRO", 64, 123),
    keyboard(0x1BAD, "K60 RGB PRO LP", 64, 123),
    keyboard(0x2B10, "K65 PLUS WIRELESS", 64, 123),
    keyboard(0x2B11, "K65 PLUS WIRELESS", 64, 123),
    keyboard(0x2B23, "K65 PLUS WIRELESS", 64, 123),
    keyboard(0x1BD7, "K65 PRO MINI", 128, 130),
    keyboard(0x1BAF, "K65 RGB MINI", 1024, 124),
    keyboard(0x1BC3, "K65 RGB MINI", 1024, 124),
    keyboard(0x1BCF, "K65 RGB MINI", 1024, 124),
    keyboard(0x1BFD, "K70 CORE RGB", 64, 123),
    keyboard(0x1BFF, "K70 CORE RGB", 64, 123),
    keyboard(0x2B0A, "K70 CORE RGB", 64, 123),
    keyboard(0x2B01, "K70 CORE TKL", 64, 125),
    keyboard(0x2B02, "K70 CORE TKL WIRELESS", 64, 123),
    keyboard(0x1BC0, "K70 MAX", 128, 142),
    keyboard(0x1BB3, "K70 PRO", 1024, 142),
    keyboard(0x1BC4, "K70 PRO", 1024, 142),
    keyboard(0x1BC6, "K70 PRO", 1024, 142),
    keyboard(0x1BD4, "K70 PRO", 1024, 142),
    keyboard(0x1BB6, "K70 PRO MINI WIRELESS", 1024, 130),
    keyboard(0x2B14, "K70 PRO TKL", 256, 139),
    keyboard(0x1B73, "K70 RGB TKL", 1024, 130),
    keyboard(0x1BB9, "K70 RGB TKL CS", 1024, 130),
    keyboard(0x1B89, "K95 RGB PLATINUM XT", 64, 156),
    keyboard(0x1B7C, "K100 RGB", 1024, 194),
    keyboard(0x1B7D, "K100 RGB", 1024, 194),
    keyboard(0x1BC5, "K100 RGB", 1024, 194),
    keyboard(0x1BAB, "K100 AIR WIRELESS", 1024, 137),
    keyboard(0x2B04, "MAKR 75", 512, 137),
];

/// Wireless receivers (Slipstream and older ones of the same generation), from the protocol page.
pub const RECEIVERS: &[(u16, &str)] = &[
    (0x1BA6, "SLIPSTREAM WIRELESS USB Receiver"),
    (0x1BDC, "SLIPSTREAM WIRELESS USB Receiver"),
    (0x2B00, "SLIPSTREAM WIRELESS USB Receiver"),
    (0x1B65, "HARPOON RGB WIRELESS receiver"),
    (0x1B7F, "DARK CORE RGB PRO SE receiver"),
    (0x1B81, "DARK CORE RGB PRO receiver"),
    (0x1B66, "IRONCLAW RGB WIRELESS receiver"),
];

pub fn is_receiver(product_id: u16) -> bool {
    RECEIVERS.iter().any(|&(id, _)| id == product_id)
}

pub fn find(product_id: u16) -> Option<&'static CorsairModel> {
    MODELS.iter().find(|m| m.product_id == product_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn product_ids_are_unique_and_sizes_are_known() {
        let mut ids: Vec<u16> = MODELS.iter().map(|m| m.product_id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), MODELS.len());
        assert!(MODELS.iter().all(|m| matches!(m.packet, 64 | 128 | 256 | 512 | 1024)));
        assert!(MODELS.iter().filter(|m| m.kind == Kind::Mouse).all(|m| m.leds <= 15));
    }

    #[test]
    fn receivers_match_the_protocol_page() {
        let page = include_str!("../../../../docs/protocols/corsair-bragi.md");
        for (id, name) in RECEIVERS {
            assert!(page.contains(&format!("| {name} | `{id:04x}` |")), "{name} {id:04x}");
            assert!(find(*id).is_none(), "a receiver is not a device");
        }
    }

    #[test]
    fn the_tables_match_the_protocol_page() {
        let page = include_str!("../../../../docs/protocols/corsair-bragi.md");
        for m in MODELS {
            let row = format!("| {} | `{:04x}` | {} | {} |", m.name, m.product_id, m.packet, m.leds);
            assert!(page.contains(&row), "missing from the page: {row}");
        }
    }
}
