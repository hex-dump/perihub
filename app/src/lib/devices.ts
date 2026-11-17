// The brands and models PeriHub supports, from devices.generated.json (cargo xtask gen writes it
// from the driver tables, and cargo xtask check fails when it is out of date).
import generated from "./devices.generated.json";

export type Level = "supported" | "experimental" | "read_only" | "by_memory_format" | "recognized";

export interface DeviceModel {
  name: string;
  kind: "mouse" | "keyboard" | "receiver";
  level: Level;
}

export interface DeviceBrand {
  brand: string;
  models: DeviceModel[];
}

/** From the most to the least supported. */
export const LEVELS: { level: Level; label: string; note: string }[] = [
  { level: "supported", label: "Supported", note: "tested on real hardware" },
  { level: "experimental", label: "Experimental", note: "changes at your own risk" },
  { level: "read_only", label: "Read-only", note: "read, never changed" },
  { level: "by_memory_format", label: "Depends on memory", note: "the level follows the memory format the mouse reports" },
  { level: "recognized", label: "Recognized", note: "named, nothing sent to it" },
];

export const brands: DeviceBrand[] = generated.brands as DeviceBrand[];

/** The models of a brand at each level that has any, most supported first. */
export function byLevel(models: DeviceModel[]) {
  return LEVELS.map((l) => ({ ...l, models: models.filter((m) => m.level === l.level) })).filter(
    (l) => l.models.length > 0,
  );
}
