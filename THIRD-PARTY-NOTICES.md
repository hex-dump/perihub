# Third-party notices

PeriHub is released under the MIT license (`LICENSE`). Parts of it come from other projects under
licenses that ask for their notices to go with every copy. They are listed here, and this file
goes with every PeriHub package.

## Taken from permissively licensed projects

### libratbag

<https://github.com/libratbag/libratbag>, MIT license. PeriHub takes from it:

- the list of Logitech mice and their HID++ details, and the Logitech G300's profile format;
- the ASUS ROG and TUF mouse table (`data/devices/asus-*.device`) and the ASUS protocol
  (`src/asus.c`, `src/asus.h`, `src/driver-asus.c`, Copyright (C) 2021 Kyoken), rewritten in Rust
  in `crates/perihub-core/src/peripheral/asus.rs` (see `docs/protocols/asus-rog.md`);
- the Glorious mouse table (`data/devices/glorious-*.device`) and the SinoWealth protocol
  (`src/driver-sinowealth.c`, `src/driver-sinowealth.h`, Copyright © 2020 Marian Beermann), rewritten
  in Rust in `crates/perihub-core/src/peripheral/sinowealth.rs` (see
  `docs/protocols/sinowealth-glorious.md`);
- the Logitech G600's profile format (`src/driver-logitech-g600.c`, Copyright © 2018 Thomas Hindoe
  Paaboel Andersen), rewritten in Rust in `crates/perihub-core/src/peripheral/logitech_g600.rs` (see
  `docs/protocols/logitech-g600.md`);
- the Roccat Kone Pure, Kone XTD and Kone EMP settings (`src/driver-roccat-kone-pure.c`, `src/driver-roccat.c`,
  Copyright © 2015 Red Hat, Inc., and `src/driver-roccat-kone-emp.c`, Copyright © 2021 Alexandre Laurent),
  read in `crates/perihub-core/src/peripheral/roccat.rs` (see `docs/protocols/roccat-kone.md`).

```
Copyright © 2015-2017 Red Hat, Inc.
Copyright © 2015 David Herrmann <dh.herrmann@gmail.com>
Copyright (C) 2021 Kyoken, kyoken@kyoken.ninja
Copyright © 2020 Marian Beermann
Copyright © 2018 Thomas Hindoe Paaboel Andersen
Copyright © 2021 Alexandre Laurent

Permission is hereby granted, free of charge, to any person obtaining a
copy of this software and associated documentation files (the "Software"),
to deal in the Software without restriction, including without limitation
the rights to use, copy, modify, merge, publish, distribute, sublicense,
and/or sell copies of the Software, and to permit persons to whom the
Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice (including the next
paragraph) shall be included in all copies or substantial portions of the
Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.  IN NO EVENT SHALL
THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
DEALINGS IN THE SOFTWARE.
```

### apex-3-tkl

<https://github.com/gorkemgun/apex-3-tkl>, MIT license: the SteelSeries Apex 3 TKL key mapping.

```
Copyright (c) 2026 gorkemgun

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

### pulsar-mouse-linux

<https://github.com/packerlschupfer/pulsar-mouse-linux> (commit 037cefe), MIT license: the Pulsar mice on
their cable, their USB ids, profiles and DPI ranges, and the Sonix protocol (`src/pulsar_mouse/drivers/x2a.py`
and the other models' drivers), rewritten in Rust in `crates/perihub-core/src/peripheral/pulsar.rs` (see
`docs/protocols/pulsar-sonix.md`). What each of their USB interfaces carries comes from the protocol notes of
[Pulsar-X3-Web-Config](https://github.com/santoshpanna/Pulsar-X3-Web-Config) (MIT, Copyright (c) 2026 Santosh
Panna); nothing else was taken from it.

```
Copyright (c) 2026 Packerlschupfer

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

### rivalcfg

<https://github.com/flozz/rivalcfg> (commit 779ea52), WTFPL, which asks for nothing: credited out of
courtesy. PeriHub takes from it the SteelSeries mouse table and commands
(\`crates/perihub-core/src/peripheral/steelseries_models.rs\`, generated by \`tools/rivalcfg_to_rust.py\`)
and the reports its tests expect (\`crates/perihub-core/tests/fixtures/rivalcfg_vectors.json\`).

```
            DO WHAT THE FUCK YOU WANT TO PUBLIC LICENSE
                    Version 2, December 2004

 Copyright (C) 2004 Sam Hocevar <sam@hocevar.net>

 Everyone is permitted to copy and distribute verbatim or modified
 copies of this license document, and changing it is allowed as long
 as the name is changed.

            DO WHAT THE FUCK YOU WANT TO PUBLIC LICENSE
   TERMS AND CONDITIONS FOR COPYING, DISTRIBUTION AND MODIFICATION

  0. You just DO WHAT THE FUCK YOU WANT TO.
```

## Facts only, no code

These projects are under the GPL. PeriHub took only facts from them (commands, byte layouts, USB ids,
interface numbers), described in its own words in `docs/protocols`, and wrote its own code. No code
was copied.

- [OpenRGB](https://gitlab.com/CalcProgrammer1/OpenRGB) (GPL-2.0-or-later): the Apex 8-zone lighting
  protocol, the interface and usage page of each ASUS mouse's configuration collection, and for the
  Glorious mice the usage page of theirs, how their reports are split over collections on Windows, and
  that a reading right after a write is incomplete, and the interface and usage page of the Logitech
  G600's.
- The [Linux kernel](https://github.com/torvalds/linux) (GPL-2.0): that the Roccat Kone mice are configured
  through their boot mouse interface (`drivers/hid/hid-roccat-konepure.c`, `hid-roccat-koneplus.c`).
- [OpenRazer](https://github.com/openrazer/openrazer) (GPL-2.0): the Razer report layout and commands, and per mouse
  its USB interface, transaction ids and maximum DPI, read as text by `tools/openrazer_facts.py` (see
  `docs/protocols/razer-mice.md`).
- [ckb-next](https://github.com/ckb-next/ckb-next) (GPL-2.0-only) and
  [OpenLinkHub](https://github.com/jurkovic-nikola/OpenLinkHub) (GPL-3.0): Corsair's protocol.
- [QMK](https://github.com/qmk/qmk_firmware) (GPL-2.0-or-later), [vial-qmk](https://github.com/vial-kb/vial-qmk)
  (GPL-2.0) and [vial-gui](https://github.com/vial-kb/vial-gui) (GPL-2.0): the VIA and Vial commands, the
  lighting values and VialRGB's effect ids, the raw HID collection and interface of QMK keyboards, and the
  serial numbers Vial uses to find its keyboards (see `docs/protocols/qmk-via-vial.md`).
