import { MAX_LINK, fitsGithubLink } from "./githubLink";
import type {
  CorsairInfo, G300Info, G300Profile, LogitechLive, PeripheralInfo, ReceiverReport, UnsupportedReceiverDevice,
} from "./api";
import type {
  Battery, DescriptorCollection, DescriptorReports, HidppFeature, HidppLighting, InterfaceFeatures, LedZone, LightsLook, MouseInfo,
  ReportDevice,
} from "./api";
import { blindText } from "./blind";
import { effectName, kindName, zoneName } from "./logitechFeatures";
import { layoutReport } from "./keyboardLayout.svelte";

const LOGITECH = "046d";
const TEST_FORM = "https://github.com/hex-dump/perihub/issues/new?template=01_test_report.yml";

/** "4-7,9,12" for [4, 5, 6, 7, 9, 12]: short enough for the test form's address. */
function ranges(ids: number[]): string {
  const out: string[] = [];
  for (let i = 0; i < ids.length; i++) {
    let j = i;
    while (j + 1 < ids.length && ids[j + 1] === ids[j] + 1) j++;
    out.push(j > i ? `${ids[i]}-${ids[j]}` : `${ids[i]}`);
    i = j;
  }
  return out.join(",");
}

/** One lighting zone of a test report: "zone 0 at 1, effects 0 1 3". A zone of 0x8071 says so, and a zone whose
 * profile LED slot is not its own number says which slot it is (or that it has none). */
export function ledZoneText(z: LedZone): string {
  const via = z.feature === 0x8071 ? " (8071)" : "";
  const slot = z.slot === z.index ? "" : z.slot === null ? ", no profile slot" : `, profile slot ${z.slot}`;
  return `zone ${z.index} at ${z.location}${via}, effects ${z.effects.join(" ")}${slot}`;
}

/** What a device's lighting features told, in a few lines, with every reply as it came: the
 * replies tell the meaning of the bytes that is not documented yet (issue #30). */
function lightingLines(lighting: HidppLighting, kind: number | null): string[] {
  const b = lighting.brightness;
  const e = lighting.effects;
  const power = (name: string, p: NonNullable<HidppLighting["effects"]>["startup"]) =>
    p ? `${name} ${p.enabled ? "on" : "off"} (${p.colors.join(" ")})` : `${name} not read`;
  // Names only: the ids, settings and periods are in the replies below.
  const zones = e?.zones.map(
    (z) => `${zoneName(z.location, kind)} (location ${z.location}): ${z.effects.map((f) => effectName(f.id)).join(", ") || "no effects"}`,
  );
  return [
    `Brightness: ${b ? `${b.min}-${b.max}, ${b.steps} levels${b.on_off ? ", can be turned off" : ""}` : "no 0x8040"}`,
    `Lighting zones: ${zones?.join("; ") || "no 0x8071"}`,
    `Startup and shutdown: ${e ? `${power("startup", e.startup)}, ${power("shutdown", e.shutdown)}` : "no 0x8071"}`,
    `Lights dim after: ${e?.idle_s != null ? `${e.idle_s} s, off ${e.sleep_s} s later` : "not read"}`,
    `Per-key lighting: ${lighting.keys.length ? `${lighting.keys.length} keys (${ranges(lighting.keys)})` : "none"}`,
    `Layout (0x4540): ${lighting.layout ?? "not read"}`,
    `Lighting replies: ${lighting.raw.join(" | ") || "none"}`,
  ];
}

/** What PeriHub shows on a keyboard it drives, in one short line: effect ids, colours and periods by
 * zone, and how many keys have how many colours. */
function lookText(look: LightsLook): string {
  const zones = look.zones.map((z) =>
    [`zone ${z.zone} effect 0x${z.effect.toString(16).padStart(2, "0")}`, z.color, z.period_ms && `${z.period_ms} ms`, z.direction && `direction ${z.direction}`]
      .filter(Boolean)
      .join(" "),
  );
  if (look.keys.length) zones.push(`${look.keys.length} keys in ${new Set(look.keys.map(([, c]) => c)).size} colours`);
  return zones.join("; ");
}

/** The mouse's profile list when it was connected. */
const PROFILE_LIST = {
  own: "saved in the mouse",
  factory: "none when connected (factory profiles in use)",
  damaged: "damaged when connected (checksum does not match)",
};

const hex = (id: number) => id.toString(16).padStart(4, "0");

/** The end of a test report: every request and answer in full, the transcript a replay test is made
 * from (docs/replay.md). None from an older core. */
function transcriptLines(transcript: string | undefined): string[] {
  return transcript ? ["Transcript (every request and answer, for a replay test):", transcript.trimEnd()] : [];
}

/** Connected over Bluetooth Low Energy (the core says which id the mouse has there). */
const overBluetooth = (mouse: MouseInfo) => typeof mouse.bluetooth === "number";

/** The id a report names: the cable's, the receiver's, or the one the mouse has over Bluetooth. */
function usbId(mouse: MouseInfo): string {
  return `${LOGITECH}:${hex(mouse.bluetooth ?? mouse.link?.receiver_id ?? mouse.product_id)}`;
}

/** What a Logitech receiver is called, from its USB product id (as the core names it). */
export function receiverName(productId: number | undefined): string {
  if (productId === 0xc548) return "Logi Bolt receiver";
  if (productId === 0xc52b || productId === 0xc532) return "Unifying receiver";
  return "LIGHTSPEED receiver";
}

function connection(mouse: MouseInfo): string {
  if (overBluetooth(mouse)) return "Bluetooth";
  if (!mouse.receiver) return "USB cable";
  if (!mouse.link) return receiverName(undefined);
  const { receiver_id, slot, wireless_id } = mouse.link;
  return `${receiverName(receiver_id)} ${LOGITECH}:${hex(receiver_id)}, slot ${slot}, wireless id ${hex(wireless_id)}`;
}

function support(mouse: MouseInfo): string {
  if (overBluetooth(mouse)) {
    // Issue #43: saves over Bluetooth open on a tested memory format only, as through a receiver.
    return mouse.experimental
      ? "experimental (saves over Bluetooth)"
      : "read-only over Bluetooth (memory format not saved to that way)";
  }
  if (mouse.experimental) {
    return mouse.verified
      ? "experimental (saves through this receiver)"
      : "experimental (memory format not tested yet)";
  }
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
/** The version as a report's first line says it: a test build adds its branch and commit
 * ("0.9.5 (test build test/x@abc1234)"), which tools/triage.py reads: such a report counts as the version's. */
export function reportVersion(version: string, build?: string | null): string {
  return build ? `${version} (test build ${build})` : version;
}

export function testReport(mouse: MouseInfo, version: string, system: string): string {
  const { onboard } = mouse;
  const zones = mouse.led_zones.map(ledZoneText);
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
    ...transcriptLines(mouse.transcript),
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
    connection: overBluetooth(mouse) ? "Bluetooth" : mouse.receiver ? receiverName(mouse.link?.receiver_id) : "USB cable",
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

function batteryText(battery: Battery | null): string {
  if (!battery) return "not read";
  return `${battery.estimated ? "about " : ""}${battery.percent}%, ${battery.state}`;
}

/** What a tester sends about a Logitech receiver and a device paired with it that PeriHub does not
 * support yet (`null`: none answered), or about a device on its own USB connection (`receiver.direct`),
 * which has no receiver: `receiver` then says how it connects and what its HID++ collections answered. */
export function receiverTest(
  receiver: ReceiverReport,
  device: UnsupportedReceiverDevice | null,
  version: string,
  system: string,
  live: LogitechLive | null = null,
): DeviceTestFields {
  const id = `${LOGITECH}:${hex(receiver.product_id)}`;
  const reached = receiver.denied ? "no (the system refused to open it)" : receiver.reachable ? "yes" : "no";
  const lines = receiver.direct
    ? [
        `PeriHub ${version} on ${system}`,
        `Device: USB ID ${id}, on its ${receiver.name} (no receiver), read-only`,
        `HID collections: ${receiver.collections.join("; ") || "none"}`,
        `PeriHub opens its HID++ collection: ${reached}`,
        `HID++ answers: ${receiver.slots.join("; ") || "not asked yet"}`,
        `Open now: ${device && receiver.open_slots.includes(device.slot) ? "yes" : "no"}`,
      ]
    : [
        `PeriHub ${version} on ${system}`,
        `Receiver: ${receiver.name}, USB ID ${id}`,
        `HID collections: ${receiver.collections.join("; ") || "none"}`,
        `PeriHub reaches its paired devices: ${reached}`,
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
    const zones = device.led_zones.map(ledZoneText);
    lines.push(
      `${receiver.direct ? `Device index ${device.slot.toString(16).padStart(2, "0")}` : `Slot ${device.slot}`}: ${device.name ?? "name not read"}, HID++ ${device.protocol}`,
      `Kind: ${kindName(device.kind)}${device.kind === null ? "" : ` (${device.kind})`}`,
      `Device info (0x0003): ${device.device_info ?? "not read"}`,
      `Firmware: ${device.firmware.join(", ") || "not read"}`,
      `Features: ${featureList(device.features) || "not read"}`,
      `DPI: ${dpi}`,
      `Polling (ms): ${device.report_rates_ms.join(", ") || "none reported"}`,
    );
    if (device.report_rates_hz?.length) lines.push(`Polling (Hz, 0x8061): ${device.report_rates_hz.join(" ")}`);
    lines.push(
      `Onboard memory: ${onboard}`,
      `Battery: ${batteryText(device.battery)}${device.battery_reply ? ` (${device.battery_reply})` : ""}`,
      `LED zones (0x8070): ${zones.join("; ") || "none"}`,
    );
    if (device.kind === 0 || device.layout) lines.push(layoutReport(null, device.layout));
    if (device.lighting) lines.push(...lightingLines(device.lighting, device.kind));
    if (live) {
      lines.push(
        `Battery now: ${batteryText(live.battery)}`,
        `DPI now: ${live.dpi ?? "not read"}`,
        `DPI set by PeriHub at every connection: ${live.saved_dpi ?? "none"}`,
      );
      if (live.lighting) {
        const l = live.lighting;
        const control =
          l.control === null
            ? "not read"
            : l.control === 0
              ? "the device (0)"
              : `${live.host && l.control === 3 ? "PeriHub" : "a program"} (${l.control})`;
        lines.push(
          `Lighting now: brightness ${l.brightness ?? "not read"}${l.on === null ? "" : l.on ? ", on" : ", off"}, driven by ${control}`,
        );
      }
      if (live.host) {
        lines.push(`Lights driven by PeriHub: ${live.host.power}`, `PeriHub shows: ${lookText(live.host.look)}`);
      } else {
        if (live.gave_back) lines.push(`PeriHub gave the lights back: ${live.gave_back}`);
        if (live.saved_look) lines.push(`Last look PeriHub showed: ${lookText(live.saved_look)}`);
      }
      if (live.drive_at_start) lines.push(`PeriHub drives the lights at every start${live.waiting ? `; not now: ${live.waiting}` : ""}`);
      // Counted over the whole session: every take-over, give-back and reconnection.
      const s = live.host_log;
      if (s) {
        lines.push(
          `Host mode this session: taken ${s.taken} (${s.on_connect} on reconnection), given back ${s.given_back}${s.last_reason ? ` (last: ${s.last_reason})` : ""}, timeouts ${s.timeouts}, idle notices ${s.idle_events}, in-use notices ${s.active_events}, taken again ${s.retaken}, looks shown ${s.shown}, wave resets ${s.resets}`,
        );
      }
    }
    const errors = [...device.read_errors, ...(device.lighting?.errors ?? [])];
    lines.push(`Could not read: ${errors.join("; ") || "nothing"}`, ...transcriptLines(device.transcript));
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
      info.receiver ? `Battery: ${info.battery ?? "not read"}%` : null,
      `LEDs: ${info.leds}`,
      info.kind === "keyboard" ? layoutReport(`1b1c:${hex(info.product_id)}`, info.layout) : null,
      `Could not read: ${info.read_errors.join("; ") || "nothing"}`,
      `Experimental lighting: ${info.lighting ? "on" : "off"}`,
      ...transcriptLines(info.transcript),
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

/** What a tester sends about a device of another brand, with every request and answer in full (the
 * transcript a replay test is made from, docs/replay.md), or the short list of an older core. */
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
        .map((b) => {
          const text = b.last!.type === "dpi" ? b.last!.value.join(", ") : blindText(b.last, b);
          // A setting the device keeps and cannot report: what PeriHub sent, in this session or an earlier one.
          return b.remembered
            ? `Last sent by PeriHub: ${b.label} = ${text}; before that: ${b.previous ? blindText(b.previous, b) : "not known (set by other software)"}`
            : `Sent: ${b.label} = ${text}`;
        }),
      ...(info.sent?.length
        ? ["Changes sent, with the answer of the device to each:", ...info.sent.map((line) => `  ${line}`)]
        : []),
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
      ...(info.transcript ? transcriptLines(info.transcript) : ["Requests and answers:", ...info.raw.map((line) => `  ${line}`)]),
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

/** What the device request form takes for "Kind of device": the labels of its options. */
export type DeviceKind = "Mouse" | "Keyboard" | "Headset" | "Other";

const GENERIC_DESKTOP = 0x01;
const MOUSE_USAGE = 0x02;
const KEYBOARD_USAGE = 0x06;
const TELEPHONY = 0x0b;

/** Whether the keyboard collection of this interface has an output report, which is how a keyboard gets its
 * Caps Lock and other indicator lights. A mouse's keyboard collection (for its macros) has none, as a rule.
 * `false` as well when the descriptors were not read: nothing is then known. */
function keyboardHasLights(device: ReportDevice, interface_: number): boolean {
  return device.descriptors
    .filter((d) => d.interface === interface_)
    .flatMap((d) => d.collections ?? [])
    .some((c) => c.usage_page === GENERIC_DESKTOP && c.usage === KEYBOARD_USAGE && c.reports.some((r) => r.output > 0));
}

/** What a device is, from its name when it says so, else from its collections: the mouse or keyboard ones
 * of the lowest interface that has any (a mouse often has a keyboard interface for its macros, and a
 * keyboard a mouse one), or the telephony page headsets use. When that interface has both, as the
 * SteelSeries Apex Pro's has (a mouse, then a keyboard collection), the device is a keyboard if its keyboard
 * collection has an output report (the indicator lights), and "Other" if it has none or the descriptors were
 * not read: the report then says so, no kind is filled in, and the form's own choice stands. */
export function deviceKind(device: ReportDevice): DeviceKind {
  const name = reportDeviceName(device).toLowerCase();
  if (/headset|headphone|earbud/.test(name)) return "Headset";
  if (/mouse/.test(name)) return "Mouse";
  if (/keyboard/.test(name)) return "Keyboard";
  const input = device.collections.filter(
    (c) => c.usage_page === GENERIC_DESKTOP && (c.usage === MOUSE_USAGE || c.usage === KEYBOARD_USAGE),
  );
  if (input.length > 0) {
    const first = Math.min(...input.map((c) => c.interface));
    const here = input.filter((c) => c.interface === first);
    const mouse = here.some((c) => c.usage === MOUSE_USAGE);
    const keyboard = here.some((c) => c.usage === KEYBOARD_USAGE);
    if (mouse !== keyboard) return mouse ? "Mouse" : "Keyboard";
    return keyboardHasLights(device, first) ? "Keyboard" : "Other";
  }
  return device.collections.some((c) => c.usage_page === TELEPHONY) ? "Headset" : "Other";
}

/** The brands the forms list, by USB vendor id. */
const BRANDS: Record<number, string> = {
  0x046d: "Logitech", 0x1038: "SteelSeries", 0x1b1c: "Corsair", 0x1532: "Razer", 0x0b05: "ASUS",
};

/** A link longer than this is not opened by GitHub (it answers 414 at about 8 KB); see also [fitsGithubLink],
 * which every link is checked with. */
export const MAX_URL = MAX_LINK;
/** Said in the details when the link could not carry them all. */
export const SHORTENED = "(shortened: Copy the details and paste them)";

/** The reports of an interface's collections: "usage page ff00, usage 0001: id 10 (in 7, out 7), id 11 (in 20)". */
function reportLengths(collections: DescriptorCollection[]): string {
  const lengths = (r: DescriptorReports) =>
    [["in", r.input], ["out", r.output], ["feature", r.feature]]
      .filter(([, n]) => n)
      .map(([kind, n]) => `${kind} ${n}`)
      .join(", ");
  return collections
    .map((c) => {
      const ids = c.reports.map((r) => `${r.id === 0 ? "no id" : "id " + r.id.toString(16).padStart(2, "0")} (${lengths(r)})`);
      return `usage page ${hex(c.usage_page)}, usage ${hex(c.usage)}: ${ids.join(", ") || "no reports"}`;
    })
    .join("; ");
}

/** What happened to one interface's feature reports, one line for it and one for each id asked for. */
const FEATURE_ENDINGS: Record<InterfaceFeatures["outcome"], string> = {
  nothing_to_read: "",
  not_opened: "not opened (it needs rights PeriHub does not ask for)",
  read: "",
  stopped_after_error: "stopped after the first error, the other ids were not asked for",
  timed_out: "stopped after no answer in time, the other ids were not asked for",
};

/** The section "Feature reports (read only)": the current contents of the feature reports a device's
 * vendor-defined collections declare, read with GET_FEATURE only, from the interfaces PeriHub could open
 * without extra rights. `null`: they were not read (yet). */
export function featureLines(reports: InterfaceFeatures[] | null): string[] {
  if (reports === null) return ["  not read"];
  const lines = reports.flatMap((f) => [
    ...f.reports.map(
      (r) =>
        "  interface " + f.interface + ", id " + r.id.toString(16).padStart(2, "0") + " (" + r.length + " bytes): " + (r.hex ?? "no answer"),
    ),
    ...(FEATURE_ENDINGS[f.outcome] ? ["  interface " + f.interface + ": " + FEATURE_ENDINGS[f.outcome]] : []),
  ]);
  return lines.length ? lines : ["  none declared by a vendor-defined collection"];
}

/** What PeriHub sees of a device, for the "What PeriHub saw" field: names, ids, release number, HID
 * collections, the shape of each interface's report descriptor and the contents of its feature reports
 * (never a serial number or a path). `raw`: also the descriptors in hex, which a maintainer can decode to
 * the last byte. `recording`: also a USB recording of the official software. `features`: also the feature
 * reports (the section is then there, saying it was left out). */
export function deviceDetails(
  device: ReportDevice,
  version: string,
  system: string,
  raw = true,
  recording: string | null = null,
  features = true,
): string {
  const collections = device.collections.map(
    (c) =>
      "  " + (c.interface < 0 ? "no interface number" : "interface " + c.interface) + ", usage page " + hex(c.usage_page) + ", usage " + hex(c.usage),
  );
  const descriptors = device.descriptors.map((d) => {
    const where = d.interface < 0 ? "no interface number" : "interface " + d.interface;
    if (d.hex === null) return "  " + where + ": not read";
    return "  " + where + ", " + d.hex.length / 2 + " bytes: " + (d.collections ? reportLengths(d.collections) : "read, not understood");
  });
  const rawLines = device.descriptors
    .filter((d) => d.hex !== null)
    .map((d) => "  " + (d.interface < 0 ? "no interface number" : "interface " + d.interface) + ": " + d.hex);
  return [
    "PeriHub " + version + " on " + system,
    "Device: " + reportDeviceName(device),
    "Kind: " + deviceKind(device),
    "USB ID: " + reportUsbId(device) + " (" + device.bus + ")",
    "Release number (bcdDevice): " + (device.release === null ? "not read" : "0x" + hex(device.release)),
    "USB interfaces: " + (device.usb_interfaces ?? "not read"),
    "HID collections:",
    ...collections,
    "Report descriptors (lengths in bytes, with the id byte):",
    ...(descriptors.length ? descriptors : ["  not read"]),
    ...(rawLines.length ? ["Raw descriptors (hex):", ...(raw ? rawLines : ["  left out of the link: use Copy report"])] : []),
    "Feature reports (read only):",
    ...(features ? featureLines(device.feature_reports) : ["  left out of the link: use Copy report"]),
    ...(recording ? [RECORDING_HEADING, recording] : []),
  ].join("\n");
}

/** Above a USB recording made by "Record the official software" (docs/captures.md). */
export const RECORDING_HEADING =
  "USB recording of the official software (made by PeriHub; key presses, other devices and the serial number left out):";

/** The device request form, filled in with what PeriHub sees of `device`. GitHub fills in inputs,
 * text areas and dropdowns from the link, not checkboxes, so what the device can do is left to be
 * ticked. The link stays under [MAX_URL]: the USB recording goes first (it is the biggest; it is cut short
 * and then left out), then the raw descriptors, then the feature reports, then the end of the details. */
export function deviceRequestUrl(device: ReportDevice, version: string, system: string, recording: string | null = null): string {
  return deviceRequestLink(device, version, system, recording).url;
}

/** [deviceRequestUrl], and whether the details had to be shortened to fit the link. */
export function deviceRequestLink(
  device: ReportDevice,
  version: string,
  system: string,
  recording: string | null = null,
): { url: string; shortened: boolean } {
  const name = reportDeviceName(device);
  const link = (details: string) => {
    const kind = deviceKind(device);
    const values = {
      title: "Device request: " + name,
      model: name,
      // The form's own dropdowns (the triage sets its labels from them): no device type for a headset.
      ...(kind === "Mouse" || kind === "Keyboard" ? { "device-type": kind } : {}),
      brand: BRANDS[device.vendor_id] ?? "Other",
      "usb-id": reportUsbId(device),
      details,
    };
    const query = Object.entries(values).map(([field, value]) => field + "=" + encodeURIComponent(value));
    return DEVICE_REQUEST_FORM + "&" + query.join("&");
  };
  const full = link(deviceDetails(device, version, system, true, recording));
  if (fitsGithubLink(full)) return { url: full, shortened: false };
  const short = (details: string) => link([details, SHORTENED].join("\n"));
  // 1. The USB recording, as many of its lines as fit (none: the section is left out).
  if (recording) {
    const rows = recording.split("\n");
    const fits = (n: number) => fitsGithubLink(short(deviceDetails(device, version, system, true, rows.slice(0, n).join("\n"))));
    let lo = 0;
    let hi = rows.length - 1;
    while (lo < hi) {
      const mid = Math.ceil((lo + hi) / 2);
      if (fits(mid)) lo = mid;
      else hi = mid - 1;
    }
    if (lo > 0) return { url: short(deviceDetails(device, version, system, true, rows.slice(0, lo).join("\n"))), shortened: true };
  }
  // 2. The raw descriptors, 3. the feature reports, 4. the end of the details.
  const withFeatures = deviceDetails(device, version, system, false);
  if (fitsGithubLink(short(withFeatures))) return { url: short(withFeatures), shortened: true };
  const lines = deviceDetails(device, version, system, false, null, false).split("\n");
  while (lines.length > 1 && !fitsGithubLink(link([...lines, SHORTENED].join("\n")))) lines.pop();
  return { url: link([...lines, SHORTENED].join("\n")), shortened: true };
}
