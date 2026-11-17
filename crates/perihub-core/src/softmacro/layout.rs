//! Which key types each character with the keyboard layout in use, for typing text on Linux.
//!
//! The virtual keyboard sends key positions, and the desktop turns them into characters with its
//! layout. So PeriHub compiles the same layout with libxkbcommon and looks for the key, alone or
//! with Shift and AltGr, that types each character. Accented letters a layout has no key for can
//! be typed with a dead key (the accent, then the letter); other characters are skipped.

use std::collections::HashMap;

use xkbcommon::xkb;

/// Modifiers of a press, as bits of a HID keyboard report.
pub const SHIFT: u8 = 0x02;
pub const ALT_GR: u8 = 0x40;

/// One key press that types something: Linux input code and modifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Press {
    pub code: u16,
    pub modifiers: u8,
}

pub struct Layout {
    chars: HashMap<char, Press>,
    /// Dead keys by keysym (dead_grave, dead_acute…).
    dead: HashMap<u32, Press>,
}

const DEAD_GRAVE: u32 = 0xfe50;
const DEAD_ACUTE: u32 = 0xfe51;
const DEAD_CIRCUMFLEX: u32 = 0xfe52;
const DEAD_TILDE: u32 = 0xfe53;
const DEAD_DIAERESIS: u32 = 0xfe57;
const DEAD_CEDILLA: u32 = 0xfe5b;

/// Letter and accent of an accented Latin letter, e.g. 'è' → ('e', dead_grave).
fn decompose(c: char) -> Option<(char, u32)> {
    const TABLE: &[(&str, &str, u32)] = &[
        ("àèìòù", "aeiou", DEAD_GRAVE),
        ("áéíóúý", "aeiouy", DEAD_ACUTE),
        ("âêîôû", "aeiou", DEAD_CIRCUMFLEX),
        ("ãõñ", "aon", DEAD_TILDE),
        ("äëïöüÿ", "aeiouy", DEAD_DIAERESIS),
        ("ç", "c", DEAD_CEDILLA),
    ];
    let lower = c.to_lowercase().next()?;
    TABLE.iter().find_map(|(accented, bases, dead)| {
        let base = bases.chars().nth(accented.chars().position(|a| a == lower)?)?;
        Some((if c.is_uppercase() { base.to_ascii_uppercase() } else { base }, *dead))
    })
}

impl Layout {
    /// The layout the desktop types with (see [`crate::kblayout::xkb_names`]), US if none is found.
    pub fn current() -> Option<Self> {
        match crate::kblayout::xkb_names() {
            Some(names) => Self::new(&names.layout, &names.variant, &names.options),
            None => Self::new("us", "", ""),
        }
    }

    pub fn new(layout: &str, variant: &str, options: &str) -> Option<Self> {
        let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
        let options = (!options.is_empty()).then(|| options.to_string());
        let keymap = xkb::Keymap::new_from_names(
            &context,
            "evdev",
            "pc105",
            layout,
            variant,
            options,
            xkb::KEYMAP_COMPILE_NO_FLAGS,
        )?;
        let shift = 1 << keymap.mod_get_index(xkb::MOD_NAME_SHIFT);
        // AltGr (ISO_Level3_Shift) sets Mod5 on the usual layouts.
        let level3 = 1 << keymap.mod_get_index("Mod5");
        let mut state = xkb::State::new(&keymap);
        let (mut chars, mut dead) = (HashMap::new(), HashMap::new());
        // Fewer modifiers first, so that a character uses the simplest press that types it.
        for (mask, modifiers) in [(0, 0), (shift, SHIFT), (level3, ALT_GR), (shift | level3, SHIFT | ALT_GR)] {
            state.update_mask(mask, 0, 0, 0, 0, 0);
            for raw in keymap.min_keycode().raw().max(8)..=keymap.max_keycode().raw() {
                let key = xkb::Keycode::new(raw);
                let press = Press { code: (raw - 8) as u16, modifiers };
                let sym = state.key_get_one_sym(key).raw();
                if (0xfe50..=0xfe8f).contains(&sym) {
                    dead.entry(sym).or_insert(press);
                } else if let Some(c) = char::from_u32(state.key_get_utf32(key))
                    && (!c.is_control() || c == '\r' || c == '\t')
                {
                    chars.entry(c).or_insert(press);
                }
            }
        }
        Some(Self { chars, dead })
    }

    /// The presses that type `c`: one key, or a dead key and then the letter.
    pub fn presses(&self, c: char) -> Option<Vec<Press>> {
        let c = if c == '\n' { '\r' } else { c };
        if let Some(press) = self.chars.get(&c) {
            return Some(vec![*press]);
        }
        let (base, dead) = decompose(c)?;
        Some(vec![*self.dead.get(&dead)?, *self.chars.get(&base)?])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accents_decompose() {
        assert_eq!(decompose('è'), Some(('e', DEAD_GRAVE)));
        assert_eq!(decompose('É'), Some(('E', DEAD_ACUTE)));
        assert_eq!(decompose('ñ'), Some(('n', DEAD_TILDE)));
        assert_eq!(decompose('ç'), Some(('c', DEAD_CEDILLA)));
        assert_eq!(decompose('x'), None);
    }

    /// Needs the xkb data files, present on any Linux desktop.
    #[test]
    fn italian_and_uk_layouts() {
        let Some(it) = Layout::new("it", "", "") else { return };
        // Italian: è has its own key.
        assert_eq!(it.presses('a'), Some(vec![Press { code: 30, modifiers: 0 }]));
        assert_eq!(it.presses('A'), Some(vec![Press { code: 30, modifiers: SHIFT }]));
        assert_eq!(it.presses('è'), Some(vec![Press { code: 26, modifiers: 0 }]));
        // @ is on AltGr+ò, and also on AltGr+Q in xkb's Italian layout.
        let at = it.presses('@').expect("@ on it");
        assert!(at == [Press { code: 40, modifiers: ALT_GR }] || at == [Press { code: 16, modifiers: ALT_GR }]);
        let Some(uk) = Layout::new("gb", "extd", "") else { return };
        // UK extended: é with AltGr, è with the dead grave key (AltGr + backquote) then e.
        assert_eq!(uk.presses('é'), Some(vec![Press { code: 18, modifiers: ALT_GR }]));
        let grave = uk.presses('è').expect("è on gb extd");
        assert_eq!(grave.len(), 2);
        assert_eq!(grave[1], Press { code: 18, modifiers: 0 });
        assert_eq!(uk.presses('\n'), Some(vec![Press { code: 28, modifiers: 0 }]));
    }
}
