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

/** Said under the group of tools downloaded at run time, as in THIRD-PARTY-NOTICES.md. */
export const RUNTIME_ONLY =
  "Not part of PeriHub and not shipped with it: downloaded from the project's own release only when you use the feature that needs it, checked against a pinned SHA-256, and removed afterwards.";

/** Said under the facts-only group, as in THIRD-PARTY-NOTICES.md. */
export const FACTS_ONLY =
  "These projects are under the GPL. PeriHub took only facts from them (commands, byte layouts, USB ids, interface numbers), described in its own words in docs/protocols, and wrote its own code. No code was copied.";

export const NOT_AFFILIATED =
  "PeriHub is not affiliated with SteelSeries, Logitech, Corsair, ASUS, Razer, Glorious, Roccat, Pulsar, Lamzu, Cooler Master, EVGA or EVision; product names belong to their owners.";

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
          "The Logitech G604's ids (data/devices/logitech-g604.device) and the answers of a real G604 that its issue #1477 holds (docs/protocols/logitech-onboard-profiles.md).",
          "The ASUS ROG and TUF mouse table and the ASUS protocol (Copyright (C) 2021 Kyoken), rewritten in Rust (docs/protocols/asus-rog.md).",
          "The Glorious mouse table and the SinoWealth protocol (Copyright © 2020 Marian Beermann), rewritten in Rust (docs/protocols/sinowealth-glorious.md).",
          "The Logitech G600's profile format (Copyright © 2018 Thomas Hindoe Paaboel Andersen), rewritten in Rust (docs/protocols/logitech-g600.md).",
          "The Roccat Kone Pure, Kone XTD and Kone EMP settings (Copyright © 2015 Red Hat, Inc., and Copyright © 2021 Alexandre Laurent), read in Rust (docs/protocols/roccat-kone.md).",
          "The HID++ 1.0 registers, memory pages and profile layouts of the Logitech G9, G9x, G500, G500s, G700 and G700s (Copyright © 2015-2017 Red Hat, Inc.), rewritten in Rust (docs/protocols/logitech-hidpp10.md).",
          "The extended DPI (0x2202) and polling rate (0x8061) features and the layout of Logitech profile format 7, from libratbag's open pull requests #1676 (Nathan Rossi) and #1806 (nt-slug), contributed under the project's MIT license (docs/protocols/logitech-onboard-profiles.md).",
        ],
      },
      {
        name: "claws-mouse-linux",
        link: "https://github.com/naevui/claws-mouse-linux",
        license: "MIT license",
        copyright: ["Copyright (c) 2026 tuxdevv"],
        notes: ["(commit ed9e0e9) How the mice on the YJX receiver take their reports, the version and settings commands and the layout of the settings (docs/protocols/yjx-mice.md)."],
      },
      {
        name: "Phosphor Icons",
        link: "https://github.com/phosphor-icons/core",
        license: "MIT license",
        copyright: ["Copyright (c) 2023 Phosphor Icons"],
        notes: ["The \"Flask\" icon (regular weight, @phosphor-icons/core 2.1.1), unaltered, for the experimental state."],
      },
      {
        name: "EWEADN-GS02-Pro-Control",
        link: "https://github.com/mimuruu/EWEADN-GS02-Pro-Control",
        license: "MIT license",
        copyright: ["Copyright (c) 2026 yudhis"],
        notes: ["(commit f31692d) That some mice of the YJX platform number their reports, and the battery command (docs/protocols/yjx-mice.md)."],
      },
      {
        name: "AulaControl",
        link: "https://github.com/sgtflixy/AulaControl",
        license: "Apache License 2.0",
        copyright: [],
        notes: ["(commit 9a526ee) The configuration collection, report and commands of the AULA WIN60 HE, which the Wraith W60 shares, and how its model and lighting are read (docs/protocols/wraith-he.md)."],
      },
      {
        name: "G213Colors",
        link: "https://github.com/SebiTimeWaster/G213Colors",
        license: "MIT license",
        copyright: ["Copyright (c) 2016, 2017 TimeWaster"],
        notes: [
          "(commit 369daa2) The byte layouts of the Logitech G213's fixed colour, breathing and colour cycle commands, which its author captured from the official software, that the keyboard's answer is read after each command, that the keyboard does not keep what is sent, and the range of the times (docs/protocols/logitech-g213.md).",
        ],
      },
      {
        name: "gdevd",
        link: "https://github.com/R1tschY/gdevd",
        license: "MIT or Apache-2.0, taken under MIT",
        copyright: ["Copyright (c) 2016-2019 Sergio Benitez"],
        notes: [
          "(commit 60fcfc3) The Logitech G213's brightness byte, the layout of its wave command and the wave directions, and the command that switches its startup effect (docs/protocols/logitech-g213.md).",
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
          "The Apex 8-zone lighting protocol, the interface and usage page of each ASUS mouse's configuration collection, and for the Glorious mice the usage page of theirs, how their reports are split over collections on Windows, and that a reading right after a write is incomplete, and the interface and usage page of the Logitech G600's, and the name and USB id of a white Corsair K70 PRO MINI WIRELESS (docs/protocols/corsair-bragi.md), and the values of the Corsair keyboards' physical layout property and how it reads a Logitech keyboard's layout code (docs/protocols/keyboard-layouts.md), and the Logitech G213's USB id, the collection that takes its lighting commands, its five lighting areas, the bytes of its lighting commands and that its answer is read after each (docs/protocols/logitech-g213.md). " + NO_CODE,
        ],
      },
      {
        name: "g810-led",
        link: "https://github.com/MatMoul/g810-led",
        license: "GPL-3.0",
        notes: [
          "(commit efa3c35) The same lighting commands as sent to the Logitech G213: the numbers of the effect groups and of the wave variants, that the keyboard needs no \"commit\", and the byte of an effect that stores it (docs/protocols/logitech-g213.md). " + NO_CODE,
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
        name: "OpenRGB, SteelSeries Apex controller (Controllers/SteelSeriesController)",
        link: "https://gitlab.com/CalcProgrammer1/OpenRGB",
        license: "GPL-2.0-or-later",
        notes: [
          "Commit 6b681e3c: the USB ids and interface of the Apex Pro, Apex 7, Apex 7 TKL and Apex 5, the report sizes, the firmware, brightness, colour and back-to-the-keyboard's-own-lighting commands, their answers, and the keys of the colour frame (docs/protocols/steelseries-apex.md). " + NO_CODE,
        ],
      },
      {
        name: "apex-tux",
        link: "https://github.com/not-jan/apex-tux",
        license: "Unlicense",
        notes: [
          "Commit d0911fe: the picture the display of the Apex Pro, Apex 7 and Apex 5 takes (128 x 40 pixels, one bit each) and the feature report it is sent in (docs/protocols/steelseries-apex.md). Public domain, credited although nothing requires it. " + NO_CODE,
        ],
      },
      {
        name: "OpenRGB, EVision keyboard controllers (Controllers/EVisionKeyboardController)",
        link: "https://gitlab.com/CalcProgrammer1/OpenRGB",
        license: "GPL-2.0-or-later",
        notes: [
          "Commit 91714d1: the EVision keyboards' USB ids, interface and usage page, report format and checksum, the configuration read and write commands, the lighting header and its modes, and the layout of the newer controller's profiles (docs/protocols/evision.md). " + NO_CODE,
        ],
      },
      {
        name: "OpenEVision",
        link: "https://github.com/iefedev/OpenEVision",
        license: "GPL-2.0",
        notes: [
          "Commit 1cca304: what it checked on a 320f:505b: the reads and writes of the configuration memory, the layout of the lighting header and of the key table, the regions it never writes, and that every program with the collection open receives every answer (docs/protocols/evision.md). " + NO_CODE,
        ],
      },
      {
        name: "Linux kernel",
        link: "https://github.com/torvalds/linux",
        license: "GPL-2.0",
        notes: [
          "That the Roccat Kone mice are configured through their boot mouse interface (drivers/hid/hid-roccat-konepure.c, hid-roccat-koneplus.c), and the product ids of the Logitech receivers (drivers/hid/hid-logitech-dj.c), and how a HID++ 2.0 request and an error answer are laid out (drivers/hid/hid-logitech-hidpp.c, docs/protocols/logitech-g213.md). " + NO_CODE,
        ],
      },
      {
        name: "Solaar",
        link: "https://github.com/pwr-Solaar/Solaar",
        license: "GPL-2.0",
        notes: [
          "The Logitech receiver ids, and how it reads and writes onboard profile lists and button entries, compared with libratbag's (docs/protocols/logitech-onboard-profiles.md); that a Logitech device on Bluetooth Low Energy is asked on device index 0xFF and may answer on 0x00 (same page); that the Logitech G9, G9x, G500, G500s, G700 and G700s speak HID++ 1.0 on USB interface 1, and how it codes the DPI of the G9 and G9x (docs/protocols/logitech-hidpp10.md); how it reads a Logitech keyboard's layout code, HID++ 0x4540 (docs/protocols/keyboard-layouts.md); what the HID++ 2.0 root feature answers (docs/protocols/logitech-g213.md); the functions and replies of the HID++ 2.0 lighting features 0x8040, 0x8071, 0x8081 and 0x4540, how a program takes a keyboard's lights over and gives them back, and the G HUB captures and hardware tests in its pull request 3149 (docs/protocols/logitech-keyboard-lighting.md). " + NO_CODE,
        ],
      },
      {
        name: "logi-ghub-linux",
        link: "https://github.com/theo-antonescu/logi-ghub-linux",
        license: "GPL-2.0",
        notes: [
          "Its notes of a USB capture of Logitech G HUB on a G915 X and of its tests on a G915 X LS TKL: host mode through 0x8101, the 0x8071 claim with both activity notices, and the light timers G HUB writes (docs/protocols/logitech-keyboard-lighting.md). " + NO_CODE,
        ],
      },
      {
        name: "logirgb",
        link: "https://github.com/mevdschee/logirgb",
        license: "MIT",
        notes: [
          "Its copy of Logitech's public HID++ 2.0 feature specifications \"x8071 – RGB Effects\", \"x8081 – Per-Key Lighting\" and \"x8040 – Brightness Control\": the byte layouts of the effects, power modes, activity notices and per-key colours (docs/protocols/logitech-keyboard-lighting.md). " + NO_CODE,
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
          "The Razer report layout and commands, and per mouse its USB interface, transaction ids, maximum DPI, lights with a brightness and custom frame width, read as text by tools/openrazer_facts.py (docs/protocols/razer-mice.md); per keyboard its USB interface, waits and transaction ids, and the battery, charging, brightness and game mode reads, read as text by the same tool, and for the BlackWidow V4 Mini HyperSpeed also the commands that change the brightness, the game mode and the macro light, those that set its lighting effects (with their values, speeds and storage) and draw a custom frame, and the size of its matrix (docs/protocols/razer-keyboards.md); the keyboard layout read and its region codes (docs/protocols/keyboard-layouts.md). " + NO_CODE,
        ],
      },
      {
        name: "OpenMouse mouse-protocol",
        link: "https://github.com/OpenMouse-Project/mouse-protocol",
        license: "AGPL-3.0",
        notes: [
          "The battery request of the YJX mice and where their settings hold the lift-off distance (docs/protocols/yjx-mice.md). " + NO_CODE,
        ],
      },
      {
        name: "ckb-next",
        link: "https://github.com/ckb-next/ckb-next",
        license: "GPL-2.0-only",
        notes: ["Corsair's protocol, and the keyboards' physical layout property (docs/protocols/keyboard-layouts.md). " + NO_CODE],
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
  {
    id: "runtime",
    title: "Tools downloaded when a feature needs them",
    intro: RUNTIME_ONLY,
    credits: [
      {
        name: "USBPcap",
        link: "https://github.com/desowin/usbpcap",
        license: "GPL-2.0 driver, BSD-2-Clause USBPcapCMD",
        notes: [
          "Version 1.5.4.0, by Tomasz Moń: on Windows, Send a report → Record the official software downloads its installer from the project's GitHub release, installs it for the recording only and removes it completely after it. PeriHub runs USBPcapCMD and reads what it captures. " + NO_CODE,
        ],
      },
    ],
  },
];
