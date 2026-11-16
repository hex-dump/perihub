import { describe, expect, it } from "vitest";
import notices from "../../../THIRD-PARTY-NOTICES.md?raw";
import { FACTS_ONLY, NOT_AFFILIATED, creditGroups } from "./credits";

const all = creditGroups.flatMap((g) => g.credits);
const shown = JSON.stringify(creditGroups) + NOT_AFFILIATED;

const section = (title: string) => {
  const start = notices.indexOf(`## ${title}`);
  expect(start, title).toBeGreaterThan(-1);
  const next = notices.indexOf("\n## ", start + 1);
  return notices.slice(start, next < 0 ? undefined : next);
};

describe("credits", () => {
  it("lists every project of the permissive section of THIRD-PARTY-NOTICES.md, with link, license and copyright", () => {
    const taken = section("Taken from permissively licensed projects");
    const blocks = taken.split(/^### /m).slice(1);
    expect(blocks.length).toBeGreaterThanOrEqual(5);
    for (const block of blocks) {
      const heading = block.split("\n")[0].trim();
      const url = block.match(/<(https:\/\/[^>]+)>/)![1];
      const credit = all.find((c) => c.link === url);
      expect(credit, heading).toBeDefined();
      expect(credit!.name.toLowerCase(), heading).toContain(heading.toLowerCase());
      // The license named in the first sentence, e.g. "MIT license", "WTFPL".
      const license = block.match(/>[^,]*, ([^:.]*?)(?:[:.]| PeriHub takes)/)?.[1] ?? "";
      expect(license, heading).not.toBe("");
      expect(credit!.license.toLowerCase(), heading).toContain(license.split(" ")[0].toLowerCase());
      for (const line of block.match(/^Copyright .*$/gm) ?? []) {
        expect(credit!.copyright, `${heading}: ${line}`).toContain(line);
      }
      // Pinned commits stay with their project.
      for (const [, commit] of block.matchAll(/commit ([0-9a-f]{7})/g)) {
        expect(credit!.notes.join(" "), heading).toContain(commit);
      }
    }
  });

  it("lists every copyright line, linked project and protocol note of THIRD-PARTY-NOTICES.md", () => {
    const allCopyright = all.flatMap((c) => c.copyright ?? []);
    for (const line of notices.match(/^ ?Copyright .*$/gm) ?? []) {
      expect(allCopyright, line).toContain(line.trim());
    }
    // Other projects named in the notes: Pulsar-X3-Web-Config and its copyright.
    for (const [, name, url, license] of notices.matchAll(/\[([^\]]+)\]\((https:[^)]+)\) \(([^)]*?)(?:, Copyright[^)]*)?\)/g)) {
      const credit = all.find((c) => c.link === url && c.license.includes(license.split(",")[0]));
      expect(credit, `${name} ${url} ${license}`).toBeDefined();
    }
    // Every protocol note that the notices point to is still pointed to.
    for (const [, doc] of notices.matchAll(/(docs\/protocols\/[\w-]+\.md)/g)) {
      expect(shown, doc).toContain(doc);
    }
  });

  it("lists the GPL projects with the facts taken from them, and says no code was copied", () => {
    const facts = section("Facts only, no code");
    expect(facts).toContain(FACTS_ONLY.replace(/^.*?PeriHub took/, "PeriHub took").slice(0, 40));
    const group = creditGroups.find((g) => g.id === "facts")!;
    expect(group.intro).toBe(FACTS_ONLY);
    expect(FACTS_ONLY.endsWith("No code was copied.")).toBe(true);
    for (const credit of group.credits) {
      expect(credit.notes.join(" "), credit.name).toContain("No code was copied.");
      expect(credit.license, credit.name).not.toBe("");
    }
    // Each link of the section, with its license, is an entry; so is each name.
    const links = [...facts.matchAll(/\[([^\]]+)\]\((https:[^)]+)\)\s*\(([^)]+)\)/g)];
    expect(links.length).toBeGreaterThanOrEqual(10);
    for (const [, name, url, license] of links) {
      const credit = all.find((c) => c.link === url && c.license === license);
      expect(credit, `${name} ${license}`).toBeDefined();
    }
    for (const name of ["Linux kernel", "Solaar", "linuxhw/LsUSB", "OpenRazer", "ckb-next", "OpenLinkHub", "QMK", "vial-qmk", "vial-gui"]) {
      expect(all.map((c) => c.name), name).toContain(name);
    }
    expect(all.filter((c) => c.name.startsWith("OpenRGB")).length).toBe(3);
    // The facts of each bullet are still said in the entry's note.
    for (const phrase of [
      "Apex 8-zone lighting",
      "usage page",
      "Cooler Master",
      "issues 1782 and 2305",
      "EVGA X12 and X20",
      "boot mouse interface",
      "hid-logitech-dj.c",
      "onboard profile lists",
      "HID++ 1.0 on USB interface 1",
      "lsusb -v",
      "transaction ids",
      "openrazer_facts.py",
      "VialRGB's effect ids",
      "serial numbers Vial uses",
    ]) {
      expect(shown, phrase).toContain(phrase);
    }
  });

  it("keeps every attribution the old credits text of Settings had", () => {
    const names = [
      "OpenRGB", "libratbag", "gorkemgun/apex-3-tkl", "ckb-next", "OpenLinkHub", "OpenRazer", "rivalcfg",
      "QMK", "Vial", "pulsar-mouse-linux", "Pulsar-X3-Web-Config", "lamzu-cfg", "Cooler Master", "EVGA",
    ];
    for (const name of names) expect(shown, name).toContain(name);
    for (const vendor of ["SteelSeries", "Logitech", "Corsair", "ASUS", "Razer", "Glorious", "Roccat", "Pulsar", "Lamzu", "Cooler Master", "EVGA"]) {
      expect(NOT_AFFILIATED, vendor).toContain(vendor);
    }
    expect(NOT_AFFILIATED).toContain("not affiliated");
  });

  it("gives every entry a name, a link, a license and a note, once", () => {
    for (const credit of all) {
      expect(credit.name).not.toBe("");
      expect(credit.link).toMatch(/^https:\/\//);
      expect(credit.license).not.toBe("");
      expect(credit.notes.length).toBeGreaterThan(0);
    }
    const names = all.map((c) => c.name);
    expect(new Set(names).size).toBe(names.length);
  });
});
