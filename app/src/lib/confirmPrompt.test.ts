import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { CorsairInfo, DeviceStatus, MouseInfo, PeripheralInfo, ReceiverReport, UnsupportedReceiverDevice } from "./api";
import { PROMPTED_KEY, markPrompted, nextToPrompt, promptedKeys, unconfirmedDevices } from "./confirmPrompt";

export const empty = (): DeviceStatus => ({
  keyboard: null, mouse: null, keyboard_error: null, mouse_error: null, mouse_asleep: null,
  unsupported_keyboard: null, conflicts: [], corsair: [], g300: null, g300_error: null, corsair_error: null,
  peripherals: [], peripheral_error: null, qmk: [], receivers: [], logitech_programs: [], sandboxed: false, scanned: true,
});

const mouse = (verified: boolean) => ({ product_id: 0xc08d, model: "G502 X", verified }) as unknown as MouseInfo;const corsair = { product_id: 0x1b2e, model: "M65", kind: "mouse", tested: false, receiver: null, read_errors: [] } as unknown as CorsairInfo;
const peripheral = { key: "1532:0099", brand: "Razer", model: "Viper", kind: "mouse", tested: false } as unknown as PeripheralInfo;
const device = { slot: 2, name: "G403", kind: 3 } as unknown as UnsupportedReceiverDevice;
const receiver = { product_id: 0xc539, name: "LIGHTSPEED receiver", unsupported: [device] } as unknown as ReceiverReport;

beforeEach(() => localStorage.clear());
afterEach(() => vi.restoreAllMocks());

describe("unconfirmedDevices", () => {
  it("counts an unverified mouse, with the key and the name of the sidebar", () => {
    const list = unconfirmedDevices({ ...empty(), mouse: mouse(false) });
    expect(list.map((d) => [d.key, d.name])).toEqual([["046d:c08d", "Logitech G502 X"]]);
  });

  it("does not count a verified mouse, a sleeping one, an error, the keyboard, the G300 or QMK", () => {
    expect(unconfirmedDevices({ ...empty(), mouse: mouse(true) })).toEqual([]);
    expect(
      unconfirmedDevices({
        ...empty(),
        mouse_asleep: "G502 X",
        mouse_error: "denied",
        keyboard: { name: "Apex 3 TKL" } as DeviceStatus["keyboard"],
        g300: {} as DeviceStatus["g300"],
        qmk: [{}] as DeviceStatus["qmk"],
      }),
    ).toEqual([]);
  });

  it("counts every Corsair, every other-brand device and every read-only receiver device", () => {
    const list = unconfirmedDevices({ ...empty(), corsair: [corsair], peripherals: [peripheral], receivers: [receiver] });
    expect(list.map((d) => [d.key, d.name])).toEqual([
      ["1b1c:1b2e", "Corsair M65"],
      ["1532:0099", "Razer Viper"],
      ["046d:c539-2", "Logitech G403"],
    ]);
  });

  it("keys a receiver device by its own id when it reports one, by receiver and slot otherwise", () => {
    const own = { ...device, wireless_id: 0x4070 } as UnsupportedReceiverDevice;
    const other = { ...receiver, product_id: 0xc547, unsupported: [own] } as unknown as ReceiverReport;
    const first = unconfirmedDevices({ ...empty(), receivers: [{ ...receiver, unsupported: [own] } as unknown as ReceiverReport] });
    const second = unconfirmedDevices({ ...empty(), receivers: [other] });
    expect(first[0].key).toBe("046d:w4070");
    expect(second[0].key).toBe("046d:w4070");
    const unknown = { ...device, wireless_id: null } as UnsupportedReceiverDevice;
    const bare = unconfirmedDevices({ ...empty(), receivers: [{ ...receiver, unsupported: [unknown] } as unknown as ReceiverReport] });
    expect(bare[0].key).toBe("046d:c539-2");
  });

  it("leaves out a device marked tested, as a later flag will", () => {
    const status = { ...empty(), corsair: [{ ...corsair, tested: true }], peripherals: [{ ...peripheral, tested: false }] };
    expect(unconfirmedDevices(status).map((d) => d.key)).toEqual(["1532:0099"]);
  });

  it("hands over the device as the wizard lists it, so the wizard can start with it", () => {
    const [d] = unconfirmedDevices({ ...empty(), corsair: [corsair] });
    expect(d.candidate.id).toBe("corsair:6958");
    expect(d.candidate.target.type).toBe("corsair");
  });

  it("is empty until the first scan is over", () => {
    expect(unconfirmedDevices({ ...empty(), mouse: mouse(false), scanned: false })).toEqual([]);  });
});

describe("once only", () => {
  it("skips a device whose key is stored, and offers the next", () => {
    const status = { ...empty(), corsair: [corsair], peripherals: [peripheral] };
    expect(nextToPrompt(status)?.key).toBe("1b1c:1b2e");
    markPrompted("1b1c:1b2e");
    expect(JSON.parse(localStorage.getItem(PROMPTED_KEY)!)).toEqual(["1b1c:1b2e"]);
    expect(nextToPrompt(status)?.key).toBe("1532:0099");
    markPrompted("1532:0099");
    expect(nextToPrompt(status)).toBeNull();
  });

  it("skips the keys seen this session even when nothing can be stored", () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("denied");
    });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new Error("denied");
    });
    const status = { ...empty(), corsair: [corsair] };
    expect(promptedKeys().size).toBe(0);
    expect(() => markPrompted("1b1c:1b2e")).not.toThrow();
    expect(nextToPrompt(status)?.key).toBe("1b1c:1b2e");
    expect(nextToPrompt(status, new Set(["1b1c:1b2e"]))).toBeNull();
  });

  it("ignores stored garbage", () => {
    localStorage.setItem(PROMPTED_KEY, "{not json");
    expect(promptedKeys().size).toBe(0);
    localStorage.setItem(PROMPTED_KEY, '{"a":1}');
    expect(promptedKeys().size).toBe(0);
  });
});