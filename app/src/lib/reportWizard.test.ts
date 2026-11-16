import { describe, expect, it, vi } from "vitest";

vi.mock("./api", () => ({ errorText: (e: unknown) => (e instanceof Error ? e.message : String(e)) }));

import type {
  CorsairInfo, DeviceStatus, G300Info, MouseInfo, PeripheralInfo, ReceiverReport, ReportDevice,
  UnsupportedReceiverDevice,
} from "./api";
import {
  candidates, checklist, checklistText, formAddress, formUrl, MAX_URL_LENGTH, needsPaste, PASTE_NOTE, prepareForm,
  readDevice, reportText, type ReadOnlyApi,
} from "./reportWizard";

const mouse = {
  product_id: 0xc08b,
  model: "G502 HERO",
  name: "G502 HERO Gaming Mouse",
  verified: true,
  experimental: false,
  pending_undo: false,
  receiver: false,
  link: null,
  host_mode: false,
  firmware: [{ kind: 0, name: "U1", version: "27.2.1" }],
  dpi: { min: 100, max: 25600, step: 50, list: [] },
  report_rates_ms: [1, 2],
  led_zones: [],
  features: [],
  onboard: {
    memory_model: 1, profile_format: 2, macro_format: 1, profile_count: 5, rom_profile_count: 5, button_count: 11,
    sector_count: 16, sector_size: 256, has_gshift: true, has_dpi_shift: true,
  },
  writable: true,
  writable_format: true,
  switches_back: true,
  profile_list: "own",
} as MouseInfo;

const g300 = {
  product_id: 0xc246, model: "G300", active_profile: 1, active_level: 1, active_raw: "f0 02 00 00", editable: true,
  pending_undo: false, profiles: [],
} as G300Info;

const corsair = {
  product_id: 0x1b4f, model: "M65", kind: "mouse", receiver: null, leds: 3, firmware: "1.0", mode: 1,
  polling_hz: 1000, dpi: 800, read_errors: [], lighting: null,
} as CorsairInfo;

const peripheral = {
  key: "0b05:195c", vendor_id: 0x0b05, product_id: 0x195c, brand: "ASUS", model: "ROG Keris", kind: "mouse",
  source: "libratbag (MIT)", profiles: 3, firmware: "1.02.03", battery: null, active_profile: 1, active_dpi: 1,
  readable: true,
  settings: { dpi_levels: [800], polling_hz: 1000, options: {}, leds: [] },
  limits: { dpi: { min: 100, max: 16000, step: 100 }, polling_hz: [1000], options: [], leds: [] },
  buttons: [], details: [], blind: [], editable: true, can_save: true, changes_stay: false, problem: null,
  pending_undo: false, raw: ["12 04 00 00 → 12 04 00 00 03 00"],
} as unknown as PeripheralInfo;

const device = {
  slot: 1, name: "MX Master 3S", kind: 3, protocol: "4.5", wireless_id: null, device_info: "02 xx xx xx xx 00",
  firmware: ["RBM 12.01"], features: [], dpi: { min: 200, max: 8000, step: 50, list: [] }, report_rates_ms: [],
  onboard: null, read_errors: [],
} as UnsupportedReceiverDevice;

const receiver = (unsupported: UnsupportedReceiverDevice[], extra: Partial<ReceiverReport> = {}) =>
  ({
    product_id: 0xc548, name: "Logi Bolt receiver", collections: [], bolt: true, unifying: false, reachable: true,
    denied: false, open_slots: [1], slots: [], unsupported, ...extra,
  }) as ReceiverReport;

const unknown: ReportDevice = {
  vendor_id: 0x1234, product_id: 0xabcd, manufacturer: "Acme", product: "Zap Mouse", bus: "USB",
  collections: [{ interface: 0, usage_page: 1, usage: 2 }],
};

const status = (part: Partial<DeviceStatus>): DeviceStatus =>
  ({ mouse: null, g300: null, corsair: [], peripherals: [], receivers: [], qmk: [], ...part }) as DeviceStatus;

describe("candidates", () => {
  it("lists every connected device with its state", () => {
    const list = candidates(
      status({ mouse, g300, corsair: [corsair], peripherals: [peripheral], receivers: [receiver([device])] }),
      [unknown],
    );
    expect(list.map((c) => [c.name, c.state, c.kind])).toEqual([
      ["Logitech G502 HERO", "Supported", "mouse"],
      ["Logitech G300", "Supported", "mouse"],
      ["Corsair M65", "Experimental", "mouse"],
      ["ASUS ROG Keris", "Experimental", "mouse"],
      ["Logitech MX Master 3S", "Experimental", "mouse"],
      ["Acme Zap Mouse", "Not supported yet", "other"],
    ]);
    expect(new Set(list.map((c) => c.id)).size).toBe(list.length);
  });

  it("offers a receiver with nothing paired that answers, but not a Unifying receiver Linux keeps closed", () => {
    expect(candidates(status({ receivers: [receiver([])] }), []).map((c) => c.name)).toEqual(["Logi Bolt receiver"]);
    expect(candidates(status({ receivers: [receiver([], { unifying: true, denied: true })] }), [])).toEqual([]);
  });

  it("leaves out the QMK keyboards, which have a page of their own, and offers nothing with no device", () => {
    const qmk = [{ key: "1234:abcd" }] as DeviceStatus["qmk"];
    expect(candidates(status({ qmk }), [unknown])).toEqual([]);
    expect(candidates(status({}), [])).toEqual([]);
  });
});

/** An api where anything that is not a reading fails the test. */
function readingApi(part: Partial<ReadOnlyApi> = {}): ReadOnlyApi {
  return {
    status: vi.fn().mockResolvedValue(status({})),
    devicesForReport: vi.fn().mockResolvedValue([]),
    mouseLive: vi.fn(),
    g300Info: vi.fn(),
    peripheralInfo: vi.fn(),
    logitechLive: vi.fn(),
    ...part,
  } as ReadOnlyApi;
}

describe("readDevice", () => {
  it("reads a mouse's status and live values, and nothing else", async () => {
    const [c] = candidates(status({ mouse }), []);
    const io = readingApi({
      status: vi.fn().mockResolvedValue(status({ mouse: { ...mouse, model: "G502 HERO 2" } })),
      mouseLive: vi.fn().mockResolvedValue({ active_profile: 2, current_dpi: 1600, battery: { percent: 80 } }),
    });
    const seen: string[][] = [];
    const result = await readDevice({ ...c, id: c.id }, io, (r) => seen.push(r.map((x) => x.state)));
    expect(result.readings.map((r) => r.state)).toEqual(["ok", "ok"]);
    expect(result.readings[1].detail).toBe("profile 2, DPI 1600, battery 80 %");
    // The old reading of the status is not kept: ids stay the same, the info is the new one.
    expect(result.candidate.target.type === "mouse" && result.candidate.target.mouse.model).toBe("G502 HERO 2");
    expect(seen[0]).toEqual(["run", "wait"]);
    expect(seen.at(-1)).toEqual(["ok", "ok"]);
  });

  it("takes the new reading of a device of another brand, with the answers it gave", async () => {
    const [c] = candidates(status({ peripherals: [peripheral] }), []);
    const fresh = { ...peripheral, raw: ["a", "b"] };
    const io = readingApi({
      status: vi.fn().mockResolvedValue(status({ peripherals: [peripheral] })),
      peripheralInfo: vi.fn().mockResolvedValue(fresh),
    });
    const result = await readDevice(c, io);
    expect(io.peripheralInfo).toHaveBeenCalledWith("0b05:195c");
    expect(result.candidate.target.type === "peripheral" && result.candidate.target.info.raw).toEqual(["a", "b"]);
  });

  it("keeps going when one reading fails, and says why", async () => {
    const [c] = candidates(status({ g300 }), []);
    const io = readingApi({
      status: vi.fn().mockResolvedValue(status({ g300 })),
      g300Info: vi.fn().mockRejectedValue("no answer from the mouse"),
    });
    const result = await readDevice(c, io);
    expect(result.readings.map((r) => r.state)).toEqual(["ok", "fail"]);
    expect(result.readings[1].detail).toBe("no answer from the mouse");
    expect(result.candidate.target.type).toBe("g300");
  });

  it("stops when the device is gone", async () => {
    const [c] = candidates(status({ mouse }), []);
    const io = readingApi();
    const result = await readDevice(c, io);
    expect(result.readings.map((r) => r.state)).toEqual(["fail", "skip"]);
    expect(io.mouseLive).not.toHaveBeenCalled();
  });

  it("does not ask a Logitech device that is asleep", async () => {
    const asleep = receiver([device], { open_slots: [] });
    const [c] = candidates(status({ receivers: [asleep] }), []);
    const io = readingApi({ status: vi.fn().mockResolvedValue(status({ receivers: [asleep] })) });
    const result = await readDevice(c, io);
    expect(io.logitechLive).not.toHaveBeenCalled();
    expect(result.readings[1].state).toBe("skip");
    expect(result.live).toBeNull();
  });

  it("reads a Logitech device's DPI, and a Corsair device and an unknown one nothing more", async () => {
    const open = receiver([device]);
    const [l] = candidates(status({ receivers: [open] }), []);
    const live = { battery: null, dpi: 1200, saved_dpi: null };
    const io = readingApi({
      status: vi.fn().mockResolvedValue(status({ receivers: [open] })),
      logitechLive: vi.fn().mockResolvedValue(live),
    });
    expect((await readDevice(l, io)).live).toEqual(live);
    expect(io.logitechLive).toHaveBeenCalledWith(0xc548, 1);

    const [k] = candidates(status({ corsair: [corsair] }), []);
    const corsairIo = readingApi({ status: vi.fn().mockResolvedValue(status({ corsair: [corsair] })) });
    expect((await readDevice(k, corsairIo)).readings).toHaveLength(1);

    const [u] = candidates(status({}), [unknown]);
    const unknownIo = readingApi({ devicesForReport: vi.fn().mockResolvedValue([unknown]) });
    expect((await readDevice(u, unknownIo)).readings.map((r) => r.state)).toEqual(["ok"]);
  });
});

describe("the form", () => {
  const targets = (): [string, ReturnType<typeof candidates>[number]][] =>
    candidates(
      status({ mouse, g300, corsair: [corsair], peripherals: [peripheral], receivers: [receiver([device]), receiver([], { product_id: 0xc52b, unifying: true })] }),
      [unknown],
    ).map((c) => [c.name, c]);

  it.each(targets())("builds the form of %s from the existing builders", (_name, c) => {
    const address = formAddress(c, null, "0.9.0", "linux x86_64");
    expect(address.startsWith("https://github.com/hex-dump/perihub/issues/new?template=")).toBe(true);
    const form = prepareForm(address, "", []);
    expect(form.base.endsWith("&")).toBe(true);
    expect(address.startsWith(form.base)).toBe(true);
    expect(reportText(form)).toContain("PeriHub 0.9.0 on linux x86_64");
    // Unchanged, as nothing needed hiding: the same address comes out.
    expect(formUrl(form)).toBe(address);
  });

  it("uses the device request for a device PeriHub does not know, and keeps its text in details", () => {
    const [u] = candidates(status({}), [unknown]);
    const form = prepareForm(formAddress(u, null, "0.9.0", "linux x86_64"), "", []);
    expect(form.base).toContain("template=device_request.yml");
    expect(form.textField).toBe("details");
    expect(reportText(form)).toContain("USB ID: 1234:abcd (USB)");
  });

  it("hides private details in every field and adds the checklist to the report", () => {
    const [c] = candidates(status({ peripherals: [{ ...peripheral, firmware: "1.0 serial: AB12CD34EF", details: [["Log", "/home/ann/x"]] } as PeripheralInfo] }), []);
    const form = prepareForm(formAddress(c, null, "0.9.0", "linux x86_64"), "What I tried:\n  worked: DPI", ["ann"]);
    const text = reportText(form);
    expect(text).toContain("serial: [id hidden]");
    expect(text).toContain("Log: /home/[user]/x");
    expect(text).not.toContain("ann");
    expect(text.endsWith("What I tried:\n  worked: DPI")).toBe(true);
    expect(form.hidden.map((h) => h.what).sort()).toEqual(["serial number or unit id", "user name in a path"]);
    // The address holds what the preview shows.
    expect(new URL(formUrl(form)).searchParams.get("report")).toBe(text);
  });

  it("adds the write log lines after the report and before the checklist, hidden like the rest", () => {
    const [c] = candidates(status({ peripherals: [peripheral] }), []);
    const writes = ["Writes this session: 1 sent, 0 refused:", "  10:00:00 ROG Keris: save, 4 bytes, failed in /home/ann/x"];
    const form = prepareForm(formAddress(c, null, "0.9.0", "linux x86_64"), "What I tried:", ["ann"], writes);
    const text = reportText(form);
    expect(text).toContain("\nWrites this session: 1 sent, 0 refused:\n  10:00:00 ROG Keris: save, 4 bytes, failed in /home/[user]/x\n\nWhat I tried:");
    expect(text).not.toContain("ann");
  });

  it("leaves the write log out of a device request", () => {
    const [u] = candidates(status({}), [unknown]);
    const form = prepareForm(formAddress(u, null, "0.9.0", "linux x86_64"), "", [], ["Writes this session: 0 sent, 0 refused"]);
    expect(reportText(form)).not.toContain("Writes this session");
  });

  it("falls back to pasting when the report does not fit in an address", () => {
    const raw = Array.from({ length: 400 }, (_, i) => `12 04 00 ${i % 100} → 12 04 00 00 03 00 8f 91 aa bb cc dd ee ff`);
    const [c] = candidates(status({ peripherals: [{ ...peripheral, raw }] }), []);
    const form = prepareForm(formAddress(c, null, "0.9.0", "linux x86_64"), "", []);
    expect(needsPaste(form)).toBe(true);
    const short = formUrl(form, false);
    expect(short.length).toBeLessThan(MAX_URL_LENGTH);
    expect(new URL(short).searchParams.get("report")).toBe(PASTE_NOTE);
    expect(new URL(short).searchParams.get("model")).toBe("ASUS ROG Keris");
    expect(short.startsWith(form.base)).toBe(true);
  });
});

describe("checklist", () => {
  const first = (s: Partial<DeviceStatus>, u: ReportDevice[] = []) => candidates(status(s), u)[0];

  it("depends on the kind of device", () => {
    const ids = (c: ReturnType<typeof first>) => checklist(c).map((i) => i.id);
    expect(ids(first({ mouse }))).toEqual(["name", "again", "dpi", "polling", "buttons", "leds"]);
    expect(ids(first({ corsair: [{ ...corsair, kind: "keyboard" }] }))).toEqual(["name", "again", "leds", "keys", "change", "kept"]);
    expect(ids(first({ receivers: [receiver([])] }))).toEqual(["name", "again"]);
    expect(ids(first({}, [unknown]))).toEqual([]);
  });

  it("asks about changes only for a device that can be changed", () => {
    expect(checklist(first({ peripherals: [peripheral] })).map((i) => i.id)).toContain("change");
    expect(checklist(first({ mouse })).map((i) => i.id)).not.toContain("change");
  });

  it("writes only what was answered", () => {
    const items = checklist(first({ mouse }));
    expect(checklistText(items, {})).toBe("");
    expect(checklistText(items, { dpi: "", name: "worked", polling: "failed", leds: "worked" })).toBe(
      [
        "What I tried:",
        "  worked: The name and USB ID PeriHub shows are right",
        "  worked: The lighting PeriHub shows matches the mouse",
        "  did not work: The polling rate PeriHub shows is right",
      ].join("\n"),
    );
  });
});
