import type {
  CorsairInfo, G300Info, G300Profile, LogitechLive, PeripheralInfo, ReceiverReport, UnsupportedReceiverDevice,
} from "./api";
import type { HidppFeature, MouseInfo, ReportDevice } from "./api";
import { kindName } from "./logitechFeatures";
import { layoutReport } from "./keyboardLayout.svelte";

const LOGITECH = "046d";
const TEST_FORM = "https://github.com/hex-dump/perihub/issues/new?template=01_test_report.yml";

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

/** What a Logitech receiver is called, from its USB product id (as the core names it). */
export function receiverName(productId: number | undefined): string {
  if (productId === 0xc548) return "Logi Bolt receiver";
  if (productId === 0xc52b || productId === 0xc532) return "Unifying receiver";
  return "LIGHTSPEED receiver";
}

function connection(mouse: MouseInfo): string {
  if (!mouse.receiver) return "USB cable";
  if (!mouse.link) return receiverName(undefined);
  const { receiver_id, slot, wireless_id } = mouse.link;
  return `${receiverName(receiver_id)} ${LOGITECH}:${hex(receiver_id)}, slot ${slot}, wireless id ${hex(wireless_id)}`;
}

function support(mouse: MouseInfo): string {
  if (mouse.experimental) return "experimental (memory format not tested yet)";
  if (mouse.verified) return mouse.writable ? "tested" : "tested, no saves on this connection";
  return mouse.writable_format ? "read-only on this connection" : "read-only (memory format PeriHub cannot write)";
}

/** A HID++ feature table as one line: "0001 v1, 2201 v2 (flags 40)". */
function featureList(features: HidppFeature[]): string {
  return features
    .map((f) => `${hex(f.id)} v${f.version}${f.kind ? ` (flags ${f.kind.toString(16).padStart(2, "0")})` : ""}`)
    .join(", ");
}

type DpiRanges = { min: number; max: number; step: number; list: number[] };

/** "100-25600, step 50", the values of a short list, or the span of a long one. */
function rangeText(dpi: DpiRanges): string {
  if (dpi.list.length > 12) return `${dpi.min}-${dpi.max}, ${dpi.list.length} values`;
  return dpi.list.length ? dpi.list.join(", ") : `${dpi.min}-${dpi.max}, step ${dpi.step}`;
}

function dpiRange({ dpi }: MouseInfo): string {
  return rangeText(dpi);
}

const hzList = (rates: number[]) => (rates.length ? rates.join(" ") : "none");

/** The lines for what a PRO X 2 style mouse reports through its extended features, short enough for the form's link. */
function extendedLines(mouse: MouseInfo): string[] {
  const e = mouse.extended;
  if (!e) return [];
  const lines: string[] = [];
  if (e.dpi_y) lines.push(`DPI Y axis: ${rangeText(e.dpi_y)}`);
  if (e.dpi_now) {
    const { x, default_x, y, default_y, lod } = e.dpi_now;
    const axes = y === null ? `${x} (default ${default_x})` : `X ${x} (default ${default_x}), Y ${y} (default ${default_y})`;
    lines.push(`DPI now: ${axes}${lod === null ? "" : `, lift-off distance ${lod}`}`);
  }
  if (e.rates_wired_hz.length || e.rates_wireless_hz.length || e.rate_now_hz !== null) {
    lines.push(
      `Polling (Hz): cable ${hzList(e.rates_wired_hz)}; receiver ${hzList(e.rates_wireless_hz)}; in use ${e.rate_now_hz ?? "not read"}`,
    );
  }
  if (e.raw.length) lines.push(`Polling answers: ${e.raw.join("; ")}`);
  lines.push(...e.profiles);
  if (e.errors.length) lines.push(`Could not read: ${e.errors.join("; ")}`);
  return lines;
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
    `Support: ${support(mouse)}`,
    `Software control: ${mouse.host_mode ? "yes" : "no"}`,
    `Firmware: ${mouse.firmware.map((f) => `${f.name} ${f.version}`).join(", ")}`,
    `Profiles: ${onboard.profile_count}, buttons: ${onboard.button_count}`,
    `Memory: model ${onboard.memory_model}, profile format ${onboard.profile_format}, macro format ${onboard.macro_format}, ${onboard.sector_count} sectors of ${onboard.sector_size} bytes, ${onboard.rom_profile_count} factory profiles`,
    `Profile list: ${PROFILE_LIST[mouse.profile_list]}`,
    `G-Shift: ${onboard.has_gshift ? "yes" : "no"}, DPI shift: ${onboard.has_dpi_shift ? "yes" : "no"}`,
    `DPI: ${dpiRange(mouse)}`,
    ...(mouse.report_rates_ms.length || !mouse.extended?.rates_wireless_hz.length
      ? [`Polling (ms): ${mouse.report_rates_ms.join(", ") || "none reported"}`]
      : []),
    ...extendedLines(mouse),
    `LEDs: ${zones.join("; ") || "none reported"}`,
    `Features: ${featureList(mouse.features) || "not read"}`,
  ].join("\n");
}

/** The test form on GitHub with the fields filled in that PeriHub knows. */
export function testFormUrl(
  mouse: MouseInfo,
  version: string,
  system: string,
  report = testReport(mouse, version, system),
): string {
  const fields = {
    title: `${mouse.model} test report`,
    "device-type": "Mouse",
    brand: "Logitech",
    model: mouse.model,
    "usb-id": usbId(mouse),
    connection: mouse.receiver ? receiverName(mouse.link?.receiver_id) : "USB cable",
    version,
    os: system.toLowerCase().startsWith("windows") ? "Windows" : "Linux",
    report,
  };
  const query = Object.entries(fields).map(([name, value]) => `${name}=${encodeURIComponent(value)}`);
  return `${TEST_FORM}&${query.join("&")}`;
}

const DEVICE_TEST_FORM = "https://github.com/hex-dump/perihub/issues/new?template=01_test_report.yml";

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

/** What a tester sends about a Logitech receiver and a device paired with it that PeriHub does not
 * support yet (`null`: none answered). */
export function receiverTest(
  receiver: ReceiverReport,
  device: UnsupportedReceiverDevice | null,
  version: string,
  system: string,
  live: LogitechLive | null = null,
): DeviceTestFields {
  const id = `${LOGITECH}:${hex(receiver.product_id)}`;
  const lines = [
    `PeriHub ${version} on ${system}`,
    `Receiver: ${receiver.name}, USB ID ${id}`,
    `HID collections: ${receiver.collections.join("; ") || "none"}`,
    `PeriHub reaches its paired devices: ${receiver.denied ? "no (the system refused to open it)" : receiver.reachable ? "yes" : "no"}`,
    `Slots: ${receiver.slots.join("; ") || "not asked"}`,
  ];
  if (device) {
    const dpi = device.dpi
      ? rangeText(device.dpi)
      : device.dpi_extended
        ? `${rangeText(device.dpi_extended)} (0x2202)`
        : "no 0x2201";
    const onboard = device.onboard
      ? `profile format ${device.onboard.profile_format}, ${device.onboard.sector_count} sectors of ${device.onboard.sector_size} bytes`
      : "none";
    lines.push(
      `Slot ${device.slot}: ${device.name ?? "name not read"}, HID++ ${device.protocol}`,
      `Kind: ${kindName(device.kind)}${device.kind === null ? "" : ` (${device.kind})`}`,
      `Device info (0x0003): ${device.device_info ?? "not read"}`,
      `Firmware: ${device.firmware.join(", ") || "not read"}`,
      `Features: ${featureList(device.features) || "not read"}`,
      `DPI: ${dpi}`,
      `Polling (ms): ${device.report_rates_ms.join(", ") || "none reported"}`,
    );
    if (device.report_rates_hz?.length) lines.push(`Polling (Hz, 0x8061): ${device.report_rates_hz.join(" ")}`);
    lines.push(`Onboard memory: ${onboard}`);
    if (device.kind === 0 || device.layout) lines.push(layoutReport(null, device.layout));
    if (live) {
      lines.push(
        `DPI now: ${live.dpi ?? "not read"}`,
        `DPI set by PeriHub at every connection: ${live.saved_dpi ?? "none"}`,
      );
    }
    lines.push(`Could not read: ${device.read_errors.join("; ") || "nothing"}`);
  } else {
    lines.push("Paired device: none answered");
  }
  return {
    model: device?.name ? `Logitech ${device.name}` : receiver.name,
    usbId: id,
    connection: receiver.name,
    report: lines.join("\n"),
  };
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
      info.kind === "keyboard" ? layoutReport(`1b1c:${hex(info.product_id)}`, info.layout) : null,
      `Could not read: ${info.read_errors.join("; ") || "nothing"}`,
      `Experimental lighting: ${info.lighting ? "on" : "off"}`,
    ]
      .filter((line): line is string => line !== null)
      .join("\n"),
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
      `Saving: ${info.editable ? "available" : "off (read-only)"}`,
      ...info.profiles.map((p) => `Profile ${p.number}: ${describe(p)}\n  ${p.raw}`),
    ].join("\n"),
  };
}

/** What a tester sends about a device of another brand, with every request and answer of the last
 * reading. */
export function peripheralTest(info: PeripheralInfo, version: string, system: string): DeviceTestFields {
  const usbId = info.key;
  const set = info.settings;
  const options = info.limits.options
    .map((c) => `${c.label}: ${c.values.find(([v]) => v === set.options[c.id])?.[1] ?? set.options[c.id] ?? "not read"}`)
    .join(", ");
  return {
    model: `${info.brand} ${info.model}`,
    usbId,
    connection: "",
    report: [
      `PeriHub ${version} on ${system}`,
      `Device: ${info.brand} ${info.model} (${info.kind}), USB ID ${usbId}, protocol from ${info.source}`,
      `Firmware: ${info.firmware ?? "not read"}`,
      info.battery ? `Battery: ${info.battery.level} %${info.battery.charging ? ", charging" : ""}` : null,
      info.readable ? null : "Settings: this device cannot be read back; PeriHub sends them one at a time",
      ...info.blind
        .filter((b) => b.last)
        .map((b) => `Sent: ${b.label} = ${b.last!.type === "dpi" ? b.last!.value.join(", ") : b.last!.value}`),
      info.kind === "mouse"
        ? `Active profile: ${info.active_profile ?? "-"} of ${info.profiles}, DPI level ${info.active_dpi ?? "-"}`
        : null,
      info.kind === "mouse"
        ? `DPI levels: ${set.dpi_levels.join(" / ") || "-"}, polling: ${set.polling_hz ?? "not read"} Hz`
        : null,
      options ? `Other settings: ${options}` : null,
      ...set.leds.map((l, i) => {
        const limits = info.limits.leds[i];
        return limits && limits.modes.length === 0 && !limits.color
          ? `${limits.name}: brightness ${l.brightness ?? "-"} %`
          : `LED ${i + 1}: mode ${l.mode}, brightness ${l.brightness ?? "-"}, ${l.color}`;
      }),
      ...(info.details ?? []).map(([label, value]) => `${label}: ${value}`),
      info.kind === "keyboard" ? layoutReport(info.key, info.layout) : null,
      ...info.buttons.map((b) => `Button ${b.name}: ${b.action}`),
      `Changes: ${info.problem ? `off (${info.problem})` : info.editable ? "available (experimental)" : "off (read-only)"}`,
      "Requests and answers:",
      ...info.raw.map((line) => `  ${line}`),
    ]
      .filter((line): line is string => line !== null)
      .join("\n"),
  };
}

const DEVICE_REQUEST_FORM = "https://github.com/hex-dump/perihub/issues/new?template=device_request.yml";

/** A device's name as it gives it, with its maker unless the name already starts with it. */
export function reportDeviceName(device: ReportDevice): string {
  const product = device.product ?? "Unnamed device";
  const maker = device.manufacturer;
  return maker && !product.toLowerCase().startsWith(maker.toLowerCase().split(" ")[0]) ? `${maker} ${product}` : product;
}

export const reportUsbId = (device: ReportDevice) => `${hex(device.vendor_id)}:${hex(device.product_id)}`;

/** The device request form, filled in with what PeriHub sees of `device`: no serial number, no path. */
export function deviceRequestUrl(device: ReportDevice, version: string, system: string): string {
  const name = reportDeviceName(device);
  const collections = device.collections.map(
    (c) =>
      `  ${c.interface < 0 ? "no interface number" : `interface ${c.interface}`}, usage page ${hex(c.usage_page)}, usage ${hex(c.usage)}`,
  );
  const details = [
    `PeriHub ${version} on ${system}`,
    `Device: ${name}`,
    `USB ID: ${reportUsbId(device)} (${device.bus})`,
    "HID collections:",
    ...collections,
  ].join("\n");
  const values = { title: `Device request: ${name}`, model: name, "usb-id": reportUsbId(device), details };
  const query = Object.entries(values).map(([field, value]) => `${field}=${encodeURIComponent(value)}`);
  return `${DEVICE_REQUEST_FORM}&${query.join("&")}`;
}
