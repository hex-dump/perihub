import type { CorsairInfo, G300Info, G300Profile } from "./api";
import type { MouseInfo } from "./api";

const LOGITECH = "046d";
const TEST_FORM = "https://github.com/hex-dump/perihub/issues/new?template=mouse_test.yml";

/** The mouse's profile list when it was connected. */
const PROFILE_LIST = {
  own: "saved in the mouse",
  factory: "none when connected (factory profiles in use)",
  damaged: "damaged when connected (checksum does not match)",
};

const hex = (id: number) => id.toString(16).padStart(4, "0");

function usbId(mouse: MouseInfo): string {
  return `${LOGITECH}:${hex(mouse.link?.receiver_id ?? mouse.product_id)}`;
}

function connection(mouse: MouseInfo): string {
  if (!mouse.receiver) return "USB cable";
  if (!mouse.link) return "LIGHTSPEED receiver";
  const { receiver_id, slot, wireless_id } = mouse.link;
  return `LIGHTSPEED receiver ${LOGITECH}:${hex(receiver_id)}, slot ${slot}, wireless id ${hex(wireless_id)}`;
}

function dpiRange({ dpi }: MouseInfo): string {
  return dpi.list.length ? dpi.list.join(", ") : `${dpi.min}-${dpi.max}, step ${dpi.step}`;
}

/** What a tester sends about a mouse: never key presses, macros, apps or log lines. */
export function testReport(mouse: MouseInfo, version: string, system: string): string {
  const { onboard } = mouse;
  const zones = mouse.led_zones.map((z) => `zone ${z.index} at ${z.location}, effects ${z.effects.join(" ")}`);
  return [
    `PeriHub ${version} on ${system}`,
    `Mouse: ${mouse.model} (reports itself as "${mouse.name}")`,
    `USB ID: ${usbId(mouse)}`,
    `Connection: ${connection(mouse)}`,
    `Support: ${mouse.verified ? "tested" : mouse.experimental ? "experimental editing on" : "read-only"}`,
    `Software control: ${mouse.host_mode ? "yes" : "no"}`,
    `Firmware: ${mouse.firmware.map((f) => `${f.name} ${f.version}`).join(", ")}`,
    `Profiles: ${onboard.profile_count}, buttons: ${onboard.button_count}`,
    `Memory: model ${onboard.memory_model}, profile format ${onboard.profile_format}, macro format ${onboard.macro_format}, ${onboard.sector_count} sectors of ${onboard.sector_size} bytes, ${onboard.rom_profile_count} factory profiles`,
    `Profile list: ${PROFILE_LIST[mouse.profile_list]}`,
    `G-Shift: ${onboard.has_gshift ? "yes" : "no"}, DPI shift: ${onboard.has_dpi_shift ? "yes" : "no"}`,
    `DPI: ${dpiRange(mouse)}`,
    `Polling (ms): ${mouse.report_rates_ms.join(", ") || "none reported"}`,
    `LEDs: ${zones.join("; ") || "none reported"}`,
  ].join("\n");
}

/** The test form on GitHub with the fields filled in that PeriHub knows. */
export function testFormUrl(mouse: MouseInfo, version: string, system: string): string {
  const fields = {
    title: `${mouse.model} test report`,
    model: mouse.model,
    "usb-id": usbId(mouse),
    connection: mouse.receiver ? "LIGHTSPEED receiver" : "USB cable",
    version,
    os: system.toLowerCase().startsWith("windows") ? "Windows" : "Linux",
    report: testReport(mouse, version, system),
  };
  const query = Object.entries(fields).map(([name, value]) => `${name}=${encodeURIComponent(value)}`);
  return `${TEST_FORM}&${query.join("&")}`;
}

const DEVICE_TEST_FORM = "https://github.com/hex-dump/perihub/issues/new?template=device_test.yml";

/** The fields of the device test form (Corsair devices and the G300). */
export interface DeviceTestFields {
  model: string;
  usbId: string;
  connection: string;
  report: string;
}

export function deviceTestFormUrl(fields: DeviceTestFields, version: string, system: string): string {
  const values = {
    title: `${fields.model} test report`,
    model: fields.model,
    "usb-id": fields.usbId,
    connection: fields.connection,
    version,
    os: system.toLowerCase().startsWith("windows") ? "Windows" : "Linux",
    report: fields.report,
  };
  const query = Object.entries(values).map(([name, value]) => `${name}=${encodeURIComponent(value)}`);
  return `${DEVICE_TEST_FORM}&${query.join("&")}`;
}

/** What a tester sends about a Corsair device. */
export function corsairTest(info: CorsairInfo, version: string, system: string): DeviceTestFields {
  const connection = info.receiver ? `wireless receiver 1b1c:${hex(info.receiver)}` : "USB cable";
  return {
    model: `Corsair ${info.model}`,
    usbId: `1b1c:${hex(info.product_id)}`,
    connection: info.receiver ? "wireless receiver" : "USB cable",
    report: [
      `PeriHub ${version} on ${system}`,
      `Device: Corsair ${info.model} (${info.kind}), USB ID 1b1c:${hex(info.product_id)}`,
      `Connection: ${connection}`,
      `Firmware: ${info.firmware ?? "not read"}`,
      `Mode at connection: ${info.mode ?? "not read"}`,
      `Polling: ${info.polling_hz ?? "not read"} Hz${info.kind === "mouse" ? `, DPI: ${info.dpi ?? "not read"}` : ""}`,
      `LEDs: ${info.leds}`,
      `Could not read: ${info.read_errors.join("; ") || "nothing"}`,
      `Experimental lighting: ${info.lighting ? "on" : "off"}`,
    ].join("\n"),
  };
}

/** What a tester sends about the G300, with the bytes of every profile. */
export function g300Test(
  info: G300Info,
  describe: (profile: G300Profile) => string,
  version: string,
  system: string,
): DeviceTestFields {
  return {
    model: "Logitech G300",
    usbId: "046d:c246",
    connection: "USB cable",
    report: [
      `PeriHub ${version} on ${system}`,
      "Mouse: Logitech G300, USB ID 046d:c246",
      `Active profile: ${info.active_profile}, DPI level ${info.active_level + 1} (${info.active_raw})`,
      `Experimental editing: ${info.experimental ? "on" : "off"}`,
      ...info.profiles.map((p) => `Profile ${p.number}: ${describe(p)}\n  ${p.raw}`),
    ].join("\n"),
  };
}
