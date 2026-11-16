import { describe, expect, it } from "vitest";

import { mergeHidden, redact, userNames } from "./redact";

describe("userNames", () => {
  it("takes the user name from the folders PeriHub shows", () => {
    expect(userNames(["/home/ann/.config/perihub", "C:\\Users\\Ann Smith\\AppData\\Roaming\\perihub", "/Users/bob/Library"])).toEqual([
      "ann",
      "Ann Smith",
      "bob",
    ]);
  });

  it("leaves out names too short to hide safely, and paths without one", () => {
    expect(userNames(["/home/me/x", "/etc/perihub", ""])).toEqual([]);
  });
});

describe("redact", () => {
  it("hides the user name in paths of both systems", () => {
    const { text, hidden } = redact("log: /home/ann/.local/share/perihub.log, C:\\Users\\Ann Smith\\AppData\\x, /Users/bob/x");
    expect(text).toBe("log: /home/[user]/.local/share/perihub.log, C:\\Users\\[user]\\AppData\\x, /Users/[user]/x");
    expect(hidden).toEqual([{ what: "user name in a path", count: 3 }]);
  });

  it("hides a user name wherever it stands, as a whole word", () => {
    const { text, hidden } = redact("Owner ann: annex and Ann.", ["ann"]);
    expect(text).toBe("Owner [user]: annex and [user].");
    expect(hidden).toEqual([{ what: "user name", count: 2 }]);
  });

  it("hides serial numbers and unit ids after their label", () => {
    expect(redact("Serial number: AB12CD34EF").text).toBe("Serial number: [id hidden]");
    expect(redact("serial 0A1B2C3D4E5F").text).toBe("serial [id hidden]");
    expect(redact("S/N=XYZ-1234").text).toBe("S/N=[id hidden]");
    expect(redact("unit id: 12 34 56 78, firmware 1.2").text).toBe("unit id: [id hidden], firmware 1.2");
  });

  it("leaves what the core already hid, and labels with nothing after them", () => {
    const text = "Device info (0x0003): 02 xx xx xx xx 00 02 12 34\nunit id: xx xx xx xx\nserial: not read\nSerial number is not included";
    const result = redact(text);
    expect(result.text).toBe(text);
    expect(result.hidden).toEqual([]);
  });

  it("hides hardware addresses, unique ids, email addresses and long ids", () => {
    const { text, hidden } = redact(
      "Bluetooth DC:2C:26:AB:CD:EF, id 123e4567-e89b-12d3-a456-426614174000, ann@example.org, token 0123456789ab",
    );
    expect(text).toBe("Bluetooth [address hidden], id [id hidden], [email hidden], token [id hidden]");
    expect(hidden.map((h) => h.what).sort()).toEqual(["email address", "hardware address", "long id", "unique id"].sort());
  });

  it("leaves what a report is made of: USB ids, byte dumps, versions and numbers", () => {
    const report = [
      "PeriHub 0.9.0 on linux x86_64",
      "USB ID: 046d:c547, interface 2, usage page ff00",
      "12 04 00 00 03 00 → 12 04 00 00 03 00 8f 91 aa bb cc dd ee ff",
      "Features: 0001 v1, 2201 v2 (flags 40), 8100 v1",
      "Memory: model 1, 16 sectors of 256 bytes",
    ].join("\n");
    expect(redact(report, ["ann"])).toEqual({ text: report, hidden: [] });
  });

  it("does not hide a name that is only part of another word", () => {
    expect(redact("G502 Hero, Keris", ["Her"]).text).toBe("G502 Hero, Keris");
  });
});

describe("mergeHidden", () => {
  it("adds up what several texts hid", () => {
    expect(
      mergeHidden([
        [{ what: "a", count: 1 }],
        [
          { what: "a", count: 2 },
          { what: "b", count: 1 },
        ],
        [],
      ]),
    ).toEqual([
      { what: "a", count: 3 },
      { what: "b", count: 1 },
    ]);
  });
});
