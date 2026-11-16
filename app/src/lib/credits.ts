// Who PeriHub takes code, data or facts from. THIRD-PARTY-NOTICES.md is the full text that goes with
// every package, with the license texts; this is what the Credits dialog in Settings shows, so
// every project listed there is listed here too (credits.test.ts checks it).

export interface Credit {
  name: string;
  /** Where the project lives. Shown as text: the app has no way to open a website. */
  link: string;
  /** The license's name, as the project gives it, with how PeriHub uses it when that matters. */
  license: string;
  /** Copyright lines the license asks to keep with every copy. */
  copyright?: string[];
  /** What PeriHub takes from it, one line each. */
  notes: string[];
}

export interface CreditGroup {
  id: string;
  title: string;
  intro?: string;
  credits: Credit[];
}

/** Said under the facts-only group, as in THIRD-PARTY-NOTICES.md. */
export const FACTS_ONLY =
  "These projects are under the GPL. PeriHub took only facts from them (commands, byte layouts, USB ids, interface numbers), described in its own words in docs/protocols, and wrote its own code. No code was copied.";

export const NOT_AFFILIATED =
  "PeriHub is not affiliated with SteelSeries, Logitech, Corsair, ASUS, Razer, Glorious, Roccat, Pulsar, Lamzu, Cooler Master or EVGA; product names belong to their owners.";

export const OWN_LICENSE = "PeriHub is released under the MIT license.";

export const NOTICES_FILE =
  "The license texts are in THIRD-PARTY-NOTICES.md, which comes with every PeriHub package.";

const NO_CODE = "No code was copied.";

export const creditGroups: CreditGroup[] = [
  {
    id: "taken",
    title: "Code and data taken from permissively licensed projects",
    intro: "These licenses ask for their notices to go with every copy of PeriHub.",
    credits: [
      {
        name: "libratbag",
        link: "https://github.com/libratbag/libratbag",
        license: "MIT license",
        copyright: [
          "Copyright © 2015-2017 Red Hat, Inc.",
          "Copyright © 2015 David Herrmann <dh.herrmann@gmail.com>",
          "Copyright (C) 2021 Kyoken, kyoken@kyoken.ninja",
          "Copyright © 2020 Marian Beermann",
          "Copyright © 2018 Thomas Hindoe Paaboel Andersen",
          "Copyright © 2021 Alexandre Laurent",
        ],
        notes: [
          "The list of Logitech mice and their HID++ details, and the Logitech G300's profile format.",
          "The ASUS ROG and TUF mouse table and the ASUS protocol (Copyright (C) 2021 Kyoken), rewritten in Rust (docs/protocols/asus-rog.md).",
          "The Glorious mouse table and the SinoWealth protocol (Copyright © 2020 Marian Beermann), rewritten in Rust (docs/protocols/sinowealth-glorious.md).",
          "The Logitech G600's profile format (Copyright © 2018 Thomas Hindoe Paaboel Andersen), rewritten in Rust (docs/protocols/logitech-g600.md).",
          "The Roccat Kone Pure, Kone XTD and Kone EMP settings (Copyright © 2015 Red Hat, Inc., and Copyright © 2021 Alexandre Laurent), read in Rust (docs/protocols/roccat-kone.md).",
          "The HID++ 1.0 registers, memory pages and profile layouts of the Logitech G9, G9x, G500, G500s, G700 and G700s (Copyright © 2015-2017 Red Hat, Inc.), rewritten in Rust (docs/protocols/logitech-hidpp10.md).",
          "The extended DPI (0x2202) and polling rate (0x8061) features and the layout of Logitech profile format 7, from libratbag's open pull requests #1676 (Nathan Rossi) and #1806 (nt-slug), contributed under the project's MIT license (docs/protocols/logitech-onboard-profiles.md).",
        ],
      },
      {
        name: "apex-3-tkl (gorkemgun)",
        link: "https://github.com/gorkemgun/apex-3-tkl",
        license: "MIT license",
        copyright: ["Copyright (c) 2026 gorkemgun"],
        notes: ["The SteelSeries Apex 3 TKL key mapping."],
      },
      {
        name: "pulsar-mouse-linux",
        link: "https://github.com/packerlschupfer/pulsar-mouse-linux",
        license: "MIT license",
        copyright: ["Copyright (c) 2026 Packerlschupfer"],
        notes: [
          "Commit 037cefe: the Pulsar mice on their cable, their USB ids, profiles and DPI ranges, and the Sonix protocol, rewritten in Rust (docs/protocols/pulsar-sonix.md).",
          "The CrazyLight mice's protocol, rewritten in Rust (docs/protocols/nordic-mice.md).",
        ],
      },
      {
        name: "Pulsar-X3-Web-Config",
        link: "https://github.com/santoshpanna/Pulsar-X3-Web-Config",
        license: "MIT license",
        copyright: ["Copyright (c) 2026 Santosh Panna"],
        notes: ["What each USB interface of the Pulsar mice carries, from its protocol notes. Nothing else was taken from it."],
      },
      {
        name: "lamzu-cfg",
        link: "https://github.com/LeadSun/lamzu-cfg",
        license: "MIT or Apache-2.0, taken under MIT",
        copyright: ["Copyright (c) 2024 Contributors of Project lamzu-cfg"],
        notes: [
          "Commit 51c0879: the Lamzu Atlantis's USB ids, frames, checksum and addresses, read in Rust (docs/protocols/nordic-mice.md).",
        ],
      },
      {
        name: "rivalcfg",
        link: "https://github.com/flozz/rivalcfg",
        license: "WTFPL, which asks for nothing: credited out of courtesy",
        copyright: ["Copyright (C) 2004 Sam Hocevar <sam@hocevar.net>"],
        notes: [
          "Commit 779ea52: the SteelSeries mouse table and commands, and the reports its tests expect.",
        ],
      },
    ],
  },
  {
    id: "facts",
    title: "Protocol documentation: facts only, no code",
    intro: FACTS_ONLY,
    credits: [
      {
        name: "OpenRGB",
        link: "https://gitlab.com/CalcProgrammer1/OpenRGB",
        license: "GPL-2.0-or-later",
        notes: [
          "The Apex 8-zone lighting protocol, the interface and usage page of each ASUS mouse's configuration collection, and for the Glorious mice the usage page of theirs, how their reports are split over collections on Windows, and that a reading right after a write is incomplete, and the interface and usage page of the Logitech G600's. " + NO_CODE,
        ],
      },
      {
        name: "OpenRGB, issues 1782 and 2305",
        link: "https://gitlab.com/CalcProgrammer1/OpenRGB",
        license: "GPL-2.0-or-later",
        notes: [
          "The USB captures of Cooler Master's app that users attached there, read as data, and which product ids form the Cooler Master mouse family (docs/protocols/coolermaster-mm.md). " + NO_CODE,
        ],
      },
      {
        name: "OpenRGB, EVGA drivers (Controllers/EVGAUSBController)",
        link: "https://gitlab.com/CalcProgrammer1/OpenRGB",
        license: "GPL-2.0-or-later",
        notes: [
          "The EVGA X12 and X20 USB ids, interfaces and lighting reports (docs/protocols/evga-mice.md). " + NO_CODE,
        ],
      },
      {
        name: "Linux kernel",
        link: "https://github.com/torvalds/linux",
        license: "GPL-2.0",
        notes: [
          "That the Roccat Kone mice are configured through their boot mouse interface (drivers/hid/hid-roccat-konepure.c, hid-roccat-koneplus.c), and the product ids of the Logitech receivers (drivers/hid/hid-logitech-dj.c). " + NO_CODE,
        ],
      },
      {
        name: "Solaar",
        link: "https://github.com/pwr-Solaar/Solaar",
        license: "GPL-2.0",
        notes: [
          "The Logitech receiver ids, and how it reads and writes onboard profile lists and button entries, compared with libratbag's (docs/protocols/logitech-onboard-profiles.md); that the Logitech G9, G9x, G500, G500s, G700 and G700s speak HID++ 1.0 on USB interface 1, and how it codes the DPI of the G9 and G9x (docs/protocols/logitech-hidpp10.md). " + NO_CODE,
        ],
      },
      {
        name: "linuxhw/LsUSB",
        link: "https://github.com/linuxhw/LsUSB",
        license: "Dumps read as data",
        notes: ["Which interfaces the Logitech G600 and the Roccat Kone mice have, from its lsusb -v dumps. " + NO_CODE],
      },
      {
        name: "OpenRazer",
        link: "https://github.com/openrazer/openrazer",
        license: "GPL-2.0",
        notes: [
          "The Razer report layout and commands, and per mouse its USB interface, transaction ids, maximum DPI, lights with a brightness and custom frame width, read as text by tools/openrazer_facts.py (docs/protocols/razer-mice.md). " + NO_CODE,
        ],
      },
      {
        name: "ckb-next",
        link: "https://github.com/ckb-next/ckb-next",
        license: "GPL-2.0-only",
        notes: ["Corsair's protocol. " + NO_CODE],
      },
      {
        name: "OpenLinkHub",
        link: "https://github.com/jurkovic-nikola/OpenLinkHub",
        license: "GPL-3.0",
        notes: ["Corsair's protocol. " + NO_CODE],
      },
      {
        name: "QMK",
        link: "https://github.com/qmk/qmk_firmware",
        license: "GPL-2.0-or-later",
        notes: [
          "The VIA and Vial commands, the lighting values and VialRGB's effect ids, the raw HID collection and interface of QMK keyboards, and the serial numbers Vial uses to find its keyboards (docs/protocols/qmk-via-vial.md). " + NO_CODE,
        ],
      },
      {
        name: "vial-qmk",
        link: "https://github.com/vial-kb/vial-qmk",
        license: "GPL-2.0",
        notes: [
          "The VIA and Vial commands, the lighting values and VialRGB's effect ids, the raw HID collection and interface of QMK keyboards, and the serial numbers Vial uses to find its keyboards (docs/protocols/qmk-via-vial.md). " + NO_CODE,
        ],
      },
      {
        name: "vial-gui",
        link: "https://github.com/vial-kb/vial-gui",
        license: "GPL-2.0",
        notes: [
          "The VIA and Vial commands, the lighting values and VialRGB's effect ids, the raw HID collection and interface of QMK keyboards, and the serial numbers Vial uses to find its keyboards (docs/protocols/qmk-via-vial.md). " + NO_CODE,
        ],
      },
    ],
  },
];
