// The guided test report: which devices can be tested, the read-only readings the wizard asks for,
// the checklist, and the form the report goes into. The report text and the form's address come from
// the builders in testReport.ts (they pick the form): this file only adds the readings, the hiding of
// private details and the checklist to them.
import type {
  api as Api, CorsairInfo, DeviceStatus, G300Info, LogitechLive, MouseInfo, PeripheralInfo, ReceiverReport,
  ReportDevice, UnsupportedReceiverDevice,
} from "./api";
import { MAX_LINK, fitsGithubLink } from "./githubLink";
import { errorText } from "./api";
import { uniqueBy } from "./unique";
import { describe } from "./g300";
import { kindName } from "./logitechFeatures";
import { type Hidden, mergeHidden, redact } from "./redact";
import {
  corsairTest, deviceKind, deviceRequestUrl, deviceTestFormUrl, g300Test, peripheralTest, RECORDING_HEADING,
  receiverTest, reportDeviceName, reportUsbId, testFormUrl,
} from "./testReport";

export type DeviceKind = "mouse" | "keyboard" | "other";

/** What is tested, as PeriHub knows it. */
export type Target =
  | { type: "mouse"; mouse: MouseInfo }
  | { type: "g300"; info: G300Info }
  | { type: "corsair"; info: CorsairInfo }
  | { type: "peripheral"; info: PeripheralInfo }
  | { type: "logitech"; receiver: ReceiverReport; device: UnsupportedReceiverDevice }
  | { type: "receiver"; receiver: ReceiverReport }
  | { type: "unknown"; device: ReportDevice };

/** A device the user can pick. */
export interface Candidate {
  /** The same one again after the device was read again. */
  id: string;
  name: string;
  /** "Supported", "Experimental", "Read-only" or "Not supported yet". */
  state: string;
  kind: DeviceKind;
  target: Target;
}

const hex = (id: number) => id.toString(16).padStart(4, "0");

/** Logitech HID++ device kinds: keyboards and numpads, mice and trackballs. */
function logitechKind(kind: number | null): DeviceKind {
  if (kind === 0 || kind === 2) return "keyboard";
  if (kind === 3 || kind === 5) return "mouse";
  return "other";
}

/** The devices to pick from: the connected ones PeriHub supports or reads, then the ones it does not know.
 * `unknown` as the device report lists it, without the QMK keyboards (they have their own page). */
export function candidates(status: DeviceStatus, unknown: ReportDevice[]): Candidate[] {
  const list: Candidate[] = [];
  if (status.mouse) {
    const m = status.mouse;
    list.push({
      id: `mouse:${m.product_id}`,
      name: `Logitech ${m.model}`,
      state: m.experimental ? "Experimental" : m.verified ? "Supported" : "Read-only",
      kind: "mouse",
      target: { type: "mouse", mouse: m },
    });
  }
  if (status.g300) {
    list.push({ id: "g300", name: "Logitech G300", state: "Supported", kind: "mouse", target: { type: "g300", info: status.g300 } });
  }
  for (const info of status.corsair) {
    list.push({
      id: `corsair:${info.product_id}`,
      name: `Corsair ${info.model}`,
      state: info.tested ? "Supported" : "Experimental",
      kind: info.kind,
      target: { type: "corsair", info },
    });
  }
  for (const info of status.peripherals) {
    list.push({
      id: `peripheral:${info.key}`,
      name: `${info.brand} ${info.model}`,
      state: info.tested ? "Supported" : info.editable ? "Experimental" : "Read-only",
      kind: info.kind,
      target: { type: "peripheral", info },
    });
  }
  for (const receiver of status.receivers) {
    for (const device of receiver.unsupported) {
      list.push({
        id: `logitech:${hex(receiver.product_id)}-${device.slot}`,
        name: `Logitech ${device.name ?? kindName(device.kind)}`,
        state: device.dpi && !device.onboard ? "Experimental" : "Read-only",
        kind: logitechKind(device.kind),
        target: { type: "logitech", receiver, device },
      });
    }
    // A receiver that answers nothing it can be tested through, as on the mouse page.
    if (receiver.unsupported.length === 0 && !(receiver.unifying && receiver.denied)) {
      list.push({
        id: `receiver:${hex(receiver.product_id)}`,
        name: receiver.name,
        state: "Not supported yet",
        kind: "other",
        target: { type: "receiver", receiver },
      });
    }
  }
  const qmk = status.qmk.map((k) => k.key);
  for (const device of unknown) {
    if (qmk.includes(reportUsbId(device)) || deviceKind(device) === "Headset") continue;
    list.push({
      id: `unknown:${reportUsbId(device)}`,
      name: reportDeviceName(device),
      state: "Not supported yet",
      kind: "other",
      target: { type: "unknown", device },
    });
  }
  // One id per device: a device with several collections or reports must not be listed twice (the list is keyed by it).
  return uniqueBy(list, (c) => c.id);
}

// ---- Readings ----

/** What the wizard may ask of the app: these only read. Nothing that saves, changes or tries a setting. */
export type ReadOnlyApi = Pick<
  typeof Api,
  "status" | "devicesForReport" | "mouseLive" | "g300Info" | "peripheralInfo" | "logitechLive"
>;

export interface Reading {
  label: string;
  state: "wait" | "run" | "ok" | "fail" | "skip";
  detail?: string;
}

export interface Probed {
  candidate: Candidate;
  /** What a Logitech device PeriHub knows by its features shows now. */
  live: LogitechLive | null;
  readings: Reading[];
}

/** The readings to do for a device, by what it is. */
function plan(c: Candidate): Reading[] {
  const refresh: Reading = { label: "What PeriHub knows about it", state: "wait" };
  switch (c.target.type) {
    case "mouse":
      return [refresh, { label: "Profile, DPI and battery", state: "wait" }];
    case "g300":
      return [refresh, { label: "Active profile and DPI level", state: "wait" }];
    case "peripheral":
      return [refresh, { label: "Settings and the answers of the device", state: "wait" }];
    case "logitech":
      return [refresh, { label: "Battery and DPI", state: "wait" }];
    default:
      return [refresh];
  }
}

const finding = (list: Candidate[], id: string) => list.find((c) => c.id === id);

/**
 * Reads the device again, and only reads. `onProgress` gets the list of readings every time one
 * changes. A reading that fails does not stop the others: the report then says what could not be read.
 */
export async function readDevice(
  start: Candidate,
  io: ReadOnlyApi,
  onProgress: (readings: Reading[]) => void = () => {},
): Promise<Probed> {
  const readings = plan(start);
  let candidate = start;
  let live: LogitechLive | null = null;
  const report = () => onProgress(readings.map((r) => ({ ...r })));
  const step = async (i: number, work: () => Promise<string | void>) => {
    readings[i].state = "run";
    report();
    try {
      const detail = await work();
      Object.assign(readings[i], { state: "ok", detail: detail || undefined });
    } catch (e) {
      Object.assign(readings[i], { state: "fail", detail: errorText(e) });
    }
    report();
  };
  const skip = (i: number, detail: string) => {
    Object.assign(readings[i], { state: "skip", detail });
    report();
  };

  let present = true;
  await step(0, async () => {
    const [status, unknown] = await Promise.all([
      io.status(),
      start.target.type === "unknown" ? io.devicesForReport() : Promise.resolve([]),
    ]);
    const now = finding(candidates(status, unknown), start.id);
    if (!now) {
      present = false;
      throw new Error("The device is not there any more. Plug it in again.");
    }
    candidate = now;
  });
  if (!present) {
    for (let i = 1; i < readings.length; i++) skip(i, "Not done");
    return { candidate, live, readings };
  }

  const t = candidate.target;
  if (t.type === "mouse") {
    await step(1, async () => {
      const l = await io.mouseLive();
      return `profile ${l.active_profile}, DPI ${l.current_dpi}${l.battery ? `, battery ${l.battery.percent} %` : ""}`;
    });
  } else if (t.type === "g300") {
    await step(1, async () => {
      const info = await io.g300Info();
      candidate = { ...candidate, target: { type: "g300", info } };
      return `profile ${info.active_profile}, level ${info.active_level + 1}`;
    });
  } else if (t.type === "peripheral") {
    await step(1, async () => {
      const info = await io.peripheralInfo(t.info.key);
      candidate = { ...candidate, target: { type: "peripheral", info } };
      return `${info.raw.length} requests and answers`;
    });
  } else if (t.type === "logitech") {
    if (t.receiver.open_slots.includes(t.device.slot)) {
      await step(1, async () => {
        live = await io.logitechLive(t.receiver.product_id, t.device.slot);
        return `${live.dpi === null ? "no DPI" : `DPI ${live.dpi}`}${live.battery ? `, battery ${live.battery.percent} %` : ""}`;
      });
    } else {
      skip(1, "The device is asleep: wake it with a click and read again");
    }
  }
  return { candidate, live, readings };
}

// ---- The form ----

/** The longest address GitHub takes as it is; [fitsGithubLink] also checks it fits through the login page. */
export const MAX_URL_LENGTH = MAX_LINK;

/** The form for a device PeriHub does not know: a request, not a test report, so no write log. */
const DEVICE_REQUEST = "template=device_request.yml";

/** The fields that hold the report text, by form. */
const TEXT_FIELDS = ["report", "details"];

/** The form with its fields, from the builders of testReport.ts. `recording`: a USB recording of the official
 * software, for the request of a device PeriHub does not know. */
export function formAddress(
  c: Candidate,
  live: LogitechLive | null,
  version: string,
  system: string,
  recording: string | null = null,
): string {
  const t = c.target;
  switch (t.type) {
    case "mouse":
      return testFormUrl(t.mouse, version, system);
    case "g300":
      return deviceTestFormUrl(g300Test(t.info, describe, version, system), version, system);
    case "corsair":
      return deviceTestFormUrl(corsairTest(t.info, version, system), version, system);
    case "peripheral":
      return deviceTestFormUrl(peripheralTest(t.info, version, system), version, system);
    case "logitech":
      return deviceTestFormUrl(receiverTest(t.receiver, t.device, version, system, live), version, system);
    case "receiver":
      return deviceTestFormUrl(receiverTest(t.receiver, null, version, system), version, system);
    case "unknown":
      return deviceRequestUrl(t.device, version, system, recording);
  }
}

/** What goes into the form: every field after the private details were hidden. */
export interface Form {
  /** The address up to the fields, as the app opens only those. */
  base: string;
  fields: [name: string, value: string][];
  /** The field that holds the report. */
  textField: string;
  hidden: Hidden[];
}

/** The sections of a device request that PeriHub writes from the device's report descriptors, its feature
 * reports and a recording it has already filtered: their long runs of hex are bytes, not ids, and the second net
 * of redact() would hide every one of them (the raw descriptors of issue #93 arrived as "[id hidden]"). They are
 * the last sections of the details (deviceDetails), so everything from the first of them on stays as it is. */
const BYTE_SECTIONS = ["Raw descriptors (hex):", "Feature reports (read only):", RECORDING_HEADING];

/** redact(), leaving the byte sections of a device request alone. */
export function redactDetails(text: string, names: string[]): ReturnType<typeof redact> {
  const lines = text.split("\n");
  const at = lines.findIndex((l) => BYTE_SECTIONS.includes(l));
  if (at < 0) return redact(text, names);
  const head = redact(lines.slice(0, at).join("\n"), names);
  return { text: [head.text, ...lines.slice(at)].join("\n"), hidden: head.hidden };
}

/** Splits the address of a form from the builders, adds the writes of this session (lines from
 * `writeLogLines`, a test report only) and the checklist to the report and hides the private
 * details in every field. `names`: user names to hide (see `userNames`). */
export function prepareForm(address: string, checklist: string, names: string[], writes: string[] = []): Form {
  const base = address.slice(0, address.indexOf("&") + 1);
  const hidden: Hidden[][] = [];
  const fields: [string, string][] = [];
  let textField = "";
  for (const [name, value] of new URLSearchParams(address.slice(base.length))) {
    const isText = TEXT_FIELDS.includes(name);
    if (isText) textField = name;
    // Like the other test reports: the writes right after the report, before the checklist.
    const report = isText && writes.length && !address.includes(DEVICE_REQUEST) ? [value, ...writes].join("\n") : value;
    const text = isText && checklist ? `${report}\n\n${checklist}` : report;
    const r = isText && address.includes(DEVICE_REQUEST) ? redactDetails(text, names) : redact(text, names);
    hidden.push(r.hidden);
    fields.push([name, r.text]);
  }
  return { base, fields, textField, hidden: mergeHidden(hidden) };
}

export const reportText = (form: Form): string => form.fields.find(([name]) => name === form.textField)?.[1] ?? "";

/** The address to open. `withReport` false: the report field left out of the address altogether, to be pasted. Not
 * even a note in it: GitHub gives a field filled in through the address back as it was, and a tester could not
 * paste over it (issues #124, #152, #156, #157). */
export function formUrl(form: Form, withReport = true): string {
  const query = form.fields
    .filter(([name]) => withReport || name !== form.textField)
    .map(([name, value]) => `${name}=${encodeURIComponent(value)}`);
  return `${form.base}${query.join("&")}`;
}

/** The report does not fit in an address: it is copied, and pasted into the form. */
export const needsPaste = (form: Form): boolean => !fitsGithubLink(formUrl(form));

// ---- The checklist ----

export type Answer = "" | "worked" | "failed";

export interface CheckItem {
  id: string;
  label: string;
}

/** A mouse PeriHub reads through its own driver shows buttons or several profiles only when it read some: one that
 * cannot be read back (SteelSeries) shows neither, so there is nothing to compare with the mouse. */
function offersButtons(t: Target): boolean {
  return t.type !== "peripheral" || t.info.buttons.length > 0 || t.info.profiles > 1;
}

/** What a tester may have tried, by kind of device. Everything is optional. */
export function checklist(c: Candidate): CheckItem[] {
  const t = c.target;
  if (t.type === "unknown") return [];
  const items: CheckItem[] = [
    { id: "name", label: "The name and USB ID PeriHub shows are right" },
    { id: "again", label: "PeriHub finds it again after I unplug it and plug it back in" },
  ];
  if (t.type === "receiver") return items;
  if (c.kind === "mouse") {
    items.push(
      { id: "dpi", label: "The DPI PeriHub shows matches the mouse" },
      { id: "polling", label: "The polling rate PeriHub shows is right" },
      ...(offersButtons(t) ? [{ id: "buttons", label: "The buttons and profiles PeriHub shows match the mouse" }] : []),
      { id: "leds", label: "The lighting PeriHub shows matches the mouse" },
    );
  } else if (c.kind === "keyboard") {
    items.push(
      { id: "leds", label: "The lighting PeriHub shows matches the keyboard" },
      { id: "keys", label: "The keys and settings PeriHub shows match the keyboard" },
    );
  } else {
    items.push({ id: "settings", label: "What PeriHub shows matches the device" });
  }
  // A device a tester confirmed keeps its experimental changes (see confirmed.rs): its report may still say how they went.
  const changes = c.state === "Experimental" || (c.state === "Supported" && (t.type === "corsair" || (t.type === "peripheral" && t.info.editable)));
  if (changes) {
    items.push(
      { id: "change", label: "A change I made in PeriHub took effect on the device (optional, at your own risk)" },
      { id: "kept", label: "A change I kept was still there after unplugging the device (optional, at your own risk)" },
    );
  }
  return items;
}

/** The checklist as lines of the report: only what the user answered. */
export function checklistText(items: CheckItem[], answers: Record<string, Answer>): string {
  const names = (answer: Answer) => items.filter((i) => answers[i.id] === answer).map((i) => i.label);
  const worked = names("worked");
  const failed = names("failed");
  if (worked.length === 0 && failed.length === 0) return "";
  return [
    "What I tried:",
    ...worked.map((l) => `  worked: ${l}`),
    ...failed.map((l) => `  did not work: ${l}`),
  ].join("\n");
}
