//! The lighting of a HID++ 2.0 device that PeriHub knows by its features, such as the G915 X
//! keyboards (issue #30): brightness (0x8040), RGB effects by zone (0x8071), per-key lighting
//! (0x8081) and the keyboard layout (0x4540). Read only: every call here is a read (see
//! `READ_ONLY_CALLS` and `READ_ONLY_GETS`), and the replies are kept as sent, for test reports.
//!
//! What each call and byte means is in docs/protocols/logitech-keyboard-lighting.md, with its sources.

use serde::Serialize;

use super::{HidppDevice, feature};
use crate::error::{Error, HidppErrorCode, Result};

/// At most this many zones and effects per zone are read: the device says how many it has.
const MAX_ZONES: u8 = 16;
const MAX_EFFECTS: u8 = 32;
/// 0x8071 function 3: the startup and shutdown effects, by capability id.
const STARTUP: u16 = 0x0001;
const SHUTDOWN: u16 = 0x0040;

/// Brightness of all the lights (0x8040 function 0).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Brightness {
    pub min: u16,
    pub max: u16,
    /// The levels the brightness key goes through, as the device reports them (low 4 bits).
    pub steps: u8,
    /// The lights can be turned off and on apart from their brightness.
    pub on_off: bool,
}

/// An effect a lighting zone can show (0x8071 function 0, zone and effect).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ZoneEffect {
    /// Its place in the zone's list: what the device is told to show.
    pub index: u8,
    /// Which effect it is, such as 1 for a fixed colour.
    pub id: u16,
    /// Settings it takes: 0x0001 colour, 0x0002 fade, 0x0004 period, 0x0010 direction.
    pub caps: u16,
    pub period_ms: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EffectZone {
    pub index: u8,
    /// 1 primary, 2 logo, 3 left side, 4 right side, 5 combined, 6 to 11 primary 1 to 6.
    pub location: u16,
    pub effects: Vec<ZoneEffect>,
}

/// The effect the device plays when it is turned on or off (0x8071 function 3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PowerEffect {
    pub enabled: bool,
    /// Two colours, "#rrggbb".
    pub colors: [String; 2],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RgbEffects {
    pub zones: Vec<EffectZone>,
    pub startup: Option<PowerEffect>,
    pub shutdown: Option<PowerEffect>,
    /// After how long without use the lights dim, and turn off, in seconds (0x8071 function 7).
    pub idle_s: Option<u16>,
    pub sleep_s: Option<u16>,
}

/// What PeriHub read of a device's lighting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct Lighting {
    pub brightness: Option<Brightness>,
    pub effects: Option<RgbEffects>,
    /// The keys whose colour can be set one by one (0x8081), by the ids the device reports.
    pub keys: Vec<u8>,
    /// The layout the keyboard reports (0x4540): 1 ANSI, 2 ISO and others.
    pub layout: Option<u8>,
    /// Every reply as it came, such as "8071 0 (ff ff 00): …", for test reports.
    pub raw: Vec<String>,
    /// What could not be read.
    pub errors: Vec<String>,
}

/// The lighting as it is now: it changes while the device is in use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LightingNow {
    pub brightness: Option<u16>,
    /// The lights are on (only for a device that can turn them off).
    pub on: Option<bool>,
    /// Who drives the lights (0x8071 function 5): 0 the device itself, 3 a program.
    pub control: Option<u8>,
}

fn u16_at(reply: &[u8; 16], at: usize) -> u16 {
    u16::from_be_bytes([reply[at], reply[at + 1]])
}

fn color(bytes: &[u8]) -> String {
    format!("#{:02x}{:02x}{:02x}", bytes[0], bytes[1], bytes[2])
}

/// The keys in the 0x8081 bitmap: bit i (from the low bit of the first byte) is key id i, over the
/// pages one after the other, each with its bitmap in bytes 2 to 15.
pub fn keys_from_pages(pages: &[[u8; 16]]) -> Vec<u8> {
    let bitmap: Vec<u8> = pages.iter().flat_map(|page| page[2..].iter().copied()).collect();
    (1..=254u8).filter(|&i| bitmap.get(i as usize / 8).is_some_and(|byte| byte >> (i % 8) & 1 == 1)).collect()
}

impl Lighting {
    /// Reads what the device's lighting features tell. `has` says which features it lists.
    /// A device that falls asleep meanwhile gives [`Error::Unreachable`]; other failures are kept
    /// in `errors`.
    pub fn read(dev: &mut HidppDevice, has: impl Fn(u16) -> bool) -> Result<Self> {
        let mut out = Self::default();
        if has(feature::BRIGHTNESS_CONTROL)
            && let Some(r) = out.call(dev, feature::BRIGHTNESS_CONTROL, 0, &[])?
        {
            out.brightness = Some(Brightness {
                max: u16_at(&r, 0),
                steps: r[2] & 0x0F,
                on_off: r[3] & 0x04 != 0,
                min: u16_at(&r, 4),
            });
        }
        if has(feature::RGB_EFFECTS) {
            out.effects = out.read_effects(dev)?;
        }
        if has(feature::PER_KEY_LIGHTING_V2) {
            let mut pages = Vec::new();
            for page in 0..3 {
                match out.call(dev, feature::PER_KEY_LIGHTING_V2, 0, &[0, page])? {
                    Some(r) => pages.push(r),
                    None => break,
                }
            }
            out.keys = keys_from_pages(&pages);
        }
        if has(feature::KEYBOARD_LAYOUT) {
            out.layout = out.call(dev, feature::KEYBOARD_LAYOUT, 0, &[])?.map(|r| r[0]);
        }
        // Features of the newer keyboards that no public document describes: what function 0 answers,
        // and the mode 0x8101 function 6 reads (0 asks), kept in `raw` for test reports. Not 0x4523
        // (game mode): its function 0 is what G HUB writes the disabled keys with (issue #30 capture).
        let undocumented: [(u16, u8, &[u8]); 5] = [
            (feature::PROFILE_MANAGEMENT, 0, &[]),
            (feature::PROFILE_MANAGEMENT, 6, &[0]),
            (feature::CONTROL_LIST, 0, &[]),
            (feature::FULL_KEY_CUSTOMIZATION, 0, &[]),
            (feature::LOGI_MODIFIERS, 0, &[]),
        ];
        for (id, function, params) in undocumented {
            if has(id) {
                out.call(dev, id, function, params)?;
            }
        }
        Ok(out)
    }

    fn read_effects(&mut self, dev: &mut HidppDevice) -> Result<Option<RgbEffects>> {
        let Some(info) = self.call(dev, feature::RGB_EFFECTS, 0, &[0xFF, 0xFF, 0])? else { return Ok(None) };
        let mut effects = RgbEffects { zones: Vec::new(), startup: None, shutdown: None, idle_s: None, sleep_s: None };
        for zone in 0..info[2].min(MAX_ZONES) {
            let Some(z) = self.call(dev, feature::RGB_EFFECTS, 0, &[zone, 0xFF, 0])? else { continue };
            let mut found = EffectZone { index: zone, location: u16_at(&z, 2), effects: Vec::new() };
            for effect in 0..z[4].min(MAX_EFFECTS) {
                if let Some(e) = self.call(dev, feature::RGB_EFFECTS, 0, &[zone, effect, 0])? {
                    found.effects.push(ZoneEffect {
                        index: effect,
                        id: u16_at(&e, 2),
                        caps: u16_at(&e, 4),
                        period_ms: u16_at(&e, 6),
                    });
                }
            }
            effects.zones.push(found);
        }
        for (cap, slot) in [(STARTUP, &mut effects.startup), (SHUTDOWN, &mut effects.shutdown)] {
            let [high, low] = cap.to_be_bytes();
            // Bytes 0 to 2 repeat the request; then on (1) or off (2), and two colours.
            // A keyboard that has no such effect answers "invalid argument" (a G915 with 0x8071 version 0 does, to both,
            // issue #157): that is its way of saying "not here", kept in the replies but not among what could not be read.
            if let Some(r) = self.call_asking(dev, feature::RGB_EFFECTS, 3, &[0, high, low], false)? {
                *slot = Some(PowerEffect { enabled: r[3] == 1, colors: [color(&r[4..7]), color(&r[7..10])] });
            }
        }
        if let Some(r) = self.call(dev, feature::RGB_EFFECTS, 7, &[0])? {
            effects.idle_s = Some(u16_at(&r, 3));
            effects.sleep_s = Some(u16_at(&r, 5));
        }
        Ok(Some(effects))
    }

    /// One read, kept in `raw`. A device error is kept in `errors` and gives `None`.
    fn call(&mut self, dev: &mut HidppDevice, feature: u16, function: u8, params: &[u8]) -> Result<Option<[u8; 16]>> {
        self.call_asking(dev, feature, function, params, true)
    }

    /// The same, for a read the device may answer with "invalid argument" (0x02) when it has nothing of that kind:
    /// with `refusal_is_error` false that answer is kept in `raw` only; any other error is still in `errors`.
    fn call_asking(
        &mut self,
        dev: &mut HidppDevice,
        feature: u16,
        function: u8,
        params: &[u8],
        refusal_is_error: bool,
    ) -> Result<Option<[u8; 16]>> {
        let asked = if params.is_empty() { String::new() } else { format!(" ({})", crate::peripheral::hex(params)) };
        match dev.call(feature, function, params) {
            Ok(reply) => {
                // Trailing zeros left out: the report fits in the test form's address.
                let used = reply.iter().rposition(|&b| b != 0).map_or(1, |i| i + 1);
                self.raw.push(format!("{feature:04x} {function}{asked}: {}", crate::peripheral::hex(&reply[..used])));
                Ok(Some(reply))
            }
            Err(Error::Unreachable) => Err(Error::Unreachable),
            Err(error) => {
                self.raw.push(format!("{feature:04x} {function}{asked}: {error}"));
                let refused = matches!(error, Error::Hidpp(HidppErrorCode(0x02)));
                if refusal_is_error || !refused {
                    self.errors.push(format!("{feature:04x} function {function}{asked}: {error}"));
                }
                Ok(None)
            }
        }
    }
}

impl LightingNow {
    pub fn read(dev: &mut HidppDevice, lighting: &Lighting, has_effects: bool) -> Result<Self> {
        let mut now = Self { brightness: None, on: None, control: None };
        if let Some(brightness) = &lighting.brightness {
            now.brightness = Some(u16_at(&dev.call(feature::BRIGHTNESS_CONTROL, 1, &[])?, 0));
            if brightness.on_off {
                now.on = Some(dev.call(feature::BRIGHTNESS_CONTROL, 3, &[])?[0] & 1 == 1);
            }
        }
        if has_effects {
            // Function 5 with a first byte 0 only asks; byte 0 of the reply repeats it.
            now.control = Some(dev.call(feature::RGB_EFFECTS, 5, &[0])?[1]);
        }
        Ok(now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hidpp::receiver::UnsupportedDevice;
    use crate::hidpp::tests::{FakeDevice, reply};

    /// A keyboard with the lighting features of a G915 X LS in slot 1 (the replies are made up in the
    /// layouts of the specification), and 0x8101, of which PeriHub only reads functions 0 and 6.
    const FEATURES: &[u16] = &[
        feature::ROOT,
        feature::FEATURE_SET,
        feature::DEVICE_NAME,
        feature::BRIGHTNESS_CONTROL,
        feature::RGB_EFFECTS,
        feature::PER_KEY_LIGHTING_V2,
        feature::KEYBOARD_LAYOUT,
        feature::PROFILE_MANAGEMENT,
    ];

    fn keyboard() -> FakeDevice {
        FakeDevice::new(|request| {
            let (index, function, p) = (request[2] as usize, request[3] >> 4, &request[4..]);
            let payload: Vec<u8> = match (FEATURES.get(index).copied(), function) {
                (Some(feature::ROOT), 1) => vec![4, 2, request[6]],
                (Some(feature::ROOT), 0) => {
                    let id = u16::from_be_bytes([p[0], p[1]]);
                    vec![FEATURES.iter().position(|&f| f == id).unwrap_or(0) as u8]
                }
                (Some(feature::FEATURE_SET), 0) => vec![FEATURES.len() as u8 - 1],
                (Some(feature::FEATURE_SET), 1) => {
                    let [high, low] = FEATURES[p[0] as usize].to_be_bytes();
                    vec![high, low, 0, 0]
                }
                (Some(feature::DEVICE_NAME), 0) => vec![9],
                (Some(feature::DEVICE_NAME), 1) => b"G915 X LS".to_vec(),
                (Some(feature::DEVICE_NAME), 2) => vec![0],
                // Up to 100, 5 levels, can be turned off, at least 0; now 50 and on.
                (Some(feature::BRIGHTNESS_CONTROL), 0) => vec![0, 100, 0x05, 0x04, 0, 0],
                (Some(feature::BRIGHTNESS_CONTROL), 1) => vec![0, 50],
                (Some(feature::BRIGHTNESS_CONTROL), 3) => vec![1],
                // One zone, the keys (location 1), with two effects: fixed colour and cycle.
                (Some(feature::RGB_EFFECTS), 0) => match (p[0], p[1]) {
                    (0xFF, 0xFF) => vec![0xFF, 0xFF, 1, 0, 0, 0, 1],
                    (0, 0xFF) => vec![0, 0xFF, 0, 1, 2, 0],
                    (0, 0) => vec![0, 0, 0, 0x01, 0, 0x01, 0, 0],
                    (0, 1) => vec![0, 1, 0, 0x03, 0, 0x04, 0x27, 0x10],
                    _ => return vec![error(request, 2)],
                },
                (Some(feature::RGB_EFFECTS), 3) => match p[2] {
                    0x01 => vec![0, 0, 0x01, 1, 0xFF, 0, 0x81, 0x80, 0xAA, 0xFF],
                    _ => return vec![error(request, 2)],
                },
                (Some(feature::RGB_EFFECTS), 5) => vec![0, 0],
                (Some(feature::RGB_EFFECTS), 7) => vec![0, 0, 0, 0, 60, 1, 0x2C],
                // Keys 4 to 7 and 9 on page 0, key 112 (the first of page 1) on page 1.
                (Some(feature::PER_KEY_LIGHTING_V2), 0) => match p[1] {
                    0 => vec![0, 0, 0xF0, 0x02],
                    1 => vec![0, 1, 0x01],
                    _ => vec![0, 2],
                },
                (Some(feature::KEYBOARD_LAYOUT), 0) => vec![1],
                (Some(feature::PROFILE_MANAGEMENT), 0) => vec![1, 1, 0, 0x80],
                (Some(feature::PROFILE_MANAGEMENT), 6) => vec![1, 0, 1],
                _ => return vec![error(request, 7)],
            };
            vec![reply(request, &payload)]
        })
    }

    fn error(request: &[u8], code: u8) -> Vec<u8> {
        let mut report = vec![0x11, request[1], 0xFF, request[2], request[3], code];
        report.resize(20, 0);
        report
    }

    #[test]
    fn a_keyboard_s_lighting_is_read_with_reads_only() {
        let fake = keyboard();
        let written = fake.written.clone();
        let mut dev = HidppDevice::behind_receiver(fake, None, 1, true).unwrap();
        let device = UnsupportedDevice::read(&mut dev, 1).unwrap();
        let lighting = device.lighting.as_ref().expect("lighting read");
        assert_eq!(lighting.brightness, Some(Brightness { min: 0, max: 100, steps: 5, on_off: true }));
        let effects = lighting.effects.as_ref().unwrap();
        assert_eq!(effects.zones.len(), 1);
        assert_eq!(effects.zones[0].location, 1);
        let ids: Vec<(u8, u16, u16, u16)> =
            effects.zones[0].effects.iter().map(|e| (e.index, e.id, e.caps, e.period_ms)).collect();
        assert_eq!(ids, [(0, 1, 1, 0), (1, 3, 4, 10000)]);
        assert_eq!(effects.startup, Some(PowerEffect { enabled: true, colors: ["#ff0081".into(), "#80aaff".into()] }));
        // The shutdown effect is not there: the keyboard says "invalid argument" (as the G915 of issue #157 does to
        // both), which is kept in the replies but is not an error; the rest is read.
        assert_eq!(effects.shutdown, None);
        assert!(
            lighting.raw.iter().any(|r| r.starts_with("8071 3 (00 00 40): ") && r.contains("invalid argument")),
            "{:?}",
            lighting.raw
        );
        assert!(lighting.errors.is_empty(), "{:?}", lighting.errors);
        assert_eq!((effects.idle_s, effects.sleep_s), (Some(60), Some(300)));
        assert_eq!(lighting.keys, [4, 5, 6, 7, 9, 112]);
        assert_eq!(lighting.layout, Some(1));
        assert!(lighting.raw.iter().any(|r| r.starts_with("8071 0 (ff ff 00): ff ff 01")), "{:?}", lighting.raw);

        let now = LightingNow::read(&mut dev, lighting, true).unwrap();
        assert_eq!(now, LightingNow { brightness: Some(50), on: Some(true), control: Some(0) });

        assert!(lighting.raw.iter().any(|r| r == "8101 6 (00): 01 00 01"), "{:?}", lighting.raw);

        // Nothing but reads reached the keyboard: of 0x8101 (index 7) functions 0 and 6 with 0, and of
        // 0x8071 (index 4) functions 3, 5 and 7 only with a first byte 0, the "get".
        for request in written.lock().unwrap().iter() {
            let (index, function, first) = (request[2], request[3] >> 4, request[4]);
            let allowed = match (index, function) {
                (0..=2, _) => true,
                (3, 0 | 1 | 3) => true,
                (4, 0) => true,
                (4, 3 | 5 | 7) => first == 0,
                (5 | 6, 0) => true,
                (7, 0) => true,
                (7, 6) => first == 0,
                _ => false,
            };
            assert!(allowed, "feature index {index}, function {function}, first byte {first:#04x}");
        }
    }

    #[test]
    fn the_keys_come_from_every_page_of_the_bitmap() {
        let mut first = [0u8; 16];
        first[2] = 0b0000_0010; // key 1
        first[15] = 0b1000_0000; // key 111
        let mut second = [0u8; 16];
        second[2] = 0b0000_0001; // key 112
        assert_eq!(keys_from_pages(&[first, second]), [1, 111, 112]);
        // Bit 0 (key 0) and anything past 254 are not keys.
        assert!(keys_from_pages(&[[0xFF; 16]; 3]).iter().all(|&k| (1..=254).contains(&k)));
    }
}
