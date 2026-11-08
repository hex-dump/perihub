//! Corsair mice PeriHub recognizes on their USB cable, from the table in
//! `docs/protocols/corsair-bragi.md`. None has been tested on real hardware yet.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CorsairModel {
    pub product_id: u16,
    pub name: &'static str,
    /// LEDs the lighting data covers; 0 for mice without lighting.
    pub leds: u8,
    /// Packet size: 64 bytes, or 128 on a few mice.
    pub packet: usize,
}

const fn model(product_id: u16, name: &'static str, leds: u8) -> CorsairModel {
    CorsairModel { product_id, name, leds, packet: super::PACKET }
}

const fn big(product_id: u16, name: &'static str, leds: u8) -> CorsairModel {
    CorsairModel { product_id, name, leds, packet: 128 }
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
];

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
        assert!(MODELS.iter().all(|m| m.leds <= 15 && matches!(m.packet, 64 | 128)));
    }

    #[test]
    fn the_table_matches_the_protocol_page() {
        let page = include_str!("../../../../docs/protocols/corsair-bragi.md");
        for m in MODELS {
            let row = format!("| {} | `{:04x}` | {} | {} |", m.name, m.product_id, m.packet, m.leds);
            assert!(page.contains(&row), "missing from the page: {row}");
        }
    }
}
