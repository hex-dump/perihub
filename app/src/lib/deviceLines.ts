import type { DeviceStatus } from "./api";

/** One line per connected device for Settings → Devices (also what people copy when asking for support). */
export function deviceLines(status: DeviceStatus): string[] {
  const lines: string[] = [];
  const firmware = (text: string | null | undefined) => (text ? `firmware ${text}` : "connected");
  if (status.keyboard) lines.push(`${status.keyboard.name}: firmware ${status.keyboard.firmware}`);
  if (status.mouse) {
    const versions = status.mouse.firmware.map((f) => `${f.name} ${f.version}`).join(", ");
    lines.push(`${status.mouse.model}: ${versions || "connected"}`);
  }
  if (status.g300) lines.push(`${status.g300.model}: connected`);
  for (const d of status.corsair) lines.push(`${d.model}: ${firmware(d.firmware)}`);
  for (const d of status.peripherals) lines.push(`${d.brand} ${d.model}: ${firmware(d.firmware)}`);
  for (const d of status.qmk) lines.push(`${d.brand} ${d.name}: ${d.open ? "open" : d.denied ? "permission denied" : d.enabled ? "connected" : "found, not turned on"}`);
  for (const r of status.receivers) lines.push(`${r.name}: ${r.denied ? "permission denied" : "connected"}`);
  if (status.mouse_asleep && !status.mouse) lines.push(`${status.mouse_asleep}: asleep`);
  if (status.unsupported_keyboard && !status.keyboard) lines.push(`${status.unsupported_keyboard}: connected, not supported yet`);
  for (const error of [status.keyboard_error, status.mouse_error, status.g300_error, status.corsair_error, status.peripheral_error]) {
    if (error && !lines.includes(error)) lines.push(error);
  }
  if (!lines.length) lines.push(status.scanned ? "No supported device connected." : "Looking for devices…");
  return lines;
}
