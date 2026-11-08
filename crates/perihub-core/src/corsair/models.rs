//! Corsair mice PeriHub recognizes: wired mice with 64-byte packets, from the table in
//! `docs/protocols/corsair-bragi.md`. None has been tested on real hardware yet.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CorsairModel {
    pub product_id: u16,
    pub name: &'static str,
    /// LEDs the lighting data covers.
    pub leds: u8,
}

const fn model(product_id: u16, name: &'static str, leds: u8) -> CorsairModel {
    CorsairModel { product_id, name, leds }
}

pub const MODELS: &[CorsairModel] = &[
    model(0x1B93, "KATAR PRO", 1),
    model(0x1BAC, "KATAR PRO XT", 1),
    model(0x1B70, "M55 RGB PRO", 2),
    model(0x1B9E, "M65 RGB ULTRA", 3),
    model(0x1BF0, "M75", 2),
    model(0x1B79, "SABRE RGB PRO", 5),
    model(0x1B7A, "SABRE PRO CS", 1),
];

pub fn find(product_id: u16) -> Option<&'static CorsairModel> {
    MODELS.iter().find(|m| m.product_id == product_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn product_ids_are_unique_and_every_mouse_has_leds() {
        let mut ids: Vec<u16> = MODELS.iter().map(|m| m.product_id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), MODELS.len());
        assert!(MODELS.iter().all(|m| (1..=12).contains(&m.leds)));
    }

    #[test]
    fn the_table_matches_the_protocol_page() {
        let page = include_str!("../../../../docs/protocols/corsair-bragi.md");
        for m in MODELS {
            let row = format!("| {} | `{:04x}` | {} |", m.name, m.product_id, m.leds);
            assert!(page.contains(&row), "missing from the page: {row}");
        }
    }
}
