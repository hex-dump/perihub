import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

// ---- Types shared with the Rust backend (same field names as serde) ----

export type Rgb = string; // "#rrggbb"

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

export type FocusSupport = { kind: "supported"; desktop: string } | { kind: "unsupported"; reason: string };

export interface ProfilesInfo {
  support: FocusSupport;
  focused: string | null;
  /** Apps focused recently, most recent first. */
  recent: string[];
  profiles: AppProfiles;
  /** Names of the saved lightings. */
  presets: string[];
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
export type SoftStep = MacroStep | { type: "text"; text: string } | { type: "open"; target: string };

/** once; repeat a number of times; toggle: again and again until the button is pressed again. */
export type SoftMode = { type: "once" } | { type: "repeat"; times: number } | { type: "toggle" };

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
  led_zones: LedZone[];
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
  /** Connected Logitech receivers (LIGHTSPEED, Logi Bolt and Unifying). */
  receivers: ReceiverReport[];
  /** Running programs that set Logitech devices' settings themselves, such as "Solaar". */
  logitech_programs: string[];
  /** PeriHub runs as a Flatpak and cannot see the other programs running. */
  sandboxed: boolean;
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
  /** Its HID++ feature table: kind holds the flags (0x80 obsolete, 0x40 hidden, 0x20 engineering). */
  features: { id: number; kind: number; version: number }[];
  /** The DPI it accepts (0x2201), if it has that feature. */
  dpi: { min: number; max: number; step: number; list: number[] } | null;
  /** The polling rates it offers, in milliseconds (0x8060). */
  report_rates_ms: number[];
  /** Its onboard profile memory (0x8100), if it has one. */
  onboard: MouseInfo["onboard"] | null;
  read_errors: string[];
}

/** A connected Logitech receiver, for the test report that helps support what is paired with it. */
/** What changes while a device PeriHub knows by its features is in use. */
export interface LogitechLive {
  battery: Battery | null;
  /** The DPI it uses now, if it has adjustable DPI. */
  dpi: number | null;
  /** The DPI PeriHub sets every time the device connects, if one was chosen. */
  saved_dpi: number | null;
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
}

export type G300Button =
  | { type: "none" }
  | { type: "mouse"; button: number }
  | {
      type: "dpi_up" | "dpi_down" | "dpi_cycle" | "profile_cycle" | "dpi_shift" | "dpi_default";
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
  leds: { name: string; modes: [number, string][]; brightness_step: number | null; color: boolean }[];
}

/** A setting of a device that cannot report it: PeriHub sends it, one at a time. */
export interface BlindSetting {
  id: string;
  label: string;
  kind:
    | { type: "choice"; values: string[] }
    | { type: "color" }
    | { type: "range"; min: number; max: number; step: number }
    | { type: "dpi"; min: number; max: number; step: number; max_presets: number };
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
  blind: BlindSetting[];
  /** Changes may be tried: PeriHub's app, and everything read looked as documented. */
  editable: boolean;
  /** The device can store its settings in its memory. */
  can_save: boolean;
  /** Something did not look as documented: nothing is changed on it. */
  problem: string | null;
  /** A change was neither kept nor undone, and could not be undone on this connection. */
  pending_undo: boolean;
  /** Requests and answers of the last reading, for test reports. */
  raw: string[];
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
}

export interface AppSettings {
  start_hidden: boolean;
  autostart: boolean;
  /** Look for a new version on GitHub at startup and once a day. */
  check_updates: boolean;
  /** This build can update itself (Windows). */
  updates_supported: boolean;
  /** The package manager that installed PeriHub and updates it, such as Scoop. */
  package_manager: string | null;
  /** Folder of the log file. */
  log_dir: string;
  /** Operating system and architecture, for test reports. */
  system: string;
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
  onStatus: (handler: (status: DeviceStatus) => void): Promise<UnlistenFn> =>
    listen<DeviceStatus>("devices-changed", (event) => handler(event.payload)),

  keyboardConfig: () => invoke<KeyboardConfig>("kb_get_config"),
  setLighting: (lighting: KeyboardLighting) => invoke<void>("kb_set_lighting", { lighting }),
  setBrightness: (level: number) => invoke<void>("kb_set_brightness", { level }),
  readBrightness: () => invoke<number>("kb_read_brightness"),
  keymap: () => invoke<Keymap>("kb_get_keymap"),
  applyKeymap: (changes: KeyChange[]) => invoke<Keymap>("kb_apply_keymap", { changes }),
  resetKeymap: () => invoke<Keymap>("kb_reset_keymap"),
  keyboardBackups: () => invoke<BackupEntry[]>("kb_backups"),
  restoreKeyboardBackup: (id: string) => invoke<KeymapRestore>("kb_restore_backup", { id }),
  keyCatalog: () => invoke<KeyCatalog>("kb_key_catalog"),
  keyAccess: () => invoke<KeyAccess>("kb_key_access"),
  grantKeyAccess: () => invoke<void>("kb_grant_key_access"),
  revokeKeyAccess: () => invoke<void>("kb_revoke_key_access"),
  /** Linux: the optional rule that opens the Unifying receivers, installed through the password window. */
  unifyingAccess: () => invoke<UnifyingAccess>("unifying_access"),
  grantUnifyingAccess: () => invoke<void>("unifying_grant_access"),
  revokeUnifyingAccess: () => invoke<void>("unifying_revoke_access"),
  logitechLive: (receiverId: number, slot: number) => invoke<LogitechLive>("logitech_live", { receiverId, slot }),
  /** Experimental: sets the DPI in the device's RAM, and again every time it connects. */
  logitechSetDpi: (receiverId: number, slot: number, dpi: number) =>
    invoke<void>("logitech_set_dpi", { receiverId, slot, dpi }),
  logitechResetDpi: (receiverId: number, slot: number) => invoke<void>("logitech_reset_dpi", { receiverId, slot }),
  presets: () => invoke<LightingPreset[]>("kb_get_presets"),
  savePreset: (name: string, lighting: KeyboardLighting) =>
    invoke<LightingPreset[]>("kb_save_preset", { name, lighting }),
  deletePreset: (name: string) => invoke<LightingPreset[]>("kb_delete_preset", { name }),

  mouseState: () => invoke<MouseState>("mouse_get_state"),
  mouseLive: () => invoke<MouseLive>("mouse_live"),
  saveProfile: (number: number, settings: ProfileSettings) =>
    invoke<MouseState>("mouse_save_profile", { number, settings }),
  restoreFactory: (number: number) => invoke<MouseState>("mouse_restore_factory", { number }),
  mouseBackups: () => invoke<BackupEntry[]>("mouse_backups"),
  restoreMouseBackup: (id: string) => invoke<MouseRestore>("mouse_restore_backup", { id }),
  setProfileEnabled: (number: number, enabled: boolean) =>
    invoke<MouseState>("mouse_set_profile_enabled", { number, enabled }),
  setActiveProfile: (number: number) => invoke<MouseState>("mouse_set_active_profile", { number }),
  previewDpi: (dpi: number) => invoke<void>("mouse_preview_dpi", { dpi }),
  previewLed: (zone: number, effect: LedEffect) => invoke<void>("mouse_preview_led", { zone, effect }),
  revertPreview: () => invoke<void>("mouse_revert_preview"),
  g300Info: () => invoke<G300Info>("g300_info"),
  /** Saves a G300 profile: undone unless kept. */
  g300Save: (number: number, settings: G300Settings) => invoke<G300Info>("g300_save", { number, settings }),
  g300KeepChanges: () => invoke<void>("g300_keep_changes"),
  g300UndoLastWrite: () => invoke<G300Info>("g300_undo_last_write"),
  /** A device of another brand, read again now. */
  peripheralInfo: (key: string) => invoke<PeripheralInfo>("peripheral_info", { key }),
  /** Experimental change, also saved to the device's memory if `save`: undone unless kept. */
  /** `profile`: the active profile the settings were edited on; refused if the device switched since. */
  peripheralChange: (key: string, settings: PeripheralSettings, save: boolean, profile: number | null) =>
    invoke<PeripheralInfo>("peripheral_change", { key, settings, save, profile }),
  peripheralKeepChanges: (key: string) => invoke<void>("peripheral_keep_changes", { key }),
  peripheralUndo: (key: string) => invoke<PeripheralInfo>("peripheral_undo", { key }),
  /** Sends one setting to a device that cannot report its settings: not saved, not undone. */
  peripheralSetBlind: (key: string, id: string, value: BlindValue) =>
    invoke<PeripheralInfo>("peripheral_set_blind", { key, id, value }),
  /** Experimental lighting on a Corsair mouse: one colour on every LED, for this session. */
  corsairSetLighting: (productId: number, color: string) =>
    invoke<CorsairInfo>("corsair_set_lighting", { productId, color }),
  corsairLightingOff: (productId: number) => invoke<void>("corsair_lighting_off", { productId }),
  /** The mouse still works: the core stops counting down to give it back. */
  corsairKeepLighting: (productId: number) => invoke<void>("corsair_keep_lighting", { productId }),
  /** Keeps an experimental save: the core stops counting down to undo it. */
  keepChanges: () => invoke<void>("mouse_keep_changes"),
  /** Puts back the mouse memory saved before the last write. */
  undoLastWrite: () => invoke<MouseRestore>("mouse_undo_last_write"),

  profiles: () => invoke<ProfilesInfo>("profiles_get"),
  setProfiles: (profiles: AppProfiles) => invoke<void>("profiles_set", { profiles }),
  onFocus: (handler: (app: string | null) => void): Promise<UnlistenFn> =>
    listen<string | null>("focus-changed", (event) => handler(event.payload)),

  appSettings: () => invoke<AppSettings>("app_get_settings"),
  setAppSettings: (settings: AppSettings) => invoke<AppSettings>("app_set_settings", { settings }),
  quit: () => invoke<void>("app_quit"),
  openLogFolder: () => invoke<void>("app_open_log_folder"),
  openTestForm: (url: string) => invoke<void>("app_open_test_form", { url }),
  softMacros: () => invoke<SoftMacrosInfo>("soft_get"),
  /** Saves a macro run by PeriHub; a new one (key 0) gets its trigger key. */
  saveSoftMacro: (item: SoftwareMacro) => invoke<SoftwareMacro>("soft_save", { item }),
  deleteSoftMacro: (key: number) => invoke<void>("soft_delete", { key }),
  stopSoftMacros: () => invoke<void>("soft_stop_all"),
  grantSoftAccess: () => invoke<void>("soft_grant_access"),
  revokeSoftAccess: () => invoke<void>("soft_revoke_access"),
  /** Saves a macro to a file chosen in the system's dialog; false if cancelled. */
  exportMacro: (content: Macro, name: string) => invoke<boolean>("macro_export", { content, name }),
  /** Reads a macro from a file chosen in the system's dialog; null if cancelled. */
  importMacro: () => invoke<Macro | null>("macro_import"),
  checkUpdate: () => invoke<UpdateInfo | null>("update_check"),
  /** Result of the last check made in the background, without asking GitHub again. */
  updateAvailable: () => invoke<UpdateInfo | null>("update_available"),
  onUpdate: (handler: (update: UpdateInfo) => void): Promise<UnlistenFn> =>
    listen<UpdateInfo>("update-available", (event) => handler(event.payload)),
  installUpdate: () => invoke<void>("update_install"),
};

/** Runs `fn` only after `ms` milliseconds without new calls (for sliders and color pickers). */
export function debounce<A extends unknown[]>(fn: (...args: A) => void, ms: number) {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const call = (...args: A) => {
    clearTimeout(timer);
    timer = setTimeout(() => fn(...args), ms);
  };
  /** Cancels the pending call (e.g. when the component is destroyed). */
  call.cancel = () => clearTimeout(timer);
  return call;
}

export function errorText(error: unknown): string {
  return typeof error === "string" ? error : error instanceof Error ? error.message : JSON.stringify(error);
}
