import type { MouseInfo } from "./api";

const LOGITECH = "046d";
const TEST_FORM = "https://github.com/hex-dump/perihub/issues/new?template=mouse_test.yml";

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
    `G-Shift: ${onboard.has_gshift ? "yes" : "no"}, DPI shift: ${onboard.has_dpi_shift ? "yes" : "no"}`,
    `DPI: ${dpiRange(mouse)}`,
    `Polling (ms): ${mouse.report_rates_ms.join(", ") || "none reported"}`,
    `LEDs: ${zones.join("; ") || "none reported"}`,
  ].join("\n");
}

/** The test form on GitHub with the fields filled in that PeriHub knows. */
export function testFormUrl(mouse: MouseInfo, version: string, system: string): string {
  const fields = {
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
