//! Keyboard layouts. The physical layout (the shape of the keys: ANSI, ISO, JIS, ABNT2) comes from
//! the keyboard where its protocol has a read for it; the logical layout (what each key types, so
//! the legends to draw) comes from the system. Nothing here talks to a device: the drivers read,
//! with commands already in their lists, and hand the answer to the functions below. The facts and
//! their sources are in docs/protocols/keyboard-layouts.md.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Physical {
    /// One-row Enter, wide left Shift (United States).
    Ansi,
    /// Two-row Enter and an extra key next to the left Shift (most of Europe).
    Iso,
    /// Two-row Enter, a short Space bar between conversion keys, extra keys next to Backspace and
    /// the right Shift (Japan).
    Jis,
    /// ISO with one more key next to the right Shift (Brazil).
    Abnt,
}

use Physical::{Abnt, Ansi, Iso, Jis};

/// What a keyboard says about its own layout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeviceLayout {
    /// None for a value PeriHub does not know the meaning of yet.
    pub physical: Option<Physical>,
    /// The region of its keycaps, when the keyboard tells it (Razer), such as "Italian".
    pub region: Option<&'static str>,
    /// What was read, for the page and the test report, such as "Corsair property 0x41 = 2".
    pub read: String,
}

/// Corsair Bragi property 0x41: 1 ANSI, 2 ISO, 3 ABNT (ckb-next and OpenRGB), 4 JIS (OpenRGB).
pub fn corsair(value: u32) -> DeviceLayout {
    let physical = match value {
        1 => Some(Ansi),
        2 => Some(Iso),
        3 => Some(Abnt),
        4 => Some(Jis),
        _ => None,
    };
    DeviceLayout { physical, region: None, read: format!("Corsair property 0x41 = {value}") }
}

/// Razer command 00 86, first byte of the answer: the keyboard's region (OpenRazer). The shape of
/// the Greek and Russian keyboards is not given, so it is left to the system.
pub fn razer(code: u8) -> DeviceLayout {
    let (region, physical) = match code {
        0x01 => (Some("US"), Some(Ansi)),
        0x02 => (Some("Greek"), None),
        0x03 => (Some("German"), Some(Iso)),
        0x04 => (Some("French"), Some(Iso)),
        0x05 => (Some("Russian"), None),
        0x06 => (Some("UK"), Some(Iso)),
        0x07 => (Some("Nordic"), Some(Iso)),
        0x0A => (Some("Turkish"), Some(Iso)),
        0x0C => (Some("Japanese"), Some(Jis)),
        0x0F => (Some("Swiss"), Some(Iso)),
        0x10 => (Some("Spanish"), Some(Iso)),
        0x11 => (Some("Italian"), Some(Iso)),
        0x12 => (Some("Portuguese"), Some(Iso)),
        0x81 => (Some("US (Mac)"), Some(Ansi)),
        _ => (None, None),
    };
    DeviceLayout { physical, region, read: format!("Razer 00 86 = {code:02x}") }
}

/// Logitech HID++ feature 0x4540, function 0, first byte. Solaar and OpenRGB read it the same way
/// but give the codes different meanings, and OpenRGB's table comes from Logitech's own software,
/// which PeriHub takes no facts from: the code is only reported until test reports tell what it
/// means, and the shape comes from the system.
pub fn logitech(code: u8) -> DeviceLayout {
    DeviceLayout { physical: None, region: None, read: format!("Logitech HID++ 0x4540 = {code:02x}") }
}

/// The legend sets the interface can draw (app/src/lib/keys.ts).
pub const LEGENDS: [&str; 12] = ["us", "uk", "it", "de", "fr", "es", "pt", "se", "no", "dk", "jp", "br"];

/// The shape a layout is usually typed on: ANSI for US, JIS for Japanese, ABNT2 for Brazilian,
/// ISO for the European ones.
pub fn physical_for(legends: &str) -> Physical {
    match legends {
        "us" => Ansi,
        "jp" => Jis,
        "br" => Abnt,
        _ => Iso,
    }
}

/// The legends of an xkb layout and variant ("it", "de" and "nodeadkeys"…). Variants that move the
/// letters (Dvorak, Colemak, BÉPO…) have no legends of their own here.
pub fn legends_for_xkb(layout: &str, variant: &str) -> Option<&'static str> {
    const MOVED: [&str; 6] = ["dvorak", "colemak", "workman", "bepo", "neo", "norman"];
    let variant = variant.to_ascii_lowercase();
    if MOVED.iter().any(|m| variant.contains(m)) {
        return None;
    }
    Some(match layout.trim() {
        "us" => "us",
        "gb" | "ie" => "uk",
        "it" => "it",
        "de" | "at" => "de",
        "fr" => "fr",
        "es" => "es",
        "pt" => "pt",
        "se" | "fi" => "se",
        "no" => "no",
        "dk" => "dk",
        "jp" => "jp",
        "br" => "br",
        _ => return None,
    })
}

/// The legends of a Windows language id (the low word of a keyboard layout handle, or its high
/// word when that names the layout). Regions with layouts of their own that PeriHub does not draw
/// (Swiss, Belgian, Canadian French, Latin American) have none.
pub fn legends_for_langid(id: u16) -> Option<&'static str> {
    Some(match id {
        0x0809 | 0x1809 => "uk",
        0x0410 => "it",
        0x0407 | 0x0C07 | 0x1007 | 0x1407 => "de",
        0x040C | 0x140C | 0x180C => "fr",
        0x040A | 0x0C0A => "es",
        0x0816 => "pt",
        0x041D | 0x040B | 0x081D => "se",
        0x0414 | 0x0814 => "no",
        0x0406 => "dk",
        0x0411 => "jp",
        0x0416 => "br",
        // English elsewhere (US, Canada, Australia, India…) types on the US layout.
        id if id & 0x3FF == 0x09 => "us",
        _ => return None,
    })
}

/// The layout the system types with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SystemLayout {
    /// One of [`LEGENDS`], or None for a layout PeriHub has no legends for.
    pub legends: Option<&'static str>,
    /// The system's name for it: xkb's ("it", "us(intl)"), or the Windows keyboard layout handle
    /// in hex ("04100410").
    pub name: String,
    /// Where it was read: "Hyprland", "Sway", "GNOME", "KDE Plasma", "XKB_DEFAULT_LAYOUT", "X11",
    /// "localectl" or "Windows".
    pub source: &'static str,
    /// The shape it implies: from the system on Windows (a Japanese keyboard), else from the layout.
    pub physical: Option<Physical>,
}

/// The layout the system types with now, read without root and without opening any device.
pub fn system() -> Option<SystemLayout> {
    #[cfg(target_os = "linux")]
    {
        xkb_names().map(|n| n.system_layout())
    }
    #[cfg(windows)]
    {
        Some(windows::current())
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        None
    }
}

/// From a Windows keyboard layout handle and GetKeyboardType(0) (7 is a Japanese keyboard). The
/// handle's high word names the layout when it is a language id; 0xFxxx (a variant such as
/// US-International) and 0xExxx (an input method) do not, and then the language in the low word
/// says it.
pub fn from_windows(hkl: u32, keyboard_type: i32) -> SystemLayout {
    let language = (hkl & 0xFFFF) as u16;
    let device = (hkl >> 16) as u16;
    let layout = if device == 0 || device >> 12 >= 0xE { language } else { device };
    let legends = legends_for_langid(layout);
    let physical = if keyboard_type == 7 { Some(Jis) } else { legends.map(physical_for) };
    SystemLayout { legends, name: format!("{hkl:08x}"), source: "Windows", physical }
}

#[cfg(windows)]
mod windows {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetKeyboardLayout, GetKeyboardType};
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

    /// The layout of the window in front (PeriHub's own while the user looks at it), else the
    /// one this thread starts with, which is the user's default.
    pub fn current() -> super::SystemLayout {
        // SAFETY: plain Win32 calls; no pointer is kept, the process id is not asked for.
        let (hkl, kind) = unsafe {
            let window = GetForegroundWindow();
            let thread = if window.is_null() { 0 } else { GetWindowThreadProcessId(window, std::ptr::null_mut()) };
            (GetKeyboardLayout(thread) as usize as u32, GetKeyboardType(0))
        };
        super::from_windows(hkl, kind)
    }
}

/// An xkb layout, variant and options, and where they were read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XkbNames {
    pub layout: String,
    pub variant: String,
    pub options: String,
    pub source: &'static str,
}

impl XkbNames {
    fn new(layout: &str, variant: &str, options: &str, source: &'static str) -> Option<Self> {
        let layout = layout.trim();
        (!layout.is_empty()).then(|| Self {
            layout: layout.into(),
            variant: variant.trim().into(),
            options: options.trim().into(),
            source,
        })
    }

    pub fn system_layout(&self) -> SystemLayout {
        let legends = legends_for_xkb(&self.layout, &self.variant);
        let name =
            if self.variant.is_empty() { self.layout.clone() } else { format!("{}({})", self.layout, self.variant) };
        SystemLayout { legends, name, source: self.source, physical: legends.map(physical_for) }
    }
}

/// The item of a comma-separated list at `index` ("it,us" → "us" for 1), or "".
fn nth(list: &str, index: usize) -> &str {
    list.split(',').nth(index).unwrap_or("").trim()
}

/// Hyprland's `j/devices`: its main keyboard's active layout.
pub fn from_hyprland(devices: &serde_json::Value) -> Option<XkbNames> {
    let keyboards = devices.get("keyboards")?.as_array()?;
    let main = keyboards.iter().find(|k| k.get("main").and_then(|m| m.as_bool()) == Some(true)).or(keyboards.first())?;
    let text = |name: &str| main.get(name).and_then(|v| v.as_str()).unwrap_or("");
    // With several layouts ("us,it") the active one is chosen by its index.
    let index = main.get("active_layout_index").and_then(|i| i.as_u64()).unwrap_or(0) as usize;
    XkbNames::new(nth(text("layout"), index), nth(text("variant"), index), text("options"), "Hyprland")
}

/// The xkb layout of a description as Sway gives it ("Italian", "English (UK)"): Sway names the
/// active layout only by its description.
fn xkb_for_description(description: &str) -> Option<&'static str> {
    const EXACT: [(&str, &str); 4] =
        [("English (US)", "us"), ("English (UK)", "gb"), ("Portuguese (Brazil)", "br"), ("German (Austria)", "at")];
    if let Some((_, code)) = EXACT.iter().find(|(d, _)| *d == description) {
        return Some(code);
    }
    if description.starts_with("English (US") {
        return Some("us");
    }
    if description.starts_with("English (UK") {
        return Some("gb");
    }
    // A variant of a language's own layout: "German (no dead keys)".
    let base = description.split(" (").next()?;
    let variant = description.strip_prefix(base).unwrap_or("");
    const BASES: [(&str, &str); 11] = [
        ("Italian", "it"),
        ("German", "de"),
        ("French", "fr"),
        ("Spanish", "es"),
        ("Portuguese", "pt"),
        ("Swedish", "se"),
        ("Finnish", "fi"),
        ("Norwegian", "no"),
        ("Danish", "dk"),
        ("Japanese", "jp"),
        ("Irish", "ie"),
    ];
    let code = BASES.iter().find(|(b, _)| *b == base)?.1;
    // A layout for another country in that language ("French (Canada)", "German (Switzerland)"), or
    // one that moves the letters.
    const OTHER: [&str; 7] = ["Canada", "Switzerland", "Belgium", "Latin American", "Brazil", "Dvorak", "BEPO"];
    (!OTHER.iter().any(|c| variant.contains(c))).then_some(code)
}

/// `swaymsg -t get_inputs -r`: the first keyboard's active layout.
pub fn from_sway(inputs: &serde_json::Value) -> Option<XkbNames> {
    let keyboard = inputs.as_array()?.iter().find(|i| {
        i.get("type").and_then(|t| t.as_str()) == Some("keyboard") && i.get("xkb_active_layout_name").is_some()
    })?;
    let description = keyboard.get("xkb_active_layout_name")?.as_str()?;
    XkbNames::new(xkb_for_description(description)?, "", "", "Sway")
}

/// GNOME's `input-sources` list, as gsettings prints it: "[('xkb', 'it'), ('ibus', 'mozc-jp')]".
/// The first xkb source, "it" or "us+intl" (layout and variant).
pub fn from_gsettings(text: &str) -> Option<XkbNames> {
    let mut quoted = text.split('\'').skip(1).step_by(2);
    while let (Some(kind), Some(id)) = (quoted.next(), quoted.next()) {
        if kind == "xkb" {
            let (layout, variant) = id.split_once('+').unwrap_or((id, ""));
            return XkbNames::new(layout, variant, "", "GNOME");
        }
    }
    None
}

/// KDE Plasma's `kxkbrc`: its first layout, when Plasma sets the layouts itself (Use=true).
pub fn from_kxkbrc(text: &str) -> Option<XkbNames> {
    let mut in_layout = false;
    let (mut used, mut layouts, mut variants, mut options) = (false, "", "", "");
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            in_layout = line == "[Layout]";
            continue;
        }
        let Some((key, value)) = line.split_once('=').filter(|_| in_layout) else { continue };
        match key.trim() {
            "Use" => used = value.trim() == "true",
            "LayoutList" => layouts = value,
            "VariantList" => variants = value,
            "Options" => options = value,
            _ => {}
        }
    }
    if !used {
        return None;
    }
    XkbNames::new(nth(layouts, 0), nth(variants, 0), options, "KDE Plasma")
}

/// The first layout of the lines that give them (`setxkbmap -query`, `localectl status`).
fn from_fields(text: &str, fields: [&str; 3], source: &'static str) -> Option<XkbNames> {
    let field = |name: &str| {
        text.lines().find_map(|l| l.trim().strip_prefix(name)).map(|v| v.trim().to_string()).unwrap_or_default()
    };
    let [layout, variant, options] = fields.map(field);
    XkbNames::new(nth(&layout, 0), nth(&variant, 0), &options, source)
}

/// `setxkbmap -query`, on X11.
pub fn from_setxkbmap(text: &str) -> Option<XkbNames> {
    from_fields(text, ["layout:", "variant:", "options:"], "X11")
}

/// `localectl status`: the system's layout.
pub fn from_localectl(text: &str) -> Option<XkbNames> {
    from_fields(text, ["X11 Layout:", "X11 Variant:", "X11 Options:"], "localectl")
}

/// The layout the desktop types with: Hyprland's or Sway's active one, GNOME's or Plasma's
/// first, the XKB_DEFAULT_* variables, X11's, and the system's (localectl). All of them are read
/// as the user, without root.
#[cfg(target_os = "linux")]
pub fn xkb_names() -> Option<XkbNames> {
    use std::process::Command;
    let run = |program: &str, args: &[&str]| {
        let output = Command::new(program).args(args).output().ok().filter(|o| o.status.success())?;
        Some(String::from_utf8_lossy(&output.stdout).into_owned())
    };
    let env = |name: &str| std::env::var(name).unwrap_or_default();
    let desktop = env("XDG_CURRENT_DESKTOP").to_ascii_uppercase();
    let hyprland = || {
        let dir = crate::focus::hyprland::socket_dir()?;
        from_hyprland(&crate::focus::hyprland::request(&dir, "j/devices")?)
    };
    let sway = || {
        std::env::var_os("SWAYSOCK")?;
        from_sway(&serde_json::from_str(&run("swaymsg", &["-t", "get_inputs", "-r"])?).ok()?)
    };
    // The most recently used source is the active one; a fresh setup has only the list.
    let gnome = || {
        if !desktop.contains("GNOME") {
            return None;
        }
        ["mru-sources", "sources"]
            .iter()
            .find_map(|key| from_gsettings(&run("gsettings", &["get", "org.gnome.desktop.input-sources", key])?))
    };
    let plasma = || {
        if !desktop.contains("KDE") {
            return None;
        }
        from_kxkbrc(&std::fs::read_to_string(dirs::config_dir()?.join("kxkbrc")).ok()?)
    };
    let variables = || {
        let (layout, variant) = (env("XKB_DEFAULT_LAYOUT"), env("XKB_DEFAULT_VARIANT"));
        XkbNames::new(nth(&layout, 0), nth(&variant, 0), &env("XKB_DEFAULT_OPTIONS"), "XKB_DEFAULT_LAYOUT")
    };
    let x11 = || {
        if env("XDG_SESSION_TYPE") != "x11" {
            return None;
        }
        from_setxkbmap(&run("setxkbmap", &["-query"])?)
    };
    let system = || from_localectl(&run("localectl", &["status"])?);
    hyprland().or_else(sway).or_else(gnome).or_else(plasma).or_else(variables).or_else(x11).or_else(system)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn corsair_values() {
        assert_eq!(corsair(1).physical, Some(Ansi));
        assert_eq!(corsair(2).physical, Some(Iso));
        assert_eq!(corsair(3).physical, Some(Abnt));
        assert_eq!(corsair(4).physical, Some(Jis));
        assert_eq!(corsair(9).physical, None);
        assert_eq!(corsair(2).read, "Corsair property 0x41 = 2");
    }

    #[test]
    fn razer_codes() {
        let it = razer(0x11);
        assert_eq!((it.region, it.physical), (Some("Italian"), Some(Iso)));
        assert_eq!(razer(0x01).physical, Some(Ansi));
        assert_eq!(razer(0x0C).physical, Some(Jis));
        let ru = DeviceLayout { physical: None, region: Some("Russian"), read: "Razer 00 86 = 05".into() };
        assert_eq!(razer(0x05), ru);
        assert_eq!(razer(0x42).region, None);
    }

    #[test]
    fn logitech_codes_are_only_reported() {
        let read = DeviceLayout { physical: None, region: None, read: "Logitech HID++ 0x4540 = 1a".into() };
        assert_eq!(logitech(0x1a), read);
    }

    #[test]
    fn xkb_layouts() {
        assert_eq!(legends_for_xkb("it", ""), Some("it"));
        assert_eq!(legends_for_xkb("gb", "extd"), Some("uk"));
        assert_eq!(legends_for_xkb("de", "nodeadkeys"), Some("de"));
        assert_eq!(legends_for_xkb("fi", ""), Some("se"));
        assert_eq!(legends_for_xkb("us", "intl"), Some("us"));
        assert_eq!(legends_for_xkb("us", "dvorak"), None);
        assert_eq!(legends_for_xkb("fr", "bepo"), None);
        assert_eq!(legends_for_xkb("ru", ""), None);
        assert_eq!(physical_for("us"), Ansi);
        assert_eq!(physical_for("it"), Iso);
        assert_eq!(physical_for("jp"), Jis);
        assert_eq!(physical_for("br"), Abnt);
    }

    #[test]
    fn windows_layout_handles() {
        // Italian layout, Italian language.
        let it = from_windows(0x0410_0410, 4);
        assert_eq!((it.legends, it.physical, it.name.as_str()), (Some("it"), Some(Iso), "04100410"));
        // The US layout with the Italian language: the layout wins.
        assert_eq!(from_windows(0x0409_0410, 4).legends, Some("us"));
        // US-International (a variant id): the language says it.
        assert_eq!(from_windows(0xF001_0409, 4).legends, Some("us"));
        // The Japanese input method on a Japanese keyboard.
        let jp = from_windows(0xE001_0411, 7);
        assert_eq!((jp.legends, jp.physical), (Some("jp"), Some(Jis)));
        // English (UK), German (Austria), Swiss German (not drawn), Brazilian, English (Australia).
        assert_eq!(from_windows(0x0809_0809, 4).legends, Some("uk"));
        assert_eq!(from_windows(0x0C07_0C07, 4).legends, Some("de"));
        assert_eq!(from_windows(0x0807_0807, 4).legends, None);
        assert_eq!(from_windows(0x0416_0416, 4).physical, Some(Abnt));
        assert_eq!(from_windows(0x0C09_0C09, 4).legends, Some("us"));
    }

    #[test]
    fn hyprland_active_layout() {
        let devices = json!({"keyboards": [
            {"name": "other", "main": false, "layout": "us", "variant": "", "options": "", "active_layout_index": 0},
            {"name": "kb", "main": true, "layout": "us,it", "variant": ",", "options": "compose:ralt", "active_layout_index": 1}
        ]});
        let names = from_hyprland(&devices).unwrap();
        assert_eq!((names.layout.as_str(), names.options.as_str(), names.source), ("it", "compose:ralt", "Hyprland"));
        assert_eq!(names.system_layout().legends, Some("it"));
    }

    #[test]
    fn sway_descriptions() {
        let inputs = json!([
            {"type": "pointer", "name": "mouse"},
            {"type": "keyboard", "name": "kb", "xkb_active_layout_name": "German (no dead keys)"}
        ]);
        assert_eq!(from_sway(&inputs).unwrap().layout, "de");
        assert_eq!(xkb_for_description("English (US)"), Some("us"));
        assert_eq!(xkb_for_description("English (UK, extended, with Win keys)"), Some("gb"));
        assert_eq!(xkb_for_description("Portuguese (Brazil)"), Some("br"));
        assert_eq!(xkb_for_description("French (Canada)"), None);
        assert_eq!(xkb_for_description("Russian"), None);
    }

    #[test]
    fn gnome_sources() {
        assert_eq!(from_gsettings("[('xkb', 'it'), ('xkb', 'us')]").unwrap().layout, "it");
        let intl = from_gsettings("[('ibus', 'mozc-jp'), ('xkb', 'us+intl')]").unwrap();
        assert_eq!((intl.layout.as_str(), intl.variant.as_str()), ("us", "intl"));
        assert_eq!(from_gsettings("@a(ss) []"), None);
    }

    #[test]
    fn plasma_kxkbrc() {
        let text = "[$Version]\nupdate_info=kxkb.upd:remove-empty-lists\n\n[Layout]\nDisplayNames=,\nLayoutList=de,us\nUse=true\nVariantList=nodeadkeys,\n";
        let names = from_kxkbrc(text).unwrap();
        assert_eq!((names.layout.as_str(), names.variant.as_str()), ("de", "nodeadkeys"));
        assert_eq!(from_kxkbrc("[Layout]\nLayoutList=de\nUse=false\n"), None);
    }

    #[test]
    fn localectl_and_setxkbmap() {
        let localectl = "   System Locale: LANG=it_IT.UTF-8\n       VC Keymap: it\n      X11 Layout: it,us\n       X11 Model: pc105\n";
        let names = from_localectl(localectl).unwrap();
        assert_eq!((names.layout.as_str(), names.variant.as_str(), names.source), ("it", "", "localectl"));
        assert_eq!(from_localectl("   System Locale: LANG=C\n       VC Keymap: n/a\n"), None);
        let x11 = "rules:      evdev\nmodel:      pc105\nlayout:     fr,us\nvariant:    oss,\n";
        let names = from_setxkbmap(x11).unwrap();
        assert_eq!((names.layout.as_str(), names.variant.as_str()), ("fr", "oss"));
        assert_eq!(names.system_layout().name, "fr(oss)");
    }
}
