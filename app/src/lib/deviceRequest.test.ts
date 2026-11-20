import { describe, expect, it } from "vitest";
import type { ReportDevice } from "./api";
import { MAX_URL, SHORTENED, deviceDetails, deviceKind, deviceRequestUrl, featureLines } from "./testReport";

const mouse: ReportDevice = {
  vendor_id: 0x1234,
  product_id: 0x00ab,
  manufacturer: "Maker",
  product: "Maker Pro Mouse",
  bus: "USB",
  collections: [
    { interface: 0, usage_page: 0x01, usage: 0x02 },
    { interface: 1, usage_page: 0xff00, usage: 0x01 },
  ],
  release: 0x0114,
  usb_interfaces: 3,
  descriptors: [
    {
      interface: 0,
      hex: "05010902a1010901a100",
      collections: [{ usage_page: 1, usage: 2, reports: [{ id: 0, input: 3, output: 0, feature: 0 }] }],
    },
    {
      interface: 1,
      hex: "06 00ff".replace(" ", "") + "0901a101",
      collections: [
        {
          usage_page: 0xff00,
          usage: 1,
          reports: [
            { id: 0x10, input: 7, output: 7, feature: 0 },
            { id: 0x11, input: 20, output: 20, feature: 3 },
          ],
        },
      ],
    },
    { interface: 2, hex: null, collections: null },
  ],
  feature_reports: [
    {
      interface: 1,
      outcome: "stopped_after_error",
      reports: [
        { id: 0x20, length: 3, hex: "200a0b" },
        { id: 0x21, length: 9, hex: null },
      ],
    },
    { interface: 2, outcome: "not_opened", reports: [] },
  ],
};

const url = (device: ReportDevice) => new URL(deviceRequestUrl(device, "0.9.0", "Linux x86_64"));

describe("the device request link", () => {
  it("fills in the model, kind, USB id and details", () => {
    const u = url(mouse);
    expect(u.searchParams.get("template")).toBe("device_request.yml");
    expect(u.searchParams.get("model")).toBe("Maker Pro Mouse");
    expect(u.searchParams.get("device-type")).toBe("Mouse");
    expect(u.searchParams.get("brand")).toBe("Other");
    expect(u.searchParams.get("usb-id")).toBe("1234:00ab");
    expect([...u.searchParams.keys()]).toEqual(["template", "title", "model", "device-type", "brand", "usb-id", "details"]);
  });

  it("picks the brand by the vendor id, and no device type for a headset", () => {
    const razer = url({ ...mouse, vendor_id: 0x1532, product: "Razer Foo Headset" });
    expect(razer.searchParams.get("brand")).toBe("Razer");
    expect(razer.searchParams.has("device-type")).toBe(false);
  });

  it("tells the release, the USB interfaces and, for each interface, the report ids with their lengths", () => {
    const details = url(mouse).searchParams.get("details")!;
    expect(details).toContain("Release number (bcdDevice): 0x0114");
    expect(details).toContain("USB interfaces: 3");
    expect(details).toContain("  interface 0, 10 bytes: usage page 0001, usage 0002: no id (in 3)");
    expect(details).toContain(
      "  interface 1, 7 bytes: usage page ff00, usage 0001: id 10 (in 7, out 7), id 11 (in 20, out 20, feature 3)",
    );
    expect(details).toContain("  interface 2: not read");
    expect(details).toContain("  interface 1: 0600ff0901a101");
  });

  it("says not read for what the system did not give, and never fails", () => {
    const bare = { ...mouse, release: null, usb_interfaces: null, descriptors: [], feature_reports: null };
    const details = url(bare).searchParams.get("details")!;
    expect(details).toContain("Release number (bcdDevice): not read");
    expect(details).toContain("USB interfaces: not read");
    expect(details).toContain("Report descriptors (lengths in bytes, with the id byte):\n  not read");
    expect(details).not.toContain("Raw descriptors");
    expect(details).toContain("Feature reports (read only):\n  not read");
  });

  it("holds no serial number and no path", () => {
    const text = decodeURIComponent(deviceRequestUrl(mouse, "0.9.0", "Linux x86_64"));
    expect(text).not.toMatch(/serial/i);
    expect(text).not.toMatch(/\/dev\/|\/sys\/|\\\\\?\\|hidraw/);
  });

  it("stays under the length cap: the raw descriptors go first, and Copy keeps everything", () => {
    const big = "05".repeat(1500);
    const long = {
      ...mouse,
      descriptors: [1, 2, 3].map((interface_) => ({ ...mouse.descriptors[1], interface: interface_, hex: big })),
    };
    const link = deviceRequestUrl(long, "0.9.0", "Linux x86_64");
    expect(link.length).toBeLessThanOrEqual(MAX_URL);
    const details = new URL(link).searchParams.get("details")!;
    expect(details).not.toContain(big);
    expect(details).toContain("id 11 (in 20, out 20, feature 3)");
    expect(details.endsWith(SHORTENED)).toBe(true);
    expect(deviceDetails(long, "0.9.0", "Linux x86_64")).toContain(big);
  });

  it("cuts the end of the details when even they are too long", () => {
    const many = { ...mouse, collections: Array.from({ length: 600 }, (_, i) => ({ interface: i, usage_page: 0xff00, usage: i })) };
    const link = deviceRequestUrl(many, "0.9.0", "Linux x86_64");
    expect(link.length).toBeLessThanOrEqual(MAX_URL);
    const details = new URL(link).searchParams.get("details")!;
    expect(details.startsWith("PeriHub 0.9.0 on Linux x86_64\nDevice: Maker Pro Mouse")).toBe(true);
    expect(details.endsWith(SHORTENED)).toBe(true);
  });

  it("keeps a short link whole, with the raw descriptors", () => {
    const details = url(mouse).searchParams.get("details")!;
    expect(details).not.toContain(SHORTENED);
    expect(details).toContain("Raw descriptors (hex):");
  });
});

describe("the feature reports section", () => {
  const section = (details: string) => details.split("Feature reports (read only):\n")[1];

  it("lists each id with its length and the bytes, and what stopped the reading", () => {
    expect(section(deviceDetails(mouse, "0.9.0", "Linux x86_64"))).toBe(
      [
        "  interface 1, id 20 (3 bytes): 200a0b",
        "  interface 1, id 21 (9 bytes): no answer",
        "  interface 1: stopped after the first error, the other ids were not asked for",
        "  interface 2: not opened (it needs rights PeriHub does not ask for)",
      ].join("\n"),
    );
  });

  it("says so when no vendor-defined collection declares one, or when nothing was read yet", () => {
    expect(section(deviceDetails({ ...mouse, feature_reports: [] }, "0.9.0", "Linux x86_64"))).toBe(
      "  none declared by a vendor-defined collection",
    );
    expect(featureLines(null)).toEqual(["  not read"]);
    expect(featureLines([{ interface: 0, outcome: "timed_out", reports: [] }])[0]).toContain("no answer in time");
  });

  it("is in a short link, whole", () => {
    expect(url(mouse).searchParams.get("details")).toContain("  interface 1, id 20 (3 bytes): 200a0b");
  });

  it("is dropped after the raw descriptors, and before the end of the details", () => {
    const big = "05".repeat(1800);
    const withRaw = { ...mouse, descriptors: [1, 2].map((i) => ({ ...mouse.descriptors[1], interface: i, hex: big })) };
    // Raw descriptors too long for the link, the feature reports fit: they stay.
    const first = new URL(deviceRequestUrl(withRaw, "0.9.0", "Linux x86_64")).searchParams.get("details")!;
    expect(first).not.toContain(big);
    expect(first).toContain("  interface 1, id 20 (3 bytes): 200a0b");
    // The feature reports too long as well: they go, and the rest of the details stays.
    const many = {
      ...withRaw,
      feature_reports: [{ interface: 1, outcome: "read" as const, reports: Array.from({ length: 32 }, (_, i) => ({ id: i + 1, length: 200, hex: "ab".repeat(200) })) }],
    };
    const link = deviceRequestUrl(many, "0.9.0", "Linux x86_64");
    expect(link.length).toBeLessThanOrEqual(MAX_URL);
    const second = new URL(link).searchParams.get("details")!;
    expect(second).not.toContain(big);
    expect(second).toContain("Feature reports (read only):\n  left out of the link: use Copy report");
    expect(second).toContain("id 11 (in 20, out 20, feature 3)");
    // Copy report always has everything.
    const all = deviceDetails(many, "0.9.0", "Linux x86_64");
    expect(all).toContain(big);
    expect(all).toContain("ab".repeat(200));
  });
});

describe("the kind of device", () => {
  const device = (product: string, collections: [number, number, number][]): ReportDevice => ({
    ...mouse,
    manufacturer: null,
    product,
    collections: collections.map(([interface_, usage_page, usage]) => ({ interface: interface_, usage_page, usage })),
  });

  it("follows what the name says", () => {
    expect(deviceKind(device("Foo Gaming Headset", [[0, 1, 2]]))).toBe("Headset");
    expect(deviceKind(device("Foo Keyboard", [[0, 1, 2]]))).toBe("Keyboard");
  });

  it("follows the first mouse or keyboard collection by interface otherwise", () => {
    expect(deviceKind(device("Foo", [[1, 1, 6], [0, 1, 2]]))).toBe("Mouse");
    expect(deviceKind(device("Foo", [[0, 1, 6], [1, 1, 2]]))).toBe("Keyboard");
  });

  it("knows a headset by the telephony page, and anything else is other", () => {
    expect(deviceKind(device("Foo", [[3, 0x0b, 5]]))).toBe("Headset");
    expect(deviceKind(device("Foo", [[0, 0xff89, 0x10]]))).toBe("Other");
  });

  // The SteelSeries Apex Pro (issue #111), as PeriHub 0.9.6 saw it on Windows: its first interface has a mouse
  // collection and then a keyboard one, and the report said "Kind: Mouse" for a keyboard.
  const hid = (id: number, input: number, output = 0, feature = 0) => ({ id, input, output, feature });
  const described = (interface_: number, usage_page: number, usage: number, ...reports: ReturnType<typeof hid>[]) => ({
    interface: interface_,
    hex: "05010902a101",
    collections: [{ usage_page, usage, reports }],
  });
  const apex = (descriptors: ReportDevice["descriptors"]): ReportDevice => ({
    ...device(
      "SteelSeries Apex Pro",
      [
        [0, 1, 2], [0, 1, 6], [0, 0x0c, 1], [1, 0xffc0, 1], [2, 1, 2], [2, 1, 6], [2, 0x0c, 1], [3, 1, 2], [3, 0x0c, 1], [4, 0xffc1, 1],
      ],
    ),
    manufacturer: "SteelSeries",
    vendor_id: 0x1038,
    product_id: 0x1610,
    descriptors,
  });
  const apexDescriptors = [
    described(0, 1, 2, hid(1, 8)),
    described(0, 1, 6, hid(2, 34, 2)),
    described(0, 0x0c, 1, hid(3, 5)),
    described(1, 0xffc0, 1, hid(0, 64, 64, 642)),
    described(2, 1, 2, hid(1, 8)),
    described(2, 1, 6, hid(2, 34, 2)),
    described(2, 0x0c, 1, hid(3, 5)),
    described(3, 1, 2, hid(2, 6)),
    described(3, 0x0c, 1, hid(1, 5)),
    described(4, 0xffc1, 1, hid(0, 64)),
  ];

  it("is a keyboard when the keyboard collection that shares the interface with a mouse one has indicator lights", () => {
    const keyboard = apex(apexDescriptors);
    expect(deviceKind(keyboard)).toBe("Keyboard");
    expect(deviceDetails(keyboard, "0.9.6", "windows x86_64")).toContain("Kind: Keyboard\n");
    expect(url(keyboard).searchParams.get("device-type")).toBe("Keyboard");
    expect(url(keyboard).searchParams.get("brand")).toBe("SteelSeries");
  });

  it("is other when the same two collections have no output report, or the descriptors were not read", () => {
    const noLights = apexDescriptors.map((d) => ({ ...d, collections: d.collections.map((c) => ({ ...c, reports: c.reports.map((r) => ({ ...r, output: 0 })) })) }));
    const unread = apexDescriptors.map((d) => ({ interface: d.interface, hex: null, collections: null }));
    for (const descriptors of [noLights, unread, [], [{ interface: 0, hex: "0501", collections: null }]]) {
      const unsure = apex(descriptors);
      expect(deviceKind(unsure)).toBe("Other");
      expect(deviceDetails(unsure, "0.9.6", "windows x86_64")).toContain("Kind: Other\n");
      // The form keeps its own choice: nothing is filled in for the device type.
      expect(url(unsure).searchParams.has("device-type")).toBe(false);
    }
  });

  it("only looks at the lights of the interface where the mouse and the keyboard are together", () => {
    // Lights on interface 2's keyboard collection do not make interface 0 (no lights) a keyboard's.
    const elsewhere = apexDescriptors.map((d) => (d.interface === 0 ? { ...d, collections: d.collections.map((c) => ({ ...c, reports: c.reports.map((r) => ({ ...r, output: 0 })) })) } : d));
    expect(deviceKind(apex(elsewhere))).toBe("Other");
    // Mouse and keyboard together on an interface that is not the first one: the first one decides, as before.
    expect(deviceKind(device("Foo", [[0, 1, 2], [1, 1, 2], [1, 1, 6]]))).toBe("Mouse");
    expect(deviceKind(device("Foo", [[0, 1, 6], [1, 1, 6], [1, 1, 2]]))).toBe("Keyboard");
  });

  it("is a mouse or a keyboard when the lowest interface has only that, with or without descriptors", () => {
    const withLights = [described(0, 1, 6, hid(2, 34, 2)), described(1, 1, 2, hid(1, 8))];
    expect(deviceKind({ ...device("Foo", [[0, 1, 6], [1, 1, 2]]), descriptors: withLights })).toBe("Keyboard");
    expect(deviceKind({ ...device("Foo", [[0, 1, 2], [1, 1, 6]]), descriptors: [] })).toBe("Mouse");
    // A device the system gives no interface numbers for (Bluetooth) has all its collections on interface -1.
    expect(deviceKind(device("Foo", [[-1, 1, 2], [-1, 1, 6]]))).toBe("Other");
    expect(deviceKind({ ...device("Foo", [[-1, 1, 2], [-1, 1, 6]]), descriptors: [described(-1, 1, 6, hid(0, 8, 1))] })).toBe("Keyboard");
  });
});
