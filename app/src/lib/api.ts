import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

// ---- Types shared with the Rust backend (same field names as serde) ----

export type Rgb = string; // "#rrggbb"

// Keyboard layouts (perihub-core kblayout.rs); app/src/lib/keyboardLayout.svelte.ts combines them.
export type Physical = "ansi" | "iso" | "jis" | "abnt";
export type Legends = "us" | "uk" | "it" | "de" | "fr" | "es" | "pt" | "se" | "no" | "dk" | "jp" | "br";

/** What a keyboard says about its own layout. */
export interface DeviceLayout {
  /** null for a value PeriHub does not know the meaning of yet. */
  physical: Physical | null;
  /** The region of its keycaps, when the keyboard tells it (Razer). */
  region: string | null;
  /** What was read, such as "Corsair property 0x41 = 2". */
  read: string;
}

/** The layout the system types with. */
export interface SystemLayout {
  /** null for a layout PeriHub has no legends for. */
  legends: Legends | null;
  /** The system's own name for it: "it", "us(intl)", or a Windows layout handle. */
  name: string;
  /** Where it was read: "localectl", "GNOME", "Windows"… */
  source: string;
  physical: Physical | null;
}

export type KeyboardLighting =
  | { type: "off" }
  | { type: "static"; colors: Rgb[] }
  | { type: "gradient"; from: Rgb; to: Rgb }
  | { type: "breathing"; colors: Rgb[]; period_ms: number }
  | { type: "color_cycle"; period_ms: number }
  | { type: "wave"; period_ms: number; reverse: boolean }
  | { type: "hardware_rainbow" }
  | { type: "reactive"; base: Rgb; flash: Rgb; fade_ms: number };

export interface KeyboardConfig {
  lighting: KeyboardLighting;
  brightness: number;
}

/** A keyboard lighting saved by name, for app profiles. */
export interface LightingPreset {
  name: string;
  lighting: KeyboardLighting;
}

/** Permission to read the keyboard's key presses, for reactive lighting (Linux). */
export interface KeyAccess {
  /** PeriHub can read key presses now (null: keyboard not connected). */
  readable: boolean | null;
  rule_installed: boolean;
  supported: boolean;
  manual_install: string;
  manual_remove: string;
}

export interface AppRule {
  /** Window class on Linux, executable name on Windows. */
  app: string;
  /** Saved lighting to show, or null to keep the keyboard as it is. */
  lighting: string | null;
  /** Mouse onboard profile to activate, or null to keep it as it is. */
  mouse_profile: number | null;
}

export interface AppProfiles {
  enabled: boolean;
  rules: AppRule[];
}

export type FocusSupport =
  | { kind: "supported"; desktop: string }
  | { kind: "unsupported"; reason: string }
  /** Possible after a one-time step started with a button labelled `action` (`api.setupFocus`). */
  | { kind: "setup"; desktop: string; reason: string; action: string }
  /** Possible after the user runs `command` in a terminal outside the sandbox (GNOME in the Flatpak). */
  | { kind: "host_command"; desktop: string; reason: string; command: string };

export interface ProfilesInfo {
  support: FocusSupport;
  focused: string | null;
  /** Apps focused recently, most recent first. */
  recent: string[];
  profiles: AppProfiles;
  /** Names of the saved lightings. */
  presets: string[];
}

export interface InstalledGame {
  name: string;
  /** "Steam", "Epic", "Lutris" or "Heroic". */
  launcher: string;
  /** Executable name on Windows, window class on Linux. */
  app: string;
  /** False when the app is a best guess that may need correcting. */
  certain: boolean;
  /** Why it is not certain. */
  note: string | null;
}

export type KeyBinding =
  | { type: "key"; usage: number }
  | { type: "function"; code: number; param: number }
  | { type: "none" }
  | { type: "raw"; bytes: number[] };

export interface Keymap {
  normal: Record<string, KeyBinding>;
  steelseries: Record<string, KeyBinding>;
}

export interface KeyInfo {
  usage: number;
  name: string;
}

export interface KeyCatalog {
  targets: KeyInfo[];
  sources: number[];
}

export type SpecialFunction =
  | "tilt_left"
  | "tilt_right"
  | "dpi_up"
  | "dpi_down"
  | "dpi_cycle"
  | "dpi_default"
  | "dpi_shift"
  | "profile_next"
  | "profile_prev"
  | "profile_cycle"
  | "g_shift";

/** One command of a macro. `key` is 0 when only modifiers are pressed; mouse `buttons` is a bit mask. */
export type MacroStep =
  | { type: "key_down"; modifiers: number; key: number }
  | { type: "key_up"; modifiers: number; key: number }
  | { type: "mouse_down"; buttons: number }
  | { type: "mouse_up"; buttons: number }
  | { type: "consumer_down"; usage: number }
  | { type: "consumer_up"; usage: number }
  /** Wheel notches: positive scrolls up. */
  | { type: "wheel"; amount: number }
  | { type: "delay"; ms: number }
  | { type: "noop" };

/** A step of a macro run by PeriHub: the device steps plus typing text and opening a website or program. */
export type SoftStep =
  MacroStep | { type: "text"; text: string } | { type: "open"; target: string };

/** once; repeat a number of times; toggle: again and again until the button is pressed again. */
export type SoftMode =
  { type: "once" } | { type: "repeat"; times: number } | { type: "toggle" };

/** Macro run by PeriHub; the buttons that start it send `key` (F19, F24, F13–F18). Key 0: new. */
export interface SoftwareMacro {
  key: number;
  name: string;
  mode: SoftMode;
  steps: SoftStep[];
}

export interface SoftMacrosInfo {
  macros: SoftwareMacro[];
  /** Whether PeriHub can simulate a keyboard and a mouse, and which macros are running. */
  status: {
    available: boolean;
    error: string | null;
    running: number[];
    /** PeriHub reads the keyboard's keys, because one of them starts a macro. */
    keyboard_read: boolean;
    /** A keyboard key starts a macro, but its keys cannot be read (Linux: permission missing). */
    keyboard_error: string | null;
  };
  /** Trigger keys not used yet. */
  free_keys: number[];
  /** Linux: PeriHub's optional /dev/uinput rule is installed / can be installed. */
  rule_installed: boolean;
  rule_supported: boolean;
}

/** once: when pressed. sequence: `press` when pressed, `release` when released. while_held: again
 * and again while held. forever: until the profile changes (read only, PeriHub does not write it). */
export type MacroKind = "once" | "sequence" | "while_held" | "forever";

export interface Macro {
  kind: MacroKind;
  press: MacroStep[];
  release: MacroStep[];
}

export type ButtonAction =
  | { type: "mouse"; button: number }
  | { type: "key"; modifiers: number; key: number }
  | { type: "consumer"; usage: number }
  | { type: "special"; function: SpecialFunction }
  | { type: "disabled" }
  /** Macro in the mouse's memory; `content` is null if PeriHub cannot read it. A new or changed
   * macro gets its place in memory when the profile is saved (sector 0 = not placed yet). */
  | { type: "macro"; sector: number; offset: number; content: Macro | null }
  | { type: "raw"; bytes: number[] };

export type LedEffect =
  | { mode: "off" }
  | { mode: "fixed"; color: Rgb }
  | { mode: "breathing"; color: Rgb; period_ms: number; brightness: number }
  | { mode: "cycle"; period_ms: number; brightness: number }
  | { mode: "raw"; bytes: number[] };

export interface ProfileSettings {
  report_rate_ms: number;
  /** Polling rate in Hz, for memory formats that go above 1000 Hz (format 7); then `report_rate_ms` is 0 above 1000 Hz. */
  report_rate_hz?: number;
  dpi_levels: number[];
  default_dpi_index: number;
  shift_dpi_index: number;
  buttons: ButtonAction[];
  gshift_buttons: ButtonAction[];
  leds: LedEffect[];
}

export interface MouseProfile {
  number: number;
  enabled: boolean;
  settings: ProfileSettings;
}

export interface Battery {
  percent: number;
  /** Worked out from the voltage or a coarse level: the mouse reports no percentage. */
  estimated: boolean;
  state: "discharging" | "charging" | "full";
}

/** What changes without PeriHub: active profile and DPI (the mouse's buttons), the battery. */
export interface MouseLive {
  active_profile: number;
  active_dpi_index: number;
  current_dpi: number;
  battery: Battery | null;
}

export interface MouseState {
  active_profile: number;
  active_dpi_index: number;
  current_dpi: number;
  profiles: MouseProfile[];
  /** Bytes free for new macros in the mouse's memory. */
  macro_space: number;
  /** With "factory" the mouse uses its factory profiles, which are then the ones in `profiles`. */
  profile_list: ProfileList;
}

/**
 * The list in the mouse's memory that says which profiles it uses: its own, none yet (nothing was
 * ever saved to the mouse, which uses its factory profiles), or one whose checksum does not match.
 */
export type ProfileList = "own" | "factory" | "damaged";

export interface LedZone {
  index: number;
  location: number;
  effects: number[];
}

export interface ReceiverLink {
  /** USB product id of the receiver. */
  receiver_id: number;
  slot: number;
  /** Id the mouse reports through the receiver. */
  wireless_id: number;
}

/** An entry of a Logitech device's HID++ feature table: kind holds the flags (0x80 obsolete, 0x40 hidden, 0x20 engineering). */
export interface HidppFeature {
  id: number;
  kind: number;
  version: number;
}

/** What a PRO X 2 style mouse reports through its extended DPI and polling rate features and
 * format 7 profiles. PeriHub only reads it. */
export interface ExtendedReads {
  /** The Y axis, when it has a range of its own (the X axis is `MouseInfo.dpi`). */
  dpi_y: { min: number; max: number; step: number; list: number[] } | null;
  dpi_now: { x: number; default_x: number; y: number | null; default_y: number | null; lod: number | null } | null;
  rates_wired_hz: number[];
  rates_wireless_hz: number[];
  /** The rate on the connection the mouse is on. */
  rate_now_hz: number | null;
  /** What the mouse sent, as received. */
  raw: string[];
  /** One line for each profile of memory format 7. */
  profiles: string[];
  errors: string[];
}

export interface MouseInfo {
  product_id: number;
  /** Model name from PeriHub's list, e.g. "G502 HERO". */
  model: string;
  /** Name reported by the mouse itself. */
  name: string;
  /** Tested: its memory format (profile format and sector size), or the model itself, was tested on a
   * real mouse. Profiles and DPI levels can be switched, also through a receiver. */
  verified: boolean;
  /** Its memory format can be written but nobody has tested it yet: saves are experimental, and each
   * one is undone unless it is kept. With the cable only. */
  experimental: boolean;
  /** A save that was neither kept nor undone waits for the mouse to be connected with its cable. */
  pending_undo: boolean;
  /** Connected through a LIGHTSPEED receiver instead of its cable. */
  receiver: boolean;
  /** Which receiver and slot, for test reports. */
  link: ReceiverLink | null;
  /** Another program left the mouse under software control. */
  host_mode: boolean;
  firmware: { kind: number; name: string; version: string }[];
  dpi: { min: number; max: number; step: number; list: number[] };
  report_rates_ms: number[];
  /** Extended features (0x2202, 0x8061) and format 7 profiles, if the mouse has them. */
  extended?: ExtendedReads | null;
  /** Every request and answer of the reading when PeriHub connected, as a transcript in JSON
   * (docs/replay.md), for test reports. Nothing in it tells the unit apart. */
  transcript?: string;
  led_zones: LedZone[];
  /** The HID++ features the mouse lists, for test reports. */
  features: HidppFeature[];
  onboard: {
    memory_model: number;
    profile_format: number;
    macro_format: number;
    profile_count: number;
    rom_profile_count: number;
    button_count: number;
    sector_count: number;
    sector_size: number;
    has_gshift: boolean;
    has_dpi_shift: boolean;
  };
  writable: boolean;
  /** PeriHub knows how to write the memory format the mouse reports (with its USB cable). */
  writable_format: boolean;
  /** PeriHub gives the mouse its onboard profiles back by itself once no other program controls it
   * (models tested as a whole only). */
  switches_back: boolean;
  /** The mouse's profile list when it was connected (MouseState.profile_list is the state now). */
  profile_list: ProfileList;
}

export interface KeyboardInfo {
  name: string;
  firmware: string;
}

/** A connected USB or Bluetooth device PeriHub does not know, for a device request. */
export interface ReportDevice {
  vendor_id: number;
  product_id: number;
  manufacturer: string | null;
  product: string | null;
  bus: "USB" | "Bluetooth";
  /** Interface -1: the system does not say (Bluetooth). */
  collections: { interface: number; usage_page: number; usage: number }[];
}

export interface DeviceStatus {
  keyboard: KeyboardInfo | null;
  mouse: MouseInfo | null;
  keyboard_error: string | null;
  mouse_error: string | null;
  /** A wireless mouse paired with a connected receiver but turned off or asleep. */
  mouse_asleep: string | null;
  /** A keyboard PeriHub recognizes but does not support yet, such as "SteelSeries Apex 3". */
  unsupported_keyboard: string | null;
  /** Running programs that take control of the devices, such as "Logitech G HUB". */
  conflicts: string[];
  /** A Logitech G300. */
  g300: G300Info | null;
  g300_error: string | null;
  /** Corsair mice and keyboards (experimental support). */
  corsair: CorsairInfo[];
  corsair_error: string | null;
  /** Mice and keyboards of other brands, from published protocol documentation (experimental). */
  peripherals: PeripheralInfo[];
  peripheral_error: string | null;
  /** Keyboards with QMK firmware found by their raw HID channel, open or not. */
  qmk: QmkKeyboard[];
  /** Connected Logitech receivers (LIGHTSPEED, Logi Bolt and Unifying). */
  receivers: ReceiverReport[];
  /** Running programs that set Logitech devices' settings themselves, such as "Solaar". */
  logitech_programs: string[];
  /** PeriHub runs as a Flatpak and cannot see the other programs running. */
  sandboxed: boolean;
  /** The first detection pass is over: until then empty lists mean "not looked at yet", not "nothing there". */
  scanned: boolean;
}

/** A device paired with a receiver that PeriHub does not support yet, as far as it could be read. */
export interface UnsupportedReceiverDevice {
  slot: number;
  name: string | null;
  /** HID++ 0x0005 function 2: 0 keyboard, 1 remote control, 2 numpad, 3 mouse, 4 touchpad, 5 trackball,
   * 6 presenter, 7 receiver; newer devices may report other values. */
  kind: number | null;
  /** HID++ version, such as "4.5". */
  protocol: string;
  /** The id it reports through a LIGHTSPEED receiver, if it lists one. */
  wireless_id: number | null;
  /** HID++ 0x0003 function 0 as read, in hex, with the unit id left out ("xx"). */
  device_info: string | null;
  firmware: string[];
  /** Its HID++ feature table. */
  features: HidppFeature[];
  /** The DPI it accepts (0x2201), if it has that feature. */
  dpi: { min: number; max: number; step: number; list: number[] } | null;
  /** The DPI of its X axis (0x2202), if it has that instead of 0x2201. */
  dpi_extended?: { min: number; max: number; step: number; list: number[] } | null;
  /** The polling rates it offers, in milliseconds (0x8060). */
  report_rates_ms: number[];
  /** The polling rates it offers through the receiver, in Hz (0x8061). */
  report_rates_hz?: number[];
  /** Its onboard profile memory (0x8100), if it has one. */
  onboard: MouseInfo["onboard"] | null;
  /** Its lighting (0x8040, 0x8071, 0x8081, 0x4540), if it has one of those features. */
  lighting: HidppLighting | null;
  /** The zones of the older lighting feature (0x8070), with the effects each offers. */
  led_zones: LedZone[];
  /** Its battery when it was read, if it has a battery feature. */
  battery: Battery | null;
  /** The battery feature's reply as it came, such as "1004: 50 04 00 00". */
  battery_reply: string | null;
  /** PeriHub only reads it: a device on its own USB connection nobody has tested yet. */
  read_only: boolean;
  read_errors: string[];
  /** A keyboard's layout code (0x4540), if it has that feature. */
  layout?: DeviceLayout | null;
  /** Every request and answer of the reading when PeriHub connected, as a transcript in JSON
   * (docs/replay.md), for test reports. Nothing in it tells the unit apart. */
  transcript?: string;
}

/** What PeriHub read of a Logitech device's lighting, by its features. Read only. */
export interface HidppLighting {
  brightness: { min: number; max: number; steps: number; on_off: boolean } | null;
  effects: {
    zones: {
      index: number;
      /** 1 primary, 2 logo, 3 left side, 4 right side, 5 combined, 6 to 11 primary 1 to 6. */
      location: number;
      effects: { index: number; id: number; caps: number; period_ms: number }[];
    }[];
    startup: { enabled: boolean; colors: [string, string] } | null;
    shutdown: { enabled: boolean; colors: [string, string] } | null;
    idle_s: number | null;
    sleep_s: number | null;
  } | null;
  /** The ids of the keys whose colour can be set one by one (0x8081). */
  keys: number[];
  /** The layout the keyboard reports (0x4540): 1 ANSI, 2 ISO and others. */
  layout: number | null;
  /** Every reply as it came, for test reports. */
  raw: string[];
  errors: string[];
}

/** A connected Logitech receiver, for the test report that helps support what is paired with it. */
/** What changes while a device PeriHub knows by its features is in use. */
export interface LogitechLive {
  battery: Battery | null;
  /** The DPI it uses now, if it has adjustable DPI. */
  dpi: number | null;
  /** The DPI PeriHub sets every time the device connects, if one was chosen. */
  saved_dpi: number | null;
  /** Its lighting as it is now: brightness, lights on, and who drives them (0 the device, 3 a program). */
  lighting: { brightness: number | null; on: boolean | null; control: number | null } | null;
  /** While PeriHub drives a keyboard's lights (host mode): what it shows, and how it went. */
  host: LightsHost | null;
  /** Why PeriHub gave the lights back on its own, if it did. */
  gave_back: string | null;
  /** The last look shown on the keyboard, kept in PeriHub's settings: the editor starts from it. */
  saved_look: LightsLook | null;
  /** How host mode went this session, across give-backs and reconnections, once PeriHub took the lights. */
  host_log: LightsLog | null;
  /** PeriHub takes the lights with the saved look every time the keyboard connects, at every start too. */
  drive_at_start: boolean;
  /** Why PeriHub does not drive them now although it should, such as G HUB running. */
  waiting: string | null;
}

export interface LightsLog {
  taken: number;
  /** Taken again when the keyboard connected again. */
  on_connect: number;
  given_back: number;
  /** Why the last time ("asked" when the user did). */
  last_reason: string | null;
  timeouts: number;
  idle_events: number;
  active_events: number;
  retaken: number;
  shown: number;
  /** Looks that needed the lights given back and taken again (leaving a wave to the centre). */
  resets: number;
}

/** What one lighting zone of a Logitech keyboard shows (0x8071). */
export interface ZoneLook {
  zone: number;
  /** The effect's id, such as 1 for a fixed colour. */
  effect: number;
  color: string | null;
  period_ms: number | null;
  /** For a wave: 1 horizontal, 2 vertical, 3 centre out, 6 reverse horizontal, 7 reverse vertical, 8 centre in. */
  direction: number | null;
}

/** What PeriHub shows on a Logitech keyboard whose lights it drives: effects by zone, and a colour per key. */
export interface LightsLook {
  zones: ZoneLook[];
  keys: [number, string][];
}

export interface LightsHost {
  look: LightsLook;
  /** Full lights, dimmed because nobody uses the keyboard, or off. */
  power: "full" | "dimmed" | "off";
  idle_events: number;
  active_events: number;
  /** Times the keyboard was found back on its own lighting, and taken again. */
  retaken: number;
  shown: number;
  resets: number;
}

/** A keyboard with QMK firmware, found by its raw HID channel (usage page 0xFF60). */
export interface QmkKeyboard {
  /** "vvvv:pppp", as in the peripherals. */
  key: string;
  vendor_id: number;
  product_id: number;
  brand: string;
  name: string;
  /** Vial firmware, by its serial number: read without asking. */
  vial: boolean;
  /** PeriHub may open it: a Vial keyboard, or one turned on as a VIA keyboard. */
  enabled: boolean;
  /** Open now, with a page of its own. */
  open: boolean;
  /** The system refused to open it: on Linux, no rule opens its raw HID channel. */
  denied: boolean;
}

/** Linux: the optional udev rule that opens the raw HID channel of the QMK keyboards allowed. */
export interface QmkAccess {
  /** The keyboards ("vvvv:pppp") the rule opens. */
  allowed: string[];
  /** Linux: the rule is needed, and PeriHub can install it. */
  supported: boolean;
  /** Commands for a terminal, by keyboard, when the password window cannot be shown. */
  manual_install: [string, string][];
  manual_remove: string;
}

/** Linux: the optional udev rule that opens the Unifying receivers (see 73-perihub-unifying.rules). */
export interface UnifyingAccess {
  rule_installed: boolean;
  /** The rule can be installed from PeriHub (Linux only). */
  supported: boolean;
  /** Commands to run in a terminal if the password window cannot be shown. */
  manual_install: string;
  manual_remove: string;
}

export interface ReceiverReport {
  product_id: number;
  /** "LIGHTSPEED receiver", "Logi Bolt receiver" or "Unifying receiver". */
  name: string;
  /** Its HID collections, such as "interface 2, usage page ff00, usage 0002". */
  collections: string[];
  /** A Logi Bolt receiver: nothing paired with it is supported yet. */
  bolt: boolean;
  /** A Unifying receiver: nothing paired with it is supported yet, and on Linux PeriHub's udev rule
   * does not open it. */
  unifying: boolean;
  /** PeriHub found the collection it talks to the paired devices through. */
  reachable: boolean;
  /** The system refused to open it (on Linux, no udev rule gives access to it). */
  denied: boolean;
  /** Slots whose device PeriHub has open by its features: its DPI can be read and changed now. */
  open_slots: number[];
  /** What each of its slots answered the last time PeriHub asked, or why it could not ask. */
  slots: string[];
  /** Paired devices that answer and that PeriHub does not support yet. */
  unsupported: UnsupportedReceiverDevice[];
  /** Not a receiver: a device on its own USB connection. `name` says how it connects ("USB cable"),
   * `slots` what its HID++ collections answered, and `unsupported` holds the device itself (its "slot"
   * is the HID++ device index). */
  direct: boolean;
}

export type G300Button =
  | { type: "none" }
  | { type: "mouse"; button: number }
  | {
      type:
        | "dpi_up"
        | "dpi_down"
        | "dpi_cycle"
        | "profile_cycle"
        | "dpi_shift"
        | "dpi_default";
    }
  | { type: "key"; modifiers: number; key: number }
  | { type: "unknown"; code: number; modifiers: number; key: number };

export interface G300Profile {
  number: number;
  /** "#rrggbb": the G300's light is on or off per channel. */
  color: string;
  report_rate_hz: number | null;
  dpi_levels: number[];
  default_level: number | null;
  /** DPI while the DPI shift button is held; null when the shift is off. */
  shift_dpi: number | null;
  /** Byte 7 as read: the shift DPI / 250, and bit 0x40 when the shift is off. */
  shift_byte: number;
  buttons: G300Button[];
  raw: string;
}

/** What can be changed in a G300 profile. */
export interface G300Settings {
  /** "#rrggbb", each channel 00 or ff. */
  color: string;
  report_rate_hz: number;
  /** Four levels, 250 to 2500 DPI in steps of 250. */
  dpi_levels: number[];
  default_level: number;
  shift_dpi: number | null;
  /** Nine buttons; the left and right ones keep what they have. */
  buttons: G300Button[];
}

export interface G300Info {
  product_id: number;
  model: string;
  active_profile: number;
  active_level: number;
  profiles: G300Profile[];
  active_raw: string;
  /** The profiles can be saved. */
  editable: boolean;
  /** A save was neither kept nor undone, and this read-only connection could not undo it. */
  pending_undo: boolean;
}

/** The settings PeriHub shows and changes on a device of another brand. */
export interface PeripheralSettings {
  dpi_levels: number[];
  polling_hz: number | null;
  /** Other settings with a few values, by id (see PeripheralChoice). */
  options: Record<string, number>;
  leds: PeripheralLed[];
}

export interface PeripheralLed {
  /** The device's own mode number. */
  mode: number;
  /** Percent, where the device has a brightness. */
  brightness: number | null;
  /** "#rrggbb". */
  color: string;
}

export interface PeripheralChoice {
  id: string;
  label: string;
  /** Stored value and how it reads. */
  values: [number, string][];
}

/** What may be changed: something missing is shown but not changed. */
export interface PeripheralLimits {
  dpi: { min: number; max: number; step: number } | null;
  /** Empty: the polling rate is not changed. */
  polling_hz: number[];
  options: PeripheralChoice[];
  leds: {
    name: string;
    modes: [number, string][];
    brightness_step: number | null;
    color: boolean;
  }[];
}

/** A setting of a device that cannot report it: PeriHub sends it, one at a time. */
export interface BlindSetting {
  id: string;
  label: string;
  kind:
    | { type: "choice"; values: string[] }
    | { type: "color" }
    | { type: "range"; min: number; max: number; step: number }
    | {
        type: "dpi";
        min: number;
        max: number;
        step: number;
        max_presets: number;
      };
  /** What PeriHub last sent since the device was connected. */
  last: BlindValue | null;
}

export type BlindValue =
  | { type: "choice"; value: string }
  | { type: "color"; value: string }
  | { type: "number"; value: number }
  | { type: "dpi"; value: number[] };

export interface PeripheralInfo {
  /** "vvvv:pppp". */
  key: string;
  vendor_id: number;
  product_id: number;
  brand: string;
  model: string;
  kind: "mouse" | "keyboard";
  /** Where PeriHub knows the protocol from, e.g. "libratbag (MIT)". */
  source: string;
  profiles: number;
  firmware: string | null;
  battery: { level: number; charging: boolean } | null;
  /** From 1. */
  active_profile: number | null;
  active_dpi: number | null;
  /** The device can report its settings; if not, `blind` lists what PeriHub can send. */
  readable: boolean;
  settings: PeripheralSettings;
  limits: PeripheralLimits;
  buttons: { name: string; action: string }[];
  /** Other things read and shown as they are, such as what a profile stores: label and value. */
  details: [string, string][];
  blind: BlindSetting[];
  /** Changes may be tried: PeriHub's app, and everything read looked as documented. */
  editable: boolean;
  /** The device can store its settings in its memory. */
  can_save: boolean;
  /** Every change stays in the device's memory by itself: only Save to memory is offered. */
  changes_stay: boolean;
  /** Something did not look as documented: nothing is changed on it. */
  problem: string | null;
  /** A change was neither kept nor undone, and could not be undone on this connection. */
  pending_undo: boolean;
  /** Requests and answers of the last reading, cut short, for the backups. */
  raw: string[];
  /** Every request and answer in full, as a transcript in JSON (docs/replay.md), for test reports:
   * the first reading and the latest one. Nothing in it tells the unit apart. */
  transcript?: string;
  /** A keyboard's physical layout, when its protocol has a read for it. */
  layout?: DeviceLayout | null;
}

export interface CorsairInfo {
  product_id: number;
  model: string;
  kind: "mouse" | "keyboard";
  /** Connected through its wireless receiver: the receiver's USB product id. */
  receiver: number | null;
  leds: number;
  firmware: string | null;
  /** 1 = hardware (its own settings), 2 = software (a program drives it), as read at connection. */
  mode: number | null;
  polling_hz: number | null;
  dpi: number | null;
  /** What could not be read. */
  read_errors: string[];
  /** The colour shown on every LED while experimental lighting is on ("#rrggbb"). */
  lighting: string | null;
  /** A keyboard's physical layout, read from it. */
  layout?: DeviceLayout | null;
  /** Every request and answer of the reading when PeriHub connected, as a transcript in JSON
   * (docs/replay.md), for test reports. Nothing in it tells the unit apart. */
  transcript?: string;
}

export interface BatteryAlerts {
  /** Warn when a battery gets low. */
  enabled: boolean;
  /** Percent: the warning comes when the battery goes below it. */
  low_percent: number;
  /** Also tell when a battery is fully charged. */
  full: boolean;
}

export interface AppSettings {
  start_hidden: boolean;
  autostart: boolean;
  /** Look for a new version on GitHub at startup and once a day. */
  check_updates: boolean;
  /** Notifications about the battery of wireless devices. */
  battery: BatteryAlerts;
  /** This build can update itself (Windows). */
  updates_supported: boolean;
  /** The package manager that installed PeriHub and updates it, such as Scoop. */
  package_manager: string | null;
  /** Folder of the log file. */
  log_dir: string;
  /** Folder of the settings and backups. */
  config_dir: string;
  /** Operating system and architecture, for test reports. */
  system: string;
  /** Why closing the window cannot keep PeriHub in the tray (no tray on this desktop); null when it can. */
  no_tray_reason: string | null;
}

/** A write PeriHub sent to a device in this session, or refused in read-only mode. */
export interface WriteEntry {
  /** Milliseconds since 1970. */
  time: number;
  device: string;
  /** What the write does: a command name, a HID++ feature and function… */
  kind: string;
  /** The first bytes, in hex. */
  data: string;
  bytes: number;
  /** "ok", "refused (read-only mode)", or what went wrong. */
  result: string;
}

/** How far the download of an update has gone. */
export interface UpdateProgress {
  received: number;
  total: number | null;
}

export interface UpdateInfo {
  version: string;
}

export interface KeyChange {
  layer: number;
  source: number;
  binding: KeyBinding;
}

/** A backup PeriHub saved before writing to a device. */
export interface BackupEntry {
  id: string;
  /** Unix time in seconds. */
  created: number;
  /** What was about to be written: profile3, factory2, directory, keymap, restore… */
  label: string;
}

export interface KeymapRestore {
  keymap: Keymap;
  /** How many keys changed, on both layers. */
  keys: number;
}

export interface MouseRestore {
  state: MouseState;
  /** Profiles whose settings changed. */
  profiles: number[];
  macros: boolean;
  /** Whether the list of enabled profiles changed. */
  directory: boolean;
}

// ---- Commands ----

export const api = {
  status: () => invoke<DeviceStatus>("get_status"),
  devicesForReport: () => invoke<ReportDevice[]>("devices_for_report"),
  onStatus: (handler: (status: DeviceStatus) => void): Promise<UnlistenFn> =>
    listen<DeviceStatus>("devices-changed", (event) => handler(event.payload)),

  keyboardConfig: () => invoke<KeyboardConfig>("kb_get_config"),
  setLighting: (lighting: KeyboardLighting) =>
    invoke<void>("kb_set_lighting", { lighting }),
  setBrightness: (level: number) =>
    invoke<void>("kb_set_brightness", { level }),
  readBrightness: () => invoke<number>("kb_read_brightness"),
  keymap: () => invoke<Keymap>("kb_get_keymap"),
  applyKeymap: (changes: KeyChange[]) =>
    invoke<Keymap>("kb_apply_keymap", { changes }),
  resetKeymap: () => invoke<Keymap>("kb_reset_keymap"),
  keyboardBackups: () => invoke<BackupEntry[]>("kb_backups"),
  restoreKeyboardBackup: (id: string) =>
    invoke<KeymapRestore>("kb_restore_backup", { id }),
  keyCatalog: () => invoke<KeyCatalog>("kb_key_catalog"),
  systemLayout: () => invoke<SystemLayout | null>("kb_system_layout"),
  keyAccess: () => invoke<KeyAccess>("kb_key_access"),
  grantKeyAccess: () => invoke<void>("kb_grant_key_access"),
  revokeKeyAccess: () => invoke<void>("kb_revoke_key_access"),
  /** Linux: the optional rule that opens the Unifying receivers, installed through the password window. */
  /** QMK keyboards: turn one on or off as a VIA keyboard, and on Linux the optional rule for its raw HID channel. */
  qmkSetEnabled: (key: string, on: boolean) =>
    invoke<void>("qmk_set_enabled", { key, on }),
  qmkAccess: () => invoke<QmkAccess>("qmk_access"),
  grantQmkAccess: (key: string) => invoke<void>("qmk_grant_access", { key }),
  revokeQmkAccess: (key: string) => invoke<void>("qmk_revoke_access", { key }),
  unifyingAccess: () => invoke<UnifyingAccess>("unifying_access"),
  grantUnifyingAccess: () => invoke<void>("unifying_grant_access"),
  revokeUnifyingAccess: () => invoke<void>("unifying_revoke_access"),
  logitechLive: (receiverId: number, slot: number) =>
    invoke<LogitechLive>("logitech_live", { receiverId, slot }),
  /** Experimental: sets the DPI in the device's RAM, and again every time it connects. */
  logitechSetDpi: (receiverId: number, slot: number, dpi: number) =>
    invoke<void>("logitech_set_dpi", { receiverId, slot, dpi }),
  logitechResetDpi: (receiverId: number, slot: number) =>
    invoke<void>("logitech_reset_dpi", { receiverId, slot }),
  logitechDriveLights: (receiverId: number, slot: number, look: LightsLook) =>
    invoke<void>("logitech_drive_lights", { receiverId, slot, look }),
  logitechGiveBackLights: (receiverId: number, slot: number) =>
    invoke<void>("logitech_give_back_lights", { receiverId, slot }),
  logitechSetBrightness: (receiverId: number, slot: number, value: number) =>
    invoke<void>("logitech_set_brightness", { receiverId, slot, value }),
  presets: () => invoke<LightingPreset[]>("kb_get_presets"),
  savePreset: (name: string, lighting: KeyboardLighting) =>
    invoke<LightingPreset[]>("kb_save_preset", { name, lighting }),
  deletePreset: (name: string) =>
    invoke<LightingPreset[]>("kb_delete_preset", { name }),

  mouseState: () => invoke<MouseState>("mouse_get_state"),
  mouseLive: () => invoke<MouseLive>("mouse_live"),
  saveProfile: (number: number, settings: ProfileSettings) =>
    invoke<MouseState>("mouse_save_profile", { number, settings }),
  restoreFactory: (number: number) =>
    invoke<MouseState>("mouse_restore_factory", { number }),
  mouseBackups: () => invoke<BackupEntry[]>("mouse_backups"),
  restoreMouseBackup: (id: string) =>
    invoke<MouseRestore>("mouse_restore_backup", { id }),
  setProfileEnabled: (number: number, enabled: boolean) =>
    invoke<MouseState>("mouse_set_profile_enabled", { number, enabled }),
  setActiveProfile: (number: number) =>
    invoke<MouseState>("mouse_set_active_profile", { number }),
  previewDpi: (dpi: number) => invoke<void>("mouse_preview_dpi", { dpi }),
  previewLed: (zone: number, effect: LedEffect) =>
    invoke<void>("mouse_preview_led", { zone, effect }),
  revertPreview: () => invoke<void>("mouse_revert_preview"),
  g300Info: () => invoke<G300Info>("g300_info"),
  /** Saves a G300 profile: undone unless kept. */
  g300Save: (number: number, settings: G300Settings) =>
    invoke<G300Info>("g300_save", { number, settings }),
  g300KeepChanges: () => invoke<void>("g300_keep_changes"),
  g300UndoLastWrite: () => invoke<G300Info>("g300_undo_last_write"),
  /** A device of another brand, read again now. */
  peripheralInfo: (key: string) =>
    invoke<PeripheralInfo>("peripheral_info", { key }),
  /** Experimental change, also saved to the device's memory if `save`: undone unless kept. */
  /** `profile`: the active profile the settings were edited on; refused if the device switched since. */
  peripheralChange: (
    key: string,
    settings: PeripheralSettings,
    save: boolean,
    profile: number | null,
  ) =>
    invoke<PeripheralInfo>("peripheral_change", {
      key,
      settings,
      save,
      profile,
    }),
  peripheralKeepChanges: (key: string) =>
    invoke<void>("peripheral_keep_changes", { key }),
  peripheralUndo: (key: string) =>
    invoke<PeripheralInfo>("peripheral_undo", { key }),
  /** Sends one setting to a device that cannot report its settings: not saved, not undone. */
  peripheralSetBlind: (key: string, id: string, value: BlindValue) =>
    invoke<PeripheralInfo>("peripheral_set_blind", { key, id, value }),
  /** Experimental lighting on a Corsair mouse: one colour on every LED, for this session. */
  corsairSetLighting: (productId: number, color: string) =>
    invoke<CorsairInfo>("corsair_set_lighting", { productId, color }),
  corsairLightingOff: (productId: number) =>
    invoke<void>("corsair_lighting_off", { productId }),
  /** The mouse still works: the core stops counting down to give it back. */
  corsairKeepLighting: (productId: number) =>
    invoke<void>("corsair_keep_lighting", { productId }),
  /** Keeps an experimental save: the core stops counting down to undo it. */
  keepChanges: () => invoke<void>("mouse_keep_changes"),
  /** Puts back the mouse memory saved before the last write. */
  undoLastWrite: () => invoke<MouseRestore>("mouse_undo_last_write"),

  profiles: () => invoke<ProfilesInfo>("profiles_get"),
  setProfiles: (profiles: AppProfiles) =>
    invoke<void>("profiles_set", { profiles }),
  /** Games installed through Steam, the Epic Games Launcher, Lutris and Heroic (reads their files only). */
  installedGames: () => invoke<InstalledGame[]>("games_installed"),
  /** Runs the step a "setup" FocusSupport offers; resolves with what to tell the user. */
  setupFocus: () => invoke<string>("profiles_setup_focus"),
  onFocus: (handler: (app: string | null) => void): Promise<UnlistenFn> =>
    listen<string | null>("focus-changed", (event) => handler(event.payload)),

  appSettings: () => invoke<AppSettings>("app_get_settings"),
  setAppSettings: (settings: AppSettings) =>
    invoke<AppSettings>("app_set_settings", { settings }),
  quit: () => invoke<void>("app_quit"),
  /** Why the settings file was not used at start-up (it could not be read), or null. */
  configProblem: () => invoke<string | null>("app_config_problem"),
  /** Read-only mode: nothing that changes a device is sent to it. */
  readOnly: () => invoke<boolean>("app_read_only"),
  setReadOnly: (on: boolean) => invoke<boolean>("app_set_read_only", { on }),
  onReadOnly: (handler: (on: boolean) => void): Promise<UnlistenFn> =>
    listen<boolean>("read-only-changed", (event) => handler(event.payload)),
  /** The writes of this session, oldest first: kept in memory only. */
  writeLog: () => invoke<WriteEntry[]>("app_write_log"),
  clearWriteLog: () => invoke<void>("app_clear_write_log"),
  openLogFolder: () => invoke<void>("app_open_log_folder"),
  openTestForm: (url: string) => invoke<void>("app_open_test_form", { url }),
  openSupportLink: (url: string) => invoke<void>("app_open_support_link", { url }),
  softMacros: () => invoke<SoftMacrosInfo>("soft_get"),
  /** Saves a macro run by PeriHub; a new one (key 0) gets its trigger key. */
  saveSoftMacro: (item: SoftwareMacro) =>
    invoke<SoftwareMacro>("soft_save", { item }),
  deleteSoftMacro: (key: number) => invoke<void>("soft_delete", { key }),
  stopSoftMacros: () => invoke<void>("soft_stop_all"),
  grantSoftAccess: () => invoke<void>("soft_grant_access"),
  revokeSoftAccess: () => invoke<void>("soft_revoke_access"),
  /** Saves a macro to a file chosen in the system's dialog; false if cancelled. */
  exportMacro: (content: Macro, name: string) =>
    invoke<boolean>("macro_export", { content, name }),
  /** Reads a macro from a file chosen in the system's dialog; null if cancelled. */
  importMacro: () => invoke<Macro | null>("macro_import"),
  checkUpdate: () => invoke<UpdateInfo | null>("update_check"),
  /** Result of the last check made in the background, without asking GitHub again. */
  updateAvailable: () => invoke<UpdateInfo | null>("update_available"),
  onUpdate: (handler: (update: UpdateInfo) => void): Promise<UnlistenFn> =>
    listen<UpdateInfo>("update-available", (event) => handler(event.payload)),
  installUpdate: () => invoke<void>("update_install"),
  /** The version now running, once, when the installer of an update started PeriHub. */
  updateJustInstalled: () => invoke<string | null>("update_just_installed"),
  onUpdateProgress: (handler: (progress: UpdateProgress) => void): Promise<UnlistenFn> =>
    listen<UpdateProgress>("update-progress", (event) => handler(event.payload)),
};

/** Runs `fn` only after `ms` milliseconds without new calls (for sliders and color pickers). */
export function debounce<A extends unknown[]>(
  fn: (...args: A) => void,
  ms: number,
) {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const call = (...args: A) => {
    clearTimeout(timer);
    timer = setTimeout(() => fn(...args), ms);
  };
  /** Cancels the pending call (e.g. when the component is destroyed). */
  call.cancel = () => clearTimeout(timer);
  return call;
}

/** The error as the backend or the system said it, without any rewriting. */
export function rawErrorText(error: unknown): string {
  return typeof error === "string"
    ? error
    : error instanceof Error
      ? error.message
      : JSON.stringify(error);
}

/** Causes people meet often, and what to do about them. The first match wins; a message that
 * matches none is shown as it came. The raw text stays in brackets for reports and for the log. */
const KNOWN_ERRORS: [RegExp, string][] = [
  [/read-only mode is on/i, "Read-only mode is on, so nothing was sent to the device. Turn it off first if you want to change it."],
  [/permission denied|access is denied|access denied/i, "PeriHub is not allowed to open this device. On Linux the udev rule is usually missing (see Settings); on Windows, close the maker's own software and try again."],
  [/device or resource busy|resource busy|already in use|sharing violation/i, "Another program is using this device. Quit the maker's own software (G HUB, iCUE, Synapse...) and try again."],
  [/turned off or asleep/i, "The device is turned off or asleep. Wake it by moving it, then try again."],
  [/did not respond in time|timed out|timeout/i, "The device did not answer in time. If it is wireless, wake it by moving it, then try again."],
  [/device not connected|no such device|not connected/i, "The device is not connected any more. Plug it back in and try again."],
  [/different device answers/i, "A different device now answers in this receiver slot. Wait a moment, PeriHub looks again by itself."],
  [/device busy/i, "The device is busy right now. Try again in a moment."],
];

/** An error from the backend as a sentence a person can act on. */
export function errorText(error: unknown): string {
  const raw = rawErrorText(error);
  // PeriHub's own advice (the udev rule) says more than the general one.
  if (/udev rule/i.test(raw)) return raw;
  const known = KNOWN_ERRORS.find(([pattern]) => pattern.test(raw));
  if (!known) return raw;
  // The raw text is not repeated when it says the same (e.g. "Read-only mode is on").
  return raw.length < 30 && known[1].toLowerCase().startsWith(raw.toLowerCase()) ? known[1] : `${known[1]} (${raw})`;
}
