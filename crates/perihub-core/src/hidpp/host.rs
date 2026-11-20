//! Driving the lights of a Logitech keyboard in host mode, as Logitech G HUB does with its on-board
//! memory mode off: the keyboard hands its lights and their power modes to PeriHub, which sets the
//! effects and per-key colours in the keyboard's RAM, dims and turns the lights off when the keyboard
//! says nobody uses it, and gives everything back when it stops. Nothing is written to the keyboard's
//! memory: turning it off and on, or a deep sleep, brings its own lighting back.
//!
//! What each call and byte means is in docs/protocols/logitech-keyboard-lighting.md, with its sources
//! (Logitech's specifications of 0x8071, 0x8081 and 0x8040, and what other programs send).

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::lighting::{Lighting, ZoneEffect};
use super::{Event, HidppDevice, feature};
use crate::color::Rgb;
use crate::error::{Error, HidppErrorCode, Result};

/// The effects PeriHub sets, by the ids the keyboard lists (0x8071 function 0).
pub mod effect {
    pub const OFF: u16 = 0x00;
    pub const FIXED: u16 = 0x01;
    pub const BREATHING: u16 = 0x0A;
    pub const CYCLE: u16 = 0x15;
    pub const WAVE: u16 = 0x16;
    pub const RIPPLE: u16 = 0x17;
    pub const ALL: [u16; 6] = [OFF, FIXED, BREATHING, CYCLE, WAVE, RIPPLE];
}

/// 0x8101 function 6: host mode, and back to the keyboard's own profiles.
const HOST_MODE: u8 = 0x05;
const ONBOARD_MODE: u8 = 0x03;
/// 0x8071 function 5: the lights and their power modes go to PeriHub (0x03), and the keyboard tells
/// when nobody uses it and when it is used again (0x06). G HUB sends 0x05 (effect sync and the idle
/// notice) and, once idle, 0x03 (effect sync and the in-use notice); 0x06 asks for both notices at
/// once, and the G915 X LS of issue #30 dimmed, turned off and lit again with it.
const CLAIM: [u8; 3] = [0x01, 0x03, 0x06];
const RELEASE: [u8; 3] = [0x01, 0x00, 0x00];
/// 0x8071 function 8: lights on, and off.
const POWER_FULL: [u8; 3] = [0x01, 0x01, 0x00];
const POWER_OFF: [u8; 3] = [0x01, 0x03, 0x00];
/// 0x8071 function 1, last byte: the effect stays in RAM only.
const IN_RAM: u8 = 0x01;
/// The keyboard is busy (HID++ 2.0 error 8): asked again after these pauses.
const BUSY_RETRY_MS: [u64; 3] = [30, 60, 90];
const BUSY: u8 = 8;
/// How much of the colours is left while the keyboard is idle.
const DIMMED: f32 = 0.25;
/// Intensity (1 to 100) of the effects that have one, while the keyboard is idle; 0 is the default.
const DIMMED_INTENSITY: u8 = 25;
/// Seconds from dimming to off when the keyboard does not say (0x8071 function 7).
const DEFAULT_SLEEP_S: u16 = 270;
/// Wave directions after which a G915 X LS kept the wave and ignored every new effect of the zone
/// until its lights were given back and taken again (issue #30: "to the centre", 8). PeriHub does
/// that itself when such a wave changes.
const STICKY_DIRECTIONS: [u8; 1] = [8];
/// The pause between giving the lights back and taking them again, for the keyboard to load its own.
const RESET_PAUSE_MS: u64 = 150;
/// The key id of the logo in 0x8081 (Solaar, logi-ghub-linux and the G915 X LS's own key list).
const LOGO_KEY: u8 = 210;
/// How often PeriHub paints the logo's key while it plays the logo's breathing or colour cycle on it
/// (see [`Host::animate`]): one colour and one end of frame each time.
pub const ANIMATE_EVERY: Duration = Duration::from_millis(100);

/// What one lighting zone shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZoneLook {
    /// The zone's index, as the keyboard lists it.
    pub zone: u8,
    /// The effect's id, such as 1 for a fixed colour (see [`effect`]).
    pub effect: u16,
    #[serde(default)]
    pub color: Option<Rgb>,
    /// How long one round of the effect takes, in milliseconds.
    #[serde(default)]
    pub period_ms: Option<u16>,
    /// For a wave: 1 horizontal, 2 vertical, 3 centre out, 6 reverse horizontal, 7 reverse vertical,
    /// 8 centre in; 0 the keyboard's default.
    #[serde(default)]
    pub direction: Option<u8>,
}

/// What PeriHub shows on a keyboard it drives.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Look {
    pub zones: Vec<ZoneLook>,
    /// A colour per key, by the ids the keyboard lists (0x8081). The keyboard shows them only while
    /// its keys' zone has a fixed colour.
    #[serde(default)]
    pub keys: Vec<(u8, Rgb)>,
}

/// Whether the lights are on, dimmed because nobody uses the keyboard, or off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Power {
    Full,
    Dimmed,
    Off,
}

/// How host mode went, for the page and the test report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HostStatus {
    pub power: Power,
    /// Notices from the keyboard: nobody uses it, and it is used again.
    pub idle_events: u32,
    pub active_events: u32,
    /// Times the keyboard was found back on its own lighting, and taken again.
    pub retaken: u32,
    /// Looks shown since the lights were taken, and of them those that needed the lights given back
    /// and taken again (see [`STICKY_DIRECTIONS`]).
    pub shown: u32,
    pub resets: u32,
}

/// The direction a wave can take (1 to 8), by the 0x8071 capability bit that says it is there.
const DIRECTION_BITS: [u16; 8] = [0x0020, 0x0040, 0x0080, 0x0100, 0x0200, 0x0400, 0x0800, 0x1000];

/// The keyboard can be driven in host mode: it lists 0x8071 and 0x8101, and PeriHub read its zones.
pub fn can_drive(features: &[u16], lighting: Option<&Lighting>) -> bool {
    features.contains(&feature::RGB_EFFECTS)
        && features.contains(&feature::PROFILE_MANAGEMENT)
        && lighting.and_then(|l| l.effects.as_ref()).is_some_and(|e| !e.zones.is_empty())
}

/// The zone the keys are in (location 1).
fn keys_zone(lighting: &Lighting) -> Option<u8> {
    zone_at(lighting, 1)
}

/// The zone at `location` (1 the keys, 2 the logo).
fn zone_at(lighting: &Lighting, location: u16) -> Option<u8> {
    lighting.effects.as_ref()?.zones.iter().find(|z| z.location == location).map(|z| z.index)
}

/// The logo zone's look when the per-key frame decides what the logo shows: the keys show colours
/// per key, and the logo is one of the keys with a colour of their own (210 on the G915 X LS).
///
/// While a per-key frame is shown the logo shows the frame's colour for its key, whatever the logo
/// zone's effect, even an effect set after the frame, and a key left out of the frame keeps the
/// colour it had (issue #30: with colours per key the logo's breathing never showed and its colour
/// cycle stayed red, first with the logo in the frame, then with it left out and its effect sent
/// last; with the keys on a wave, and so no frame, both showed). So PeriHub gives the logo's key
/// the logo zone's look itself: its colour, black when it is off, and the breathing or colour cycle
/// played frame by frame (see [`Host::animate`]).
fn logo_in_frame<'a>(look: &'a Look, lighting: &Lighting) -> Option<&'a ZoneLook> {
    if look.keys.is_empty() || !lighting.keys.contains(&LOGO_KEY) {
        return None;
    }
    let logo = zone_at(lighting, 2)?;
    look.zones.iter().find(|z| z.zone == logo)
}

/// The logo zone plays an effect that changes over time.
fn animated(zone: &ZoneLook) -> bool {
    matches!(zone.effect, effect::BREATHING | effect::CYCLE)
}

/// The colour of the logo's key `elapsed` into the logo zone's effect, at full strength: what the
/// keyboard's own effect would show, played by PeriHub. Breathing rises and falls as a cosine from
/// the colour to dark and back; the colour cycle goes once round the colour wheel each period.
pub fn logo_color(zone: &ZoneLook, found: &ZoneEffect, elapsed: Duration) -> Rgb {
    let color = zone.color.unwrap_or(Rgb::WHITE);
    let turn = |default: u16, range: (u16, u16), unit: bool| {
        let ms = period(zone, found, default, range, unit).map(u16::from_be_bytes).unwrap_or(default).max(1);
        (elapsed.as_millis() % ms as u128) as f32 / ms as f32
    };
    match zone.effect {
        effect::FIXED => color,
        effect::BREATHING => {
            let t = turn(5000, (1000, 20000), false);
            color.scale((1.0 + (t * std::f32::consts::TAU).cos()) / 2.0)
        }
        effect::CYCLE => Rgb::from_hsv(turn(5000, (1000, 30000), true), 1.0, 1.0),
        _ => Rgb::BLACK,
    }
}

/// The per-key frame for `look`, with `logo` as the colour of the logo's key when the frame decides
/// what the logo shows (see [`logo_in_frame`]). Without a logo zone the logo is a key like the others.
fn frame_keys(look: &Look, lighting: &Lighting, logo: Option<Rgb>) -> Vec<(u8, Rgb)> {
    if zone_at(lighting, 2).is_none() {
        return look.keys.clone();
    }
    let mut keys: Vec<(u8, Rgb)> = look.keys.iter().filter(|(k, _)| *k != LOGO_KEY).copied().collect();
    if let Some(color) = logo.filter(|_| !keys.is_empty()) {
        keys.push((LOGO_KEY, color));
    }
    keys
}

fn find_effect(lighting: &Lighting, zone: u8, id: u16) -> Result<&ZoneEffect> {
    let zones = lighting.effects.as_ref().map(|e| e.zones.as_slice()).unwrap_or_default();
    let zone_info = zones
        .iter()
        .find(|z| z.index == zone)
        .ok_or_else(|| Error::Invalid(format!("the keyboard has no lighting zone {zone}")))?;
    if !effect::ALL.contains(&id) {
        return Err(Error::Invalid(format!("PeriHub does not set effect {id:#04x} yet")));
    }
    zone_info
        .effects
        .iter()
        .find(|e| e.id == id)
        .ok_or_else(|| Error::Invalid(format!("zone {zone} has no effect {id:#04x}")))
}

/// Checks `look` against what the keyboard reported, before anything is sent.
pub fn check(look: &Look, lighting: &Lighting) -> Result<()> {
    for zone in &look.zones {
        let found = find_effect(lighting, zone.zone, zone.effect)?;
        effect_params(found, zone, 0)?;
    }
    if !look.keys.is_empty() {
        if let Some((key, _)) = look.keys.iter().find(|(key, _)| !lighting.keys.contains(key)) {
            return Err(Error::Invalid(format!("the keyboard has no key {key} with its own colour")));
        }
        let fixed = keys_zone(lighting)
            .and_then(|index| look.zones.iter().find(|z| z.zone == index))
            .is_some_and(|z| z.effect == effect::FIXED);
        if !fixed {
            return Err(Error::Invalid("colours per key need a fixed colour on the keys".into()));
        }
    }
    Ok(())
}

/// A period in milliseconds within `range`, rounded to a multiple of the keyboard's own unit when
/// it has one (the specification asks that for colour cycles and waves).
fn period(look: &ZoneLook, found: &ZoneEffect, default: u16, range: (u16, u16), unit: bool) -> Result<[u8; 2]> {
    let mut ms = look.period_ms.unwrap_or(default);
    if !(range.0..=range.1).contains(&ms) {
        return Err(Error::Invalid(format!("the period must be {} to {} ms", range.0, range.1)));
    }
    if unit && found.period_ms > 1 {
        let step = found.period_ms;
        ms = (((ms as u32 + step as u32 / 2) / step as u32).max(1) * step as u32).min(u16::MAX as u32) as u16;
    }
    Ok(ms.to_be_bytes())
}

/// The ten effect bytes of 0x8071 function 1 for `look`, with `intensity` (1 to 100, 0 the default)
/// where the effect has one.
pub fn effect_params(found: &ZoneEffect, look: &ZoneLook, intensity: u8) -> Result<[u8; 10]> {
    let [r, g, b] = look.color.unwrap_or(Rgb::WHITE).to_bytes();
    let mut p = [0u8; 10];
    match found.id {
        effect::OFF => {}
        // Byte 3: 2 shows the colour at once, without a ramp.
        effect::FIXED => p[..4].copy_from_slice(&[r, g, b, 0x02]),
        effect::BREATHING => {
            let [high, low] = period(look, found, 5000, (1000, 20000), false)?;
            // Byte 5, the waveform: 0 the keyboard's default.
            p[..7].copy_from_slice(&[r, g, b, high, low, 0, intensity]);
        }
        effect::CYCLE | effect::WAVE => {
            let [high, low] = period(look, found, 5000, (1000, 30000), true)?;
            // Byte 1, the saturation: full colours.
            p[1] = 0xFF;
            p[6..9].copy_from_slice(&[high, low, intensity]);
            if found.id == effect::WAVE {
                let direction = look.direction.unwrap_or(0);
                if direction != 0 {
                    let bit = DIRECTION_BITS.get(direction as usize - 1).copied().unwrap_or(0);
                    if bit == 0 || found.caps & bit == 0 {
                        return Err(Error::Invalid(format!("the wave cannot go in direction {direction}")));
                    }
                }
                p[9] = direction;
            }
        }
        effect::RIPPLE => {
            let [high, low] = period(look, found, found.period_ms.clamp(20, 500), (20, 500), false)?;
            p[..4].copy_from_slice(&[r, g, b, 0xFF]);
            p[6..8].copy_from_slice(&[high, low]);
        }
        id => return Err(Error::Invalid(format!("PeriHub does not set effect {id:#04x} yet"))),
    }
    Ok(p)
}

/// The 0x8081 function 6 parameters that give `keys` their colours: one colour and up to 13 keys each.
pub fn key_reports(keys: &[(u8, Rgb)]) -> Vec<Vec<u8>> {
    let mut by_color: BTreeMap<[u8; 3], Vec<u8>> = BTreeMap::new();
    for (key, color) in keys {
        let ids = by_color.entry(color.to_bytes()).or_default();
        if !ids.contains(key) {
            ids.push(*key);
        }
    }
    by_color.iter().flat_map(|(color, ids)| ids.chunks(13).map(move |chunk| [&color[..], chunk].concat())).collect()
}

/// One call that changes something, asked again while the keyboard says it is busy.
fn send(dev: &mut HidppDevice, feature: u16, function: u8, params: &[u8]) -> Result<()> {
    let mut pauses = BUSY_RETRY_MS.iter();
    loop {
        match dev.call(feature, function, params) {
            Err(Error::Hidpp(HidppErrorCode(BUSY))) => match pauses.next() {
                Some(&ms) => std::thread::sleep(Duration::from_millis(ms)),
                None => return Err(Error::Hidpp(HidppErrorCode(BUSY))),
            },
            other => return other.map(|_| ()),
        }
    }
}

/// Sets the brightness of all the lights (0x8040 function 2). The keyboard keeps it, as it keeps
/// what its brightness key sets.
pub fn set_brightness(dev: &mut HidppDevice, lighting: &Lighting, value: u16) -> Result<()> {
    let brightness = lighting.brightness.as_ref().ok_or(Error::FeatureMissing(feature::BRIGHTNESS_CONTROL))?;
    if !(brightness.min..=brightness.max).contains(&value) {
        return Err(Error::Invalid(format!("the brightness must be {} to {}", brightness.min, brightness.max)));
    }
    send(dev, feature::BRIGHTNESS_CONTROL, 2, &value.to_be_bytes())
}

/// Who drives the lights now (0x8071 function 5, read): 0 the keyboard itself, 3 a program.
pub fn who_drives(dev: &mut HidppDevice) -> Result<u8> {
    Ok(dev.call(feature::RGB_EFFECTS, 5, &[0])?[1])
}

/// Gives the lights back to the keyboard, then its own profiles: both are tried.
pub fn give_back(dev: &mut HidppDevice) -> Result<()> {
    let lights = send(dev, feature::RGB_EFFECTS, 5, &RELEASE);
    let mode = send(dev, feature::PROFILE_MANAGEMENT, 6, &[ONBOARD_MODE]);
    lights.and(mode)
}

/// Lights on (0x8071 function 8 `01 01 00`): the first call after the keyboard woke can fail, so
/// it is tried three times, 100 ms apart, as Solaar does.
fn lights_on(dev: &mut HidppDevice) -> Result<()> {
    let mut last = Ok(());
    for attempt in 0..3 {
        if attempt > 0 {
            std::thread::sleep(Duration::from_millis(100));
        }
        last = send(dev, feature::RGB_EFFECTS, 8, &POWER_FULL);
        if last.is_ok() {
            break;
        }
    }
    last
}

/// A keyboard PeriHub drives.
pub struct Host {
    pub look: Look,
    status: HostStatus,
    idle_since: Option<Instant>,
    sleep_after: Option<Duration>,
    /// A paint failed half-way (the keyboard did not answer): done again at the next tick.
    needs_paint: bool,
    /// When the look was shown: the clock of the logo's breathing or colour cycle.
    shown_at: Instant,
    /// The colour the logo's key got last, so that an unchanged one is not sent again.
    logo_sent: Option<Rgb>,
}

impl Host {
    /// Takes the lights over and shows `look`. If anything fails the keyboard gets them back.
    pub fn take(dev: &mut HidppDevice, lighting: &Lighting, look: Look) -> Result<Self> {
        check(&look, lighting)?;
        let sleep_s = lighting.effects.as_ref().and_then(|e| e.sleep_s).unwrap_or(DEFAULT_SLEEP_S);
        let mut host = Self {
            look,
            status: HostStatus {
                power: Power::Full,
                idle_events: 0,
                active_events: 0,
                retaken: 0,
                shown: 0,
                resets: 0,
            },
            idle_since: None,
            needs_paint: false,
            shown_at: Instant::now(),
            logo_sent: None,
            // 0xFFFF: never.
            sleep_after: (sleep_s != 0xFFFF).then(|| Duration::from_secs(sleep_s as u64)),
        };
        // Notices from before are not about this session.
        dev.events(0)?;
        match host.claim(dev, lighting) {
            Ok(()) => Ok(host),
            Err(error) => {
                if let Err(e) = give_back(dev) {
                    log::debug!("giving the lights back after a failed take-over: {e}");
                }
                Err(error)
            }
        }
    }

    fn claim(&mut self, dev: &mut HidppDevice, lighting: &Lighting) -> Result<()> {
        send(dev, feature::PROFILE_MANAGEMENT, 6, &[HOST_MODE])?;
        send(dev, feature::RGB_EFFECTS, 5, &CLAIM)?;
        // Lights on: with its own lights the keyboard dims them and turns them off by itself when
        // nobody types (and the user clicks with the mouse meanwhile). Once the power modes are
        // PeriHub's, it stays dimmed or off until told otherwise: the keys stayed dark at the
        // first take-over in issue #30.
        lights_on(dev)?;
        // The first frame after the take-over can come before the keyboard has left its own mode:
        // it is sent twice.
        self.paint(dev, lighting, true)
    }

    /// Shows the look (dimmed while the keyboard is idle): the keys' zone, then the per-key frame
    /// (`twice`: sent again), then the other zones, so that the logo's own effect comes last.
    fn paint(&mut self, dev: &mut HidppDevice, lighting: &Lighting, twice: bool) -> Result<()> {
        self.needs_paint = true;
        let dimmed = self.status.power == Power::Dimmed;
        let (factor, intensity) = if dimmed { (DIMMED, DIMMED_INTENSITY) } else { (1.0, 0) };
        let keys_zone = keys_zone(lighting);
        let (first, rest): (Vec<&ZoneLook>, Vec<&ZoneLook>) =
            self.look.zones.iter().partition(|z| Some(z.zone) == keys_zone);
        let set_zone = |dev: &mut HidppDevice, zone: &ZoneLook| -> Result<()> {
            let found = find_effect(lighting, zone.zone, zone.effect)?;
            let mut shown = zone.clone();
            shown.color = zone.color.map(|c| c.scale(factor));
            let params = effect_params(found, &shown, intensity)?;
            let request = [&[zone.zone, found.index][..], &params, &[IN_RAM]].concat();
            send(dev, feature::RGB_EFFECTS, 1, &request)
        };
        for zone in first {
            set_zone(dev, zone)?;
        }
        if !self.look.keys.is_empty() {
            let logo = self.logo_now(lighting);
            let keys: Vec<(u8, Rgb)> =
                frame_keys(&self.look, lighting, logo).iter().map(|&(k, c)| (k, c.scale(factor))).collect();
            self.logo_sent = None;
            for _ in 0..if twice { 2 } else { 1 } {
                // A frame that failed half-way is not shown: no end of frame.
                for params in key_reports(&keys) {
                    send(dev, feature::PER_KEY_LIGHTING_V2, 6, &params)?;
                }
                send(dev, feature::PER_KEY_LIGHTING_V2, 7, &[0, 0, 0, 0, 0])?;
            }
            self.logo_sent = logo.map(|c| c.scale(factor));
        }
        for zone in rest {
            set_zone(dev, zone)?;
        }
        self.needs_paint = false;
        Ok(())
    }

    /// Shows a new look. Leaving a wave the keyboard sticks to goes through giving the lights back
    /// and taking them again (see [`STICKY_DIRECTIONS`]).
    pub fn show(&mut self, dev: &mut HidppDevice, lighting: &Lighting, look: Look) -> Result<()> {
        check(&look, lighting)?;
        let sticky = self.look.zones.iter().any(|old| {
            old.effect == effect::WAVE
                && old.direction.is_some_and(|d| STICKY_DIRECTIONS.contains(&d))
                && look.zones.iter().find(|z| z.zone == old.zone) != Some(old)
        });
        self.look = look;
        self.status.shown += 1;
        self.shown_at = Instant::now();
        if sticky {
            self.status.resets += 1;
            give_back(dev)?;
            std::thread::sleep(Duration::from_millis(RESET_PAUSE_MS));
            self.status.power = Power::Full;
            self.idle_since = None;
            return self.claim(dev, lighting);
        }
        if self.status.power == Power::Off {
            self.wake(dev)?;
        }
        self.paint(dev, lighting, false)
    }

    /// The colour of the logo's key now, at full strength, when the per-key frame decides what the
    /// logo shows (see [`logo_in_frame`]).
    fn logo_now(&self, lighting: &Lighting) -> Option<Rgb> {
        let zone = logo_in_frame(&self.look, lighting)?;
        let found = find_effect(lighting, zone.zone, zone.effect).ok()?;
        Some(logo_color(zone, found, self.shown_at.elapsed()))
    }

    /// PeriHub plays the logo's breathing or colour cycle on the logo's key: the keys show colours
    /// per key, which hide the logo zone's own effect (see [`logo_in_frame`]). The device's thread
    /// then calls [`Host::animate`] every [`ANIMATE_EVERY`].
    pub fn animates(&self, lighting: &Lighting) -> bool {
        logo_in_frame(&self.look, lighting).is_some_and(animated)
    }

    /// One step of the logo's breathing or colour cycle: the logo's key gets its colour of the moment
    /// (0x8081 function 6, then the end of the frame, in RAM), when it changed. Nothing while the
    /// lights are off or a paint is still to be done again.
    pub fn animate(&mut self, dev: &mut HidppDevice, lighting: &Lighting) -> Result<()> {
        if !self.animates(lighting) || self.needs_paint || self.status.power == Power::Off {
            return Ok(());
        }
        let factor = if self.status.power == Power::Dimmed { DIMMED } else { 1.0 };
        let Some(color) = self.logo_now(lighting).map(|c| c.scale(factor)) else { return Ok(()) };
        if self.logo_sent == Some(color) {
            return Ok(());
        }
        let [r, g, b] = color.to_bytes();
        self.logo_sent = None;
        send(dev, feature::PER_KEY_LIGHTING_V2, 6, &[r, g, b, LOGO_KEY])?;
        send(dev, feature::PER_KEY_LIGHTING_V2, 7, &[0, 0, 0, 0, 0])?;
        self.logo_sent = Some(color);
        Ok(())
    }

    /// The last paint failed half-way: the next tick paints again (not while the lights are off).
    pub fn paint_later(&mut self) {
        self.needs_paint = true;
    }

    /// Paints again if the last paint failed half-way.
    pub fn catch_up(&mut self, dev: &mut HidppDevice, lighting: &Lighting) -> Result<()> {
        if self.needs_paint && self.status.power != Power::Off {
            self.paint(dev, lighting, false)?;
        }
        Ok(())
    }

    pub fn status(&self) -> HostStatus {
        self.status.clone()
    }

    /// Lights on again after they were turned off.
    fn wake(&mut self, dev: &mut HidppDevice) -> Result<()> {
        lights_on(dev)?;
        send(dev, feature::RGB_EFFECTS, 5, &CLAIM)?;
        self.status.power = Power::Full;
        self.idle_since = None;
        Ok(())
    }

    /// Acts on what the keyboard told: nobody uses it (dim), it is used again (full lights). The
    /// keyboard sends each notice several times: only a change counts.
    pub fn on_events(&mut self, dev: &mut HidppDevice, lighting: &Lighting, events: &[Event]) -> Result<()> {
        for event in events.iter().filter(|e| e.feature == feature::RGB_EFFECTS && e.event == 1) {
            let idle = event.data[0] == 0;
            match (idle, self.status.power) {
                (true, Power::Full) => {
                    self.status.idle_events += 1;
                    self.status.power = Power::Dimmed;
                    self.idle_since = Some(Instant::now());
                    self.paint(dev, lighting, false)?;
                }
                (false, Power::Dimmed | Power::Off) => {
                    self.status.active_events += 1;
                    if self.status.power == Power::Off {
                        self.wake(dev)?;
                    }
                    self.status.power = Power::Full;
                    self.idle_since = None;
                    self.paint(dev, lighting, false)?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Turns the lights off once the keyboard has been idle long enough (the keyboard does not say
    /// so itself while PeriHub drives it).
    pub fn on_timer(&mut self, dev: &mut HidppDevice) -> Result<()> {
        if self.status.power == Power::Dimmed
            && let (Some(since), Some(after)) = (self.idle_since, self.sleep_after)
            && since.elapsed() >= after
        {
            send(dev, feature::RGB_EFFECTS, 8, &POWER_OFF)?;
            self.status.power = Power::Off;
        }
        Ok(())
    }

    /// The keyboard was found on its own lighting (it restarted, or another program gave it back):
    /// takes it again and shows the look.
    pub fn retake(&mut self, dev: &mut HidppDevice, lighting: &Lighting) -> Result<()> {
        self.status.retaken += 1;
        self.status.power = Power::Full;
        self.idle_since = None;
        self.claim(dev, lighting)
    }

    #[cfg(test)]
    pub(crate) fn idle_for(&mut self, elapsed: Duration) {
        self.idle_since = Some(Instant::now() - elapsed);
    }

    #[cfg(test)]
    pub(crate) fn shown_for(&mut self, elapsed: Duration) {
        self.shown_at = Instant::now() - elapsed;
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::hidpp::lighting::{Brightness, EffectZone, RgbEffects};
    use crate::hidpp::tests::{FakeDevice, reply};
    use crate::hidpp::{Access, LONG_LEN};

    const FEATURES: &[u16] = &[
        feature::ROOT,
        feature::BRIGHTNESS_CONTROL,
        feature::RGB_EFFECTS,
        feature::PER_KEY_LIGHTING_V2,
        feature::PROFILE_MANAGEMENT,
    ];

    /// What the simulated keyboard got, and how it answers.
    #[derive(Default)]
    pub(crate) struct Keyboard {
        /// (feature, function, parameters without trailing zeros).
        pub(crate) calls: Vec<(u16, u8, Vec<u8>)>,
        /// 0x8071 function 5: who drives the lights.
        pub(crate) control: u8,
        pub(crate) host_mode: bool,
        /// The next calls of 0x8081 function 6 answer "busy".
        pub(crate) busy: u32,
        /// 0x8071 function 8: the power mode set last (1 full, 3 off; 0 never set).
        pub(crate) power: u8,
        /// The receiver says the keyboard is not there (asleep, or out of reach).
        pub(crate) asleep: bool,
    }

    fn error(request: &[u8], code: u8) -> Vec<u8> {
        let mut report = vec![0x11, request[1], 0xFF, request[2], request[3], code];
        report.resize(LONG_LEN, 0);
        report
    }

    pub(crate) type Pending = Arc<Mutex<std::collections::VecDeque<Vec<u8>>>>;

    /// A G915 X LS in slot 1 (features 0x8040, 0x8071, 0x8081 and 0x8101), read-only until granted.
    pub(crate) fn keyboard_read_only() -> (HidppDevice, Arc<Mutex<Keyboard>>, Pending) {
        let state = Arc::new(Mutex::new(Keyboard::default()));
        let kb = state.clone();
        let fake = FakeDevice::new(move |request| {
            let (index, function, p) = (request[2] as usize, request[3] >> 4, &request[4..]);
            let mut k = kb.lock().unwrap();
            let id = FEATURES.get(index).copied();
            if id == Some(feature::ROOT) {
                return match function {
                    1 => vec![reply(request, &[4, 2, request[6]])],
                    _ => {
                        let id = u16::from_be_bytes([p[0], p[1]]);
                        vec![reply(request, &[FEATURES.iter().position(|&f| f == id).unwrap_or(0) as u8])]
                    }
                };
            }
            let id = id.unwrap();
            if k.asleep {
                return vec![crate::hidpp::tests::receiver_error(request, 0x09)];
            }
            let used = p.iter().rposition(|&b| b != 0).map_or(0, |i| i + 1);
            k.calls.push((id, function, p[..used].to_vec()));
            let payload: Vec<u8> = match (id, function) {
                (feature::RGB_EFFECTS, 5) if p[0] == 0 => vec![0, k.control, 0],
                (feature::RGB_EFFECTS, 5) => {
                    k.control = p[1];
                    p[..3].to_vec()
                }
                // Effects and power modes only once a program has the lights, as the specification says.
                (feature::RGB_EFFECTS, 1 | 8) if k.control != 3 => return vec![error(request, 5)],
                (feature::RGB_EFFECTS, 8) => {
                    k.power = p[1];
                    p[..2].to_vec()
                }
                (feature::PROFILE_MANAGEMENT, 6) => {
                    if p[0] == 5 {
                        k.host_mode = true;
                    } else if p[0] == 3 {
                        k.host_mode = false;
                    }
                    vec![if k.host_mode { 0 } else { 1 }, 0, 1]
                }
                (feature::PER_KEY_LIGHTING_V2, 6) if k.busy > 0 => {
                    k.busy -= 1;
                    return vec![error(request, 8)];
                }
                _ => vec![],
            };
            vec![reply(request, &payload)]
        });
        let pending = fake.pending.clone();
        let dev = HidppDevice::behind_receiver(fake, None, 1, true).unwrap();
        (dev, state, pending)
    }

    fn keyboard() -> (HidppDevice, Arc<Mutex<Keyboard>>, Pending) {
        let (mut dev, state, pending) = keyboard_read_only();
        dev.grant(Access::Host);
        (dev, state, pending)
    }

    fn effect(index: u8, id: u16, caps: u16, period_ms: u16) -> ZoneEffect {
        ZoneEffect { index, id, caps, period_ms }
    }

    /// The lighting a G915 X LS reported in issue #30.
    pub(crate) fn g915x() -> Lighting {
        let keys_zone = EffectZone {
            index: 0,
            location: 1,
            effects: vec![
                effect(0, 0x00, 0, 0),
                effect(1, 0x01, 0x0001, 0),
                effect(2, 0x0A, 0xC001, 7691),
                effect(3, 0x15, 0xC005, 992),
                effect(4, 0x16, 0xDCE1, 30),
                effect(5, 0x17, 0, 50),
            ],
        };
        let logo = EffectZone {
            index: 1,
            location: 2,
            effects: vec![
                effect(0, 0x00, 0, 0),
                effect(1, 0x01, 0x0001, 0),
                effect(2, 0x0A, 0xC001, 7691),
                effect(3, 0x15, 0xC005, 992),
            ],
        };
        let mut keys: Vec<u8> = (1..=111).collect();
        keys.extend([153, 155, 156, 157, 158]);
        keys.extend(180..=188);
        keys.push(210);
        Lighting {
            brightness: Some(Brightness { min: 0, max: 100, steps: 5, on_off: false }),
            effects: Some(RgbEffects {
                zones: vec![keys_zone, logo],
                startup: None,
                shutdown: None,
                idle_s: Some(60),
                sleep_s: Some(270),
            }),
            keys,
            ..Lighting::default()
        }
    }

    pub(crate) fn zone(zone: u8, effect: u16, color: Option<Rgb>) -> ZoneLook {
        ZoneLook { zone, effect, color, period_ms: None, direction: None }
    }

    pub(crate) const RED: Rgb = Rgb::new(0xFF, 0, 0);
    const BLUE: Rgb = Rgb::new(0, 0, 0xFF);

    /// Red keys with a blue Escape (key 38), and a breathing blue logo.
    pub(crate) fn look() -> Look {
        let mut keys: Vec<(u8, Rgb)> = g915x().keys.iter().map(|&k| (k, RED)).collect();
        keys.retain(|(k, _)| *k != 38);
        keys.push((38, BLUE));
        Look { zones: vec![zone(0, effect::FIXED, Some(RED)), zone(1, effect::BREATHING, Some(BLUE))], keys }
    }

    pub(crate) fn calls(state: &Arc<Mutex<Keyboard>>) -> Vec<(u16, u8, Vec<u8>)> {
        std::mem::take(&mut state.lock().unwrap().calls)
    }

    pub(crate) fn idle_event(active: bool) -> Vec<u8> {
        let mut report = vec![0u8; LONG_LEN];
        report[..5].copy_from_slice(&[0x11, 1, 2, 0x10, active as u8]);
        report
    }

    #[test]
    fn taking_the_lights_goes_into_host_mode_and_shows_the_look_in_ram() {
        let (mut dev, state, _) = keyboard();
        let lighting = g915x();
        let host = Host::take(&mut dev, &lighting, look()).unwrap();
        assert_eq!(host.status().power, Power::Full);
        let sent = calls(&state);
        let short: Vec<(u16, u8)> = sent.iter().map(|(f, n, _)| (*f, *n)).collect();
        let frame = [(feature::PER_KEY_LIGHTING_V2, 6), (feature::PER_KEY_LIGHTING_V2, 6)];
        let frame = frame.iter().chain(std::iter::repeat_n(&(feature::PER_KEY_LIGHTING_V2, 6), 9));
        let mut expected = vec![
            (feature::PROFILE_MANAGEMENT, 6),
            (feature::RGB_EFFECTS, 5),
            (feature::RGB_EFFECTS, 8),
            (feature::RGB_EFFECTS, 1),
        ];
        // 124 red keys in 10 reports, Escape and the logo (blue, at the start of its breath) in 1,
        // then the end of the frame: twice. Then the logo's own effect.
        for _ in 0..2 {
            expected.extend(frame.clone().take(11).copied());
            expected.push((feature::PER_KEY_LIGHTING_V2, 7));
        }
        expected.push((feature::RGB_EFFECTS, 1));
        assert_eq!(short, expected);
        assert_eq!(sent[0].2, [5]);
        assert_eq!(sent[1].2, [1, 3, 6]);
        // Lights on.
        assert_eq!(sent[2].2, [1, 1]);
        // Zone 0, effect index 1 (fixed), red, at once, in RAM.
        assert_eq!(sent[3].2, [0, 1, 0xFF, 0, 0, 2, 0, 0, 0, 0, 0, 0, 1]);
        // Zone 1, effect index 2 (breathing), blue, 5 s, default waveform and intensity, in RAM.
        assert_eq!(sent[sent.len() - 1].2, [1, 2, 0, 0, 0xFF, 0x13, 0x88, 0, 0, 0, 0, 0, 1]);
        // The end of the frame: in RAM, frame 0.
        assert!(sent.iter().filter(|c| c.1 == 7 && c.0 == feature::PER_KEY_LIGHTING_V2).all(|c| c.2.is_empty()));
        let k = state.lock().unwrap();
        assert!(k.host_mode && k.control == 3);
    }

    #[test]
    fn a_keyboard_that_dimmed_or_turned_off_its_lights_on_its_own_is_lit_when_taken() {
        let (mut dev, state, _) = keyboard();
        // Its own power save turned the lights off while the user clicked with the mouse.
        state.lock().unwrap().power = 3;
        Host::take(&mut dev, &g915x(), look()).unwrap();
        let sent = calls(&state);
        let on = sent.iter().position(|c| *c == (feature::RGB_EFFECTS, 8, vec![1, 1])).expect("lights on");
        let first_effect = sent.iter().position(|c| c.0 == feature::RGB_EFFECTS && c.1 == 1).unwrap();
        assert!(on < first_effect, "before the look");
        assert_eq!(state.lock().unwrap().power, 1);
    }

    /// The per-key reports of `sent`, by colour.
    fn keys_sent(sent: &[(u16, u8, Vec<u8>)]) -> Vec<(u8, [u8; 3])> {
        sent.iter()
            .filter(|c| c.0 == feature::PER_KEY_LIGHTING_V2 && c.1 == 6)
            .flat_map(|c| c.2[3..].iter().map(|&k| (k, [c.2[0], c.2[1], c.2[2]])).collect::<Vec<_>>())
            .collect()
    }

    /// The logo's key in the per-key reports of `sent`.
    fn logo_sent(sent: &[(u16, u8, Vec<u8>)]) -> Vec<[u8; 3]> {
        keys_sent(sent).into_iter().filter(|(k, _)| *k == LOGO_KEY).map(|(_, c)| c).collect()
    }

    #[test]
    fn with_colours_per_key_the_logo_s_key_gets_the_logo_s_look() {
        let (mut dev, state, _) = keyboard();
        let lighting = g915x();
        // Breathing blue logo: its key starts at full blue in both frames, and the logo zone's own
        // effect still comes last.
        let mut host = Host::take(&mut dev, &lighting, look()).unwrap();
        let sent = calls(&state);
        assert_eq!(logo_sent(&sent), [[0, 0, 0xFF]; 2]);
        assert_eq!(keys_sent(&sent).len(), 2 * 126);
        assert_eq!(sent.last().unwrap().2[..2], [1, 2], "the logo's breathing last");
        assert!(host.animates(&lighting));

        // A fixed green logo: green, and nothing to play.
        let green = Rgb::new(0, 0xFF, 0);
        let mut fixed = look();
        fixed.zones[1] = zone(1, effect::FIXED, Some(green));
        host.show(&mut dev, &lighting, fixed).unwrap();
        assert_eq!(logo_sent(&calls(&state)), [[0, 0xFF, 0]]);
        assert!(!host.animates(&lighting));
        host.animate(&mut dev, &lighting).unwrap();
        assert!(calls(&state).is_empty());

        // The logo off: its key is dark, instead of keeping the colour it had.
        let mut off = look();
        off.zones[1] = zone(1, effect::OFF, None);
        host.show(&mut dev, &lighting, off).unwrap();
        assert_eq!(logo_sent(&calls(&state)), [[0, 0, 0]]);

        // The keys on a wave: no frame, the logo zone's own effect shows.
        let wave =
            Look { zones: vec![zone(0, effect::WAVE, None), zone(1, effect::BREATHING, Some(BLUE))], keys: vec![] };
        host.show(&mut dev, &lighting, wave).unwrap();
        assert!(logo_sent(&calls(&state)).is_empty());
        assert!(!host.animates(&lighting));
    }

    #[test]
    fn the_logo_s_breathing_and_colour_cycle_are_played_on_its_key() {
        let (mut dev, state, _) = keyboard();
        let lighting = g915x();
        let mut host = Host::take(&mut dev, &lighting, look()).unwrap();
        calls(&state);
        // Unchanged since the frame: nothing sent.
        host.animate(&mut dev, &lighting).unwrap();
        assert!(calls(&state).is_empty());
        // Half-way through the 5 s breath: dark, one colour and one end of frame, in RAM.
        host.shown_for(Duration::from_millis(2500));
        host.animate(&mut dev, &lighting).unwrap();
        assert_eq!(
            calls(&state),
            [(feature::PER_KEY_LIGHTING_V2, 6, vec![0, 0, 0, LOGO_KEY]), (feature::PER_KEY_LIGHTING_V2, 7, vec![])]
        );
        host.animate(&mut dev, &lighting).unwrap();
        assert!(calls(&state).is_empty(), "the same colour is not sent again");

        // Idle: a quarter of the colour. Off: nothing.
        host.on_events(&mut dev, &lighting, &[Event { feature: feature::RGB_EFFECTS, event: 1, data: [0; 16] }])
            .unwrap();
        calls(&state);
        host.shown_for(Duration::from_millis(5000));
        host.animate(&mut dev, &lighting).unwrap();
        assert_eq!(calls(&state)[0].2, [0, 0, 0x40, LOGO_KEY]);
        host.idle_for(Duration::from_secs(270));
        host.on_timer(&mut dev).unwrap();
        calls(&state);
        host.shown_for(Duration::from_millis(2500));
        host.animate(&mut dev, &lighting).unwrap();
        assert!(calls(&state).is_empty());

        // The colour cycle goes round the wheel: red, then green a third of the way (5 s → 4960 ms,
        // a multiple of the keyboard's 992 ms).
        let found = &lighting.effects.as_ref().unwrap().zones[1].effects[3];
        let cycle = zone(1, effect::CYCLE, None);
        assert_eq!(logo_color(&cycle, found, Duration::ZERO), Rgb::new(0xFF, 0, 0));
        assert_eq!(logo_color(&cycle, found, Duration::from_millis(4960 / 3)), Rgb::new(0, 0xFF, 0));
        let breathing = &lighting.effects.as_ref().unwrap().zones[1].effects[2];
        let slow = ZoneLook { period_ms: Some(8000), ..zone(1, effect::BREATHING, Some(BLUE)) };
        // A quarter of an 8 s breath: half the colour.
        let quarter = logo_color(&slow, breathing, Duration::from_millis(2000)).to_bytes();
        assert!(quarter[..2] == [0, 0] && (0x7F..=0x80).contains(&quarter[2]), "{quarter:02x?}");
    }

    #[test]
    fn leaving_a_wave_to_the_centre_gives_the_lights_back_and_takes_them_again() {
        let (mut dev, state, _) = keyboard();
        let lighting = g915x();
        let wave = |direction| Look {
            zones: vec![ZoneLook { direction: Some(direction), ..zone(0, effect::WAVE, None) }],
            keys: Vec::new(),
        };
        let cycle = Look { zones: vec![zone(0, effect::CYCLE, None)], keys: Vec::new() };
        let mut host = Host::take(&mut dev, &lighting, wave(1)).unwrap();
        host.show(&mut dev, &lighting, wave(8)).unwrap();
        calls(&state);
        // From the centre-in wave to a colour cycle: given back, then taken again.
        host.show(&mut dev, &lighting, cycle.clone()).unwrap();
        let short: Vec<(u16, u8, u8)> = calls(&state).iter().map(|(f, n, p)| (*f, *n, p[0])).collect();
        assert_eq!(
            short,
            [
                (feature::RGB_EFFECTS, 5, 1),
                (feature::PROFILE_MANAGEMENT, 6, 3),
                (feature::PROFILE_MANAGEMENT, 6, 5),
                (feature::RGB_EFFECTS, 5, 1),
                (feature::RGB_EFFECTS, 8, 1),
                (feature::RGB_EFFECTS, 1, 0),
            ]
        );
        assert_eq!((host.status().shown, host.status().resets), (2, 1));
        // Any other change only sends the effect.
        host.show(&mut dev, &lighting, wave(1)).unwrap();
        host.show(&mut dev, &lighting, cycle).unwrap();
        assert!(calls(&state).iter().all(|c| c.0 == feature::RGB_EFFECTS && c.1 == 1));
        assert_eq!(host.status().resets, 1);
        let k = state.lock().unwrap();
        assert!(k.host_mode && k.control == 3);
    }

    #[test]
    fn a_paint_that_failed_half_way_is_done_again() {
        let (mut dev, state, _) = keyboard();
        let lighting = g915x();
        let mut host = Host::take(&mut dev, &lighting, look()).unwrap();
        calls(&state);
        host.catch_up(&mut dev, &lighting).unwrap();
        assert!(calls(&state).is_empty(), "nothing to catch up");
        state.lock().unwrap().busy = 4;
        assert!(host.show(&mut dev, &lighting, look()).is_err());
        calls(&state);
        host.catch_up(&mut dev, &lighting).unwrap();
        let sent = calls(&state);
        assert_eq!(sent.iter().filter(|c| c.0 == feature::PER_KEY_LIGHTING_V2 && c.1 == 7).count(), 1);
        host.catch_up(&mut dev, &lighting).unwrap();
        assert!(calls(&state).is_empty(), "once");
    }

    #[test]
    fn giving_back_hands_the_lights_then_the_profiles_to_the_keyboard() {
        let (mut dev, state, _) = keyboard();
        Host::take(&mut dev, &g915x(), look()).unwrap();
        calls(&state);
        give_back(&mut dev).unwrap();
        assert_eq!(
            calls(&state),
            [(feature::RGB_EFFECTS, 5, vec![1]), (feature::PROFILE_MANAGEMENT, 6, vec![3])],
            "01 00 00, then 03"
        );
        let k = state.lock().unwrap();
        assert!(!k.host_mode && k.control == 0);
    }

    #[test]
    fn a_look_the_keyboard_cannot_show_is_refused_before_anything_is_sent() {
        let (mut dev, state, _) = keyboard();
        let lighting = g915x();
        let mut not_fixed = look();
        not_fixed.zones[0] = zone(0, effect::CYCLE, None);
        let mut no_key = look();
        no_key.keys.push((112, RED));
        let mut logo_wave = look();
        logo_wave.zones[1] = zone(1, effect::WAVE, None);
        let mut inward = look();
        inward.keys.clear();
        inward.zones[0] = ZoneLook { direction: Some(4), ..zone(0, effect::WAVE, None) };
        let mut too_slow = look();
        too_slow.zones[1].period_ms = Some(60000);
        let mut no_zone = look();
        no_zone.zones.push(zone(2, effect::FIXED, Some(RED)));
        for bad in [not_fixed, no_key, logo_wave, inward, too_slow, no_zone] {
            assert!(matches!(Host::take(&mut dev, &lighting, bad.clone()), Err(Error::Invalid(_))), "{bad:?}");
        }
        assert!(calls(&state).is_empty());
    }

    #[test]
    fn effect_bytes_follow_the_specification() {
        let lighting = g915x();
        let zones = &lighting.effects.as_ref().unwrap().zones[0].effects;
        let bytes = |id: u16, look: ZoneLook, intensity: u8| {
            effect_params(zones.iter().find(|e| e.id == id).unwrap(), &look, intensity).unwrap()
        };
        // Colour cycle: full saturation, the period rounded to the keyboard's unit (992 ms).
        let cycle = ZoneLook { period_ms: Some(5000), ..zone(0, effect::CYCLE, None) };
        assert_eq!(bytes(effect::CYCLE, cycle, 0), [0, 0xFF, 0, 0, 0, 0, 0x13, 0x60, 0, 0]);
        // Wave to the left (6), dimmed to 25.
        let wave = ZoneLook { period_ms: Some(3000), direction: Some(6), ..zone(0, effect::WAVE, None) };
        assert_eq!(bytes(effect::WAVE, wave, 25), [0, 0xFF, 0, 0, 0, 0, 0x0B, 0xB8, 25, 6]);
        // Ripple: colour, full saturation, the keyboard's own period (50 ms).
        assert_eq!(
            bytes(effect::RIPPLE, zone(0, effect::RIPPLE, Some(BLUE)), 0),
            [0, 0, 0xFF, 0xFF, 0, 0, 0, 50, 0, 0]
        );
        assert_eq!(bytes(effect::OFF, zone(0, effect::OFF, None), 0), [0; 10]);
    }

    #[test]
    fn keys_are_sent_by_colour_thirteen_at_a_time() {
        let keys: Vec<(u8, Rgb)> = (1..=30).map(|k| (k, if k == 5 { BLUE } else { RED })).collect();
        let reports = key_reports(&keys);
        // Blue first (it sorts lower), then 29 red keys: 13, 13 and 3.
        assert_eq!(reports[0], [0, 0, 0xFF, 5]);
        assert_eq!(reports.len(), 4);
        assert_eq!(reports[1][..4], [0xFF, 0, 0, 1]);
        assert_eq!(reports[1].len(), 16);
        assert_eq!(reports[3], [0xFF, 0, 0, 28, 29, 30]);
    }

    #[test]
    fn idle_dims_once_then_turns_off_and_use_brings_the_look_back() {
        let (mut dev, state, pending) = keyboard();
        let lighting = g915x();
        let mut host = Host::take(&mut dev, &lighting, look()).unwrap();
        calls(&state);
        // The keyboard repeats its notice: dimmed once.
        for _ in 0..3 {
            pending.lock().unwrap().push_back(idle_event(false));
        }
        let events = dev.events(0).unwrap();
        host.on_events(&mut dev, &lighting, &events).unwrap();
        assert_eq!(host.status().power, Power::Dimmed);
        assert_eq!(host.status().idle_events, 1);
        let sent = calls(&state);
        // Red at a quarter, and the breathing logo with intensity 25.
        assert_eq!(sent[0].2[..6], [0, 1, 0x40, 0, 0, 2]);
        assert_eq!(sent[sent.len() - 1].2[..9], [1, 2, 0, 0, 0x40, 0x13, 0x88, 0, 25]);
        assert_eq!(sent.iter().filter(|c| c.1 == 7).count(), 1, "one frame");

        host.on_timer(&mut dev).unwrap();
        assert!(calls(&state).is_empty(), "not yet");
        host.idle_for(Duration::from_secs(270));
        host.on_timer(&mut dev).unwrap();
        assert_eq!(calls(&state), [(feature::RGB_EFFECTS, 8, vec![1, 3])]);
        assert_eq!(host.status().power, Power::Off);

        pending.lock().unwrap().push_back(idle_event(true));
        let events = dev.events(0).unwrap();
        host.on_events(&mut dev, &lighting, &events).unwrap();
        let sent = calls(&state);
        assert_eq!(sent[0], (feature::RGB_EFFECTS, 8, vec![1, 1]));
        assert_eq!(sent[1], (feature::RGB_EFFECTS, 5, vec![1, 3, 6]));
        assert_eq!(sent[2].2[..6], [0, 1, 0xFF, 0, 0, 2], "full colours again");
        assert_eq!((host.status().power, host.status().active_events), (Power::Full, 1));
    }

    #[test]
    fn a_busy_keyboard_is_asked_again_and_a_failed_frame_is_not_ended() {
        let (mut dev, state, _) = keyboard();
        let lighting = g915x();
        state.lock().unwrap().busy = 2;
        Host::take(&mut dev, &lighting, look()).unwrap();
        let sent = calls(&state);
        assert_eq!(sent.iter().filter(|c| c.0 == feature::PER_KEY_LIGHTING_V2 && c.1 == 6).count(), 2 + 2 * 11);

        state.lock().unwrap().busy = 4;
        assert!(Host::take(&mut dev, &lighting, look()).is_err());
        let sent = calls(&state);
        assert!(!sent.iter().any(|c| c.0 == feature::PER_KEY_LIGHTING_V2 && c.1 == 7), "no end of frame");
        // The take-over failed: the keyboard got its lights back.
        assert_eq!(
            sent[sent.len() - 2..],
            [(feature::RGB_EFFECTS, 5, vec![1]), (feature::PROFILE_MANAGEMENT, 6, vec![3])]
        );
    }

    #[test]
    fn brightness_is_checked_against_the_keyboard_s_range() {
        let (mut dev, state, _) = keyboard();
        let lighting = g915x();
        set_brightness(&mut dev, &lighting, 60).unwrap();
        assert!(set_brightness(&mut dev, &lighting, 101).is_err());
        assert_eq!(calls(&state), [(feature::BRIGHTNESS_CONTROL, 2, vec![0, 60])]);
    }
}
