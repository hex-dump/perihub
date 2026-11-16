// What the screenshots show: PeriHub's interface with a SteelSeries Apex 3 TKL and a Logitech G502 HERO,
// answered here instead of by the Rust side. The mouse profiles, key catalogue and key map come from
// data.json (cargo xtask gen); the rest is written to look like the real devices.
import type {
  AppSettings,
  DeviceStatus,
  KeyCatalog,
  Keymap,
  KeyboardConfig,
  MouseInfo,
  MouseLive,
  MouseProfile,
  MouseState,
  SoftMacrosInfo,
} from "../src/lib/api";
import data from "./data.json";

const mouse: MouseInfo = {
  product_id: 0xc08b,
  model: "G502 HERO",
  name: "G502 HERO Gaming Mouse",
  verified: true,
  experimental: false,
  pending_undo: false,
  receiver: false,
  link: null,
  host_mode: false,
  firmware: [{ kind: 0, name: "U1", version: "27.3.0" }],
  dpi: { min: 100, max: 25600, step: 50, list: [] },
  report_rates_ms: [1, 2, 4, 8],
  led_zones: [
    { index: 0, location: 1, effects: [0, 1, 3, 10] },
    { index: 1, location: 2, effects: [0, 1, 3, 10] },
  ],
  features: [],
  onboard: {
    memory_model: 1,
    profile_format: 2,
    macro_format: 1,
    profile_count: 5,
    rom_profile_count: 5,
    button_count: 11,
    sector_count: 16,
    sector_size: 256,
    has_gshift: true,
    has_dpi_shift: true,
  },
  writable: true,
  writable_format: true,
  switches_back: true,
  profile_list: "own",
};

const status: DeviceStatus = {
  keyboard: { name: "SteelSeries Apex 3 TKL", firmware: "3.0.4" },
  mouse,
  keyboard_error: null,
  mouse_error: null,
  mouse_asleep: null,
  unsupported_keyboard: null,
  conflicts: [],
  corsair: [],
  g300: null,
  g300_error: null,
  corsair_error: null,
  peripherals: [],
  peripheral_error: null,
  qmk: [],
  receivers: [],
  logitech_programs: [],
  sandboxed: false,
};

const profiles = data.mouse_profiles as unknown as MouseProfile[];
const first = profiles[0].settings;
const live: MouseLive = {
  active_profile: 1,
  active_dpi_index: first.default_dpi_index,
  current_dpi: first.dpi_levels[first.default_dpi_index],
  battery: null,
};
const mouseState: MouseState = {
  active_profile: 1,
  active_dpi_index: live.active_dpi_index,
  current_dpi: live.current_dpi,
  profiles,
  macro_space: 2032,
  profile_list: "own",
};

const keyboard: KeyboardConfig = { lighting: { type: "wave", period_ms: 4000, reverse: false }, brightness: 16 };

const macros: SoftMacrosInfo = {
  macros: [],
  status: { available: true, error: null, running: [], keyboard_read: false, keyboard_error: null },
  free_keys: [],
  rule_installed: true,
  rule_supported: true,
};

const settings: AppSettings = {
  start_hidden: false,
  autostart: true,
  check_updates: true,
  battery: { enabled: true, low_percent: 15, full: false },
  updates_supported: false,
  package_manager: null,
  log_dir: "~/.local/state/perihub",
  config_dir: "~/.config/perihub",
  system: "linux x86_64",
};

/** Answers by command name. A command that is not here never answers, as in the tests: the page waits. */
export const answers: Record<string, unknown> = {
  get_status: status,
  devices_for_report: [],
  kb_get_config: keyboard,
  kb_read_brightness: keyboard.brightness,
  kb_get_keymap: data.keymap as unknown as Keymap,
  kb_key_catalog: data.key_catalog as unknown as KeyCatalog,
  kb_backups: [],
  kb_get_presets: [],
  mouse_get_state: mouseState,
  mouse_live: live,
  mouse_backups: [],
  soft_get: macros,
  app_get_settings: settings,
  update_available: null,
  update_just_installed: null,
};
