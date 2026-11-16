//! SteelSeries Apex 3 TKL (USB 1038:1622), configuration interface MI_01 (usage page 0xFFC0).
//!
//! RGB protocol reconstructed from the OpenRGB documentation; key map from the
//! gorkemgun/apex-3-tkl project (MIT). The code is written from scratch.

pub mod input;
pub mod keys;

use std::collections::BTreeMap;
use std::thread::sleep;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::color::Rgb;
use crate::error::{Error, Result};
use crate::peripheral::Hid;

pub const ZONES: usize = 8;
pub const MAX_BRIGHTNESS: u8 = 0x10;
pub const LAYERS: [u8; 2] = [0, 1];

const OUTPUT_LEN: usize = 65;
const FEATURE_LEN: usize = 257;
const READ_DELAY: Duration = Duration::from_millis(80);
const WRITE_DELAY: Duration = Duration::from_millis(250);
const BINDINGS_PER_QUERY: usize = 30;
/// The keyboard's name in the write log.
const DEVICE: &str = "SteelSeries Apex 3 TKL";

/// Commands sent as output reports (lighting, volatile).
enum OutputCommand<'a> {
    Colors(&'a [Rgb; ZONES]),
    Rainbow,
    Brightness(u8),
}

/// Commands sent as feature reports. These are the only ones allowed: in particular, the command
/// that reboots the keyboard into its bootloader cannot be expressed.
enum FeatureCommand<'a> {
    Version,
    Build,
    Brightness,
    Bindings { layer: u8, keys: &'a [u8] },
    SetBinding { layer: u8, source: u8, binding: [u8; 5] },
    Save,
}

impl FeatureCommand<'_> {
    fn encode(&self) -> Vec<u8> {
        match *self {
            Self::Version => vec![0x90, 0],
            Self::Build => vec![0x90, 1],
            Self::Brightness => vec![0xA3],
            Self::Bindings { layer, keys } => {
                let mut out = vec![0xAA, layer, keys.len() as u8];
                for &key in keys {
                    out.extend_from_slice(&[key, 0, 0, 0, 0, 0]);
                }
                out
            }
            Self::SetBinding { layer, source, binding } => {
                let mut out = vec![0x2A, layer, 1, source];
                out.extend_from_slice(&binding);
                out
            }
            Self::Save => vec![0x11],
        }
    }

    /// What a command that changes the keyboard does, for the write log.
    fn name(&self) -> &'static str {
        match self {
            Self::SetBinding { .. } => "key binding",
            Self::Save => "save key map",
            _ => "read",
        }
    }
}

/// Function of a key on one of the two layers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum KeyBinding {
    /// Emits this key (HID usage).
    Key { usage: u8 },
    /// Built-in factory function of the SteelSeries layer (brightness, Win lock…).
    Function { code: u8, param: u8 },
    /// No function: on the SteelSeries layer the normal key applies.
    None,
    /// Unknown value, kept as is.
    Raw { bytes: [u8; 5] },
}

impl KeyBinding {
    pub fn decode(b: [u8; 5]) -> Self {
        match b {
            [0x51, usage, 0, 0, 0] => Self::Key { usage },
            [0x62, code, param, 0, 0] => Self::Function { code, param },
            [0, 0, 0, 0, 0] => Self::None,
            bytes => Self::Raw { bytes },
        }
    }

    pub fn encode(self) -> [u8; 5] {
        match self {
            Self::Key { usage } => [0x51, usage, 0, 0, 0],
            Self::Function { code, param } => [0x62, code, param, 0, 0],
            Self::None => [0; 5],
            Self::Raw { bytes } => bytes,
        }
    }

    pub fn factory(layer: u8, source: u8) -> Self {
        if layer == 0 {
            Self::Key { usage: source }
        } else {
            keys::factory_function(source).map(Self::decode).unwrap_or(Self::None)
        }
    }
}

/// Full map of one layer: source usage → function.
pub type Layer = BTreeMap<u8, KeyBinding>;

pub struct Apex3Tkl {
    dev: Box<dyn Hid>,
}

impl Apex3Tkl {
    pub fn new(dev: impl Hid + 'static) -> Self {
        Self { dev: Box::new(dev) }
    }

    fn output(&self, command: OutputCommand) -> Result<()> {
        let mut packet = [0u8; OUTPUT_LEN];
        let kind = match command {
            OutputCommand::Colors(_) => "lighting colors",
            OutputCommand::Rainbow => "hardware rainbow",
            OutputCommand::Brightness(_) => "brightness",
        };
        match command {
            OutputCommand::Colors(colors) => {
                packet[1] = 0x21;
                packet[2] = 0xFF;
                for (i, color) in colors.iter().enumerate() {
                    packet[3 + 3 * i..6 + 3 * i].copy_from_slice(&color.to_bytes());
                }
            }
            OutputCommand::Rainbow => {
                packet[1] = 0x22;
                packet[2] = 0xFF;
            }
            OutputCommand::Brightness(level) => {
                packet[1] = 0x23;
                packet[2] = level.min(MAX_BRIGHTNESS);
            }
        }
        let used = match command {
            OutputCommand::Colors(_) => 3 + 3 * ZONES,
            _ => 3,
        };
        crate::writes::send(DEVICE, kind, &packet[1..used], || self.dev.write(&packet).map(|_| ()))
    }

    fn send_feature(&self, command: &FeatureCommand, delay: Duration) -> Result<[u8; FEATURE_LEN]> {
        let request = command.encode();
        let mut packet = [0u8; FEATURE_LEN];
        packet[1..1 + request.len()].copy_from_slice(&request);
        self.dev.send_feature(&packet)?;
        sleep(delay);
        let mut reply = [0u8; FEATURE_LEN];
        let n = self.dev.get_feature(&mut reply)?;
        if n < 3 || reply[1] != request[0] {
            return Err(Error::Protocol(format!(
                "reply {:#04x} to command {:#04x} (is SteelSeries GG running?)",
                reply[1], request[0]
            )));
        }
        Ok(reply)
    }

    fn read(&self, command: &FeatureCommand) -> Result<[u8; FEATURE_LEN]> {
        // Replies sometimes arrive late: retry, like the official software does.
        let mut last = Error::Timeout;
        for _ in 0..3 {
            match self.send_feature(command, READ_DELAY) {
                Ok(reply) => return Ok(reply),
                Err(error) => last = error,
            }
        }
        Err(last)
    }

    fn write(&self, command: &FeatureCommand) -> Result<()> {
        crate::writes::send(DEVICE, command.name(), &command.encode(), || {
            let reply = self.send_feature(command, WRITE_DELAY)?;
            if reply[2] != 0 {
                return Err(Error::Protocol(format!("command rejected (status {:#04x})", reply[2])));
            }
            Ok(())
        })
    }

    fn read_string(&self, command: &FeatureCommand) -> Result<String> {
        let reply = self.read(command)?;
        let text = &reply[2..];
        let end = text.iter().position(|&b| b == 0).unwrap_or(text.len());
        Ok(String::from_utf8_lossy(&text[..end]).to_string())
    }

    pub fn firmware_version(&self) -> Result<String> {
        let version = self.read_string(&FeatureCommand::Version)?;
        let build = self.read_string(&FeatureCommand::Build)?;
        Ok(format!("{version}{build}"))
    }

    // ---- Lighting (volatile: lost when the keyboard is unplugged) ----

    pub fn set_colors(&self, colors: &[Rgb; ZONES]) -> Result<()> {
        self.output(OutputCommand::Colors(colors))
    }

    pub fn set_hardware_rainbow(&self) -> Result<()> {
        self.output(OutputCommand::Rainbow)
    }

    pub fn set_brightness(&self, level: u8) -> Result<()> {
        self.output(OutputCommand::Brightness(level))
    }

    pub fn brightness(&self) -> Result<u8> {
        Ok(self.read(&FeatureCommand::Brightness)?[2].min(MAX_BRIGHTNESS))
    }

    // ---- Key map (stored in the keyboard) ----

    pub fn read_layer(&self, layer: u8) -> Result<Layer> {
        check_layer(layer)?;
        let slots: Vec<u8> = (0..=255u8).filter(|&u| keys::is_mappable_slot(u)).collect();
        let mut map = Layer::new();
        for chunk in slots.chunks(BINDINGS_PER_QUERY) {
            let mut attempt = 0;
            loop {
                let reply = self.read(&FeatureCommand::Bindings { layer, keys: chunk })?;
                // reply[2] is a byte the firmware leaves uninitialized; entries start at reply[3].
                let entries = &reply[3..];
                let ok = chunk.iter().enumerate().all(|(i, &key)| entries[6 * i] == key);
                if ok {
                    for (i, &key) in chunk.iter().enumerate() {
                        let raw: [u8; 5] = entries[6 * i + 1..6 * i + 6].try_into().unwrap();
                        map.insert(key, KeyBinding::decode(raw));
                    }
                    break;
                }
                attempt += 1;
                if attempt >= 4 {
                    return Err(Error::Protocol("inconsistent keymap response".into()));
                }
                sleep(Duration::from_millis(150));
            }
        }
        Ok(map)
    }

    /// Sets a key in RAM only. It must be committed with [`Apex3Tkl::save`].
    fn set_binding_volatile(&self, layer: u8, source: u8, binding: KeyBinding) -> Result<()> {
        check_layer(layer)?;
        validate_binding(layer, source, binding)?;
        self.write(&FeatureCommand::SetBinding { layer, source, binding: binding.encode() })
    }

    fn save(&self) -> Result<()> {
        self.write(&FeatureCommand::Save)
    }

    /// Applies the changes transactionally: writes to RAM, reads back and verifies, and only
    /// then saves to the keyboard's memory. If anything does not match, restores the previous values.
    /// Returns the map read after saving.
    pub fn apply_changes(&self, changes: &[(u8, u8, KeyBinding)]) -> Result<[Layer; 2]> {
        crate::writes::refuse_if_read_only()?;
        for &(layer, source, binding) in changes {
            check_layer(layer)?;
            validate_binding(layer, source, binding)?;
        }
        let before = [self.read_layer(0)?, self.read_layer(1)?];
        let mut expected = before.clone();
        let mut written = Vec::new();
        let result = (|| {
            for &(layer, source, binding) in changes {
                let current = before[layer as usize][&source];
                if current == binding {
                    continue;
                }
                // An unknown current value could not be restored: better to stop here.
                validate_binding(layer, source, current)?;
                written.push((layer, source, current));
                self.set_binding_volatile(layer, source, binding)?;
                expected[layer as usize].insert(source, binding);
            }
            let after = [self.read_layer(0)?, self.read_layer(1)?];
            if after != expected {
                return Err(Error::Protocol("the keyboard did not apply the changes as expected".into()));
            }
            Ok(())
        })();
        if let Err(error) = result {
            for &(layer, source, binding) in written.iter().rev() {
                let _ = self.set_binding_volatile(layer, source, binding);
            }
            return Err(error);
        }
        if written.is_empty() {
            return Ok(before);
        }
        self.save()?;
        let saved = [self.read_layer(0)?, self.read_layer(1)?];
        if saved != expected {
            return Err(Error::Protocol("the keymap read back after saving is different".into()));
        }
        Ok(saved)
    }
}

fn check_layer(layer: u8) -> Result<()> {
    if LAYERS.contains(&layer) { Ok(()) } else { Err(Error::Invalid(format!("layer {layer}"))) }
}

/// Values allowed for writing: a single valid key, or the factory value.
pub fn validate_binding(layer: u8, source: u8, binding: KeyBinding) -> Result<()> {
    if !keys::is_remappable_source(source) {
        return Err(Error::Invalid(format!("key {source:#04x} cannot be remapped")));
    }
    if binding == KeyBinding::factory(layer, source) {
        return Ok(());
    }
    match binding {
        KeyBinding::Key { usage } if keys::is_valid_target(usage) => Ok(()),
        _ => Err(Error::Invalid(format!("this function is not allowed for key {source:#04x}"))),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::peripheral::fake::FakeHid;

    /// A keyboard that answers every feature report: firmware "1", brightness 8, every binding
    /// empty, every change accepted.
    pub(crate) fn fake_keyboard() -> FakeHid {
        let hid = FakeHid::new(|_| Vec::new());
        hid.0.lock().unwrap().on_feature = Some(Box::new(|request: &[u8]| match request[0] {
            0x90 => vec![0x90, b'1', 0],
            0xA3 => vec![0xA3, 8],
            0xAA => [&[0xAA, 0][..], &request[3..]].concat(),
            command => vec![command, 0],
        }));
        hid
    }

    #[test]
    fn read_only_mode_refuses_lighting_and_key_map_writes() {
        let hid = fake_keyboard();
        let dev = Apex3Tkl::new(hid.clone());
        let read_only = crate::writes::test::read_only_mode();
        let refused = |result: Result<()>| assert!(matches!(result, Err(Error::ReadOnly)), "{result:?}");
        let binding = KeyBinding::Key { usage: 0x05 };
        refused(dev.set_colors(&[Rgb { r: 1, g: 2, b: 3 }; ZONES]));
        refused(dev.set_hardware_rainbow());
        refused(dev.set_brightness(4));
        refused(dev.set_binding_volatile(0, 0x04, binding));
        refused(dev.save());
        refused(dev.apply_changes(&[(0, 0x04, binding)]).map(|_| ()));
        assert!(hid.sent().is_empty(), "nothing reached the keyboard");
        assert_eq!(dev.brightness().unwrap(), 8, "reads still work");
        assert!(hid.sent().iter().all(|r| r[0] == 0xA3));
        let log = crate::writes::test::entries_of(DEVICE);
        let kinds: Vec<&str> = log.iter().map(|e| e.kind.as_str()).collect();
        assert_eq!(kinds, ["lighting colors", "hardware rainbow", "brightness", "key binding", "save key map"]);

        drop(read_only);
        dev.set_brightness(4).unwrap();
        assert_eq!(hid.sent().last().unwrap()[..2], [0x23, 4]);
        assert_eq!(crate::writes::test::entries_of(DEVICE).last().unwrap().result, "ok");
    }

    #[test]
    fn slot_table_has_114_entries() {
        assert_eq!((0..=255u8).filter(|&u| keys::is_mappable_slot(u)).count(), 114);
        assert!(!keys::is_remappable_source(keys::STEELSERIES_KEY));
    }

    #[test]
    fn bindings_round_trip() {
        for raw in [[0x51, 0x04, 0, 0, 0], [0x62, 0x03, 0x01, 0, 0], [0; 5], [0x7F, 1, 2, 3, 4]] {
            assert_eq!(KeyBinding::decode(raw).encode(), raw);
        }
    }

    #[test]
    fn only_safe_bindings_are_writable() {
        assert!(validate_binding(0, 0x39, KeyBinding::Key { usage: 0x29 }).is_ok());
        assert!(validate_binding(1, 0x44, KeyBinding::factory(1, 0x44)).is_ok());
        assert!(validate_binding(1, 0x04, KeyBinding::None).is_ok());
        assert!(validate_binding(0, 0x04, KeyBinding::None).is_err());
        assert!(validate_binding(0, 0x04, KeyBinding::Function { code: 1, param: 0 }).is_err());
        assert!(validate_binding(0, keys::STEELSERIES_KEY, KeyBinding::Key { usage: 0x04 }).is_err());
        assert!(validate_binding(0, 0x65, KeyBinding::Key { usage: 0x04 }).is_err());
        assert!(validate_binding(0, 0x04, KeyBinding::Key { usage: 0x66 }).is_err());
    }

    #[test]
    fn no_bootloader_command_can_be_built() {
        let commands = [
            FeatureCommand::Version,
            FeatureCommand::Build,
            FeatureCommand::Brightness,
            FeatureCommand::Bindings { layer: 0, keys: &[0x04] },
            FeatureCommand::SetBinding { layer: 0, source: 0x04, binding: [0x51, 0x04, 0, 0, 0] },
            FeatureCommand::Save,
        ];
        for command in commands {
            assert!([0x90, 0xA3, 0xAA, 0x2A, 0x11].contains(&command.encode()[0]));
        }
    }
}
