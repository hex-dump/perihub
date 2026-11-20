# Third-party notices

PeriHub is released under the MIT license (`LICENSE`). Parts of it come from other projects under
licenses that ask for their notices to go with every copy. They are listed here, and this file
goes with every PeriHub package.

## Taken from permissively licensed projects

### libratbag

<https://github.com/libratbag/libratbag>, MIT license. PeriHub takes from it:

- the list of Logitech mice and their HID++ details, and the Logitech G300's profile format;
- the Logitech G604's ids (`data/devices/logitech-g604.device`) and the answers of a real G604 that its issue #1477 holds
  (`crates/perihub-core/tests/fixtures/g604`, see `docs/protocols/logitech-onboard-profiles.md`);
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
  read in `crates/perihub-core/src/peripheral/roccat.rs` (see `docs/protocols/roccat-kone.md`);
- the HID++ 1.0 registers, memory pages and profile layouts of the Logitech G9, G9x, G500, G500s, G700 and
  G700s (`src/hidpp10.c`, `src/hidpp10.h`, `src/driver-hidpp10.c`, `src/hidpp-generic.c` and their
  `data/devices/logitech-g*.device` files, Copyright © 2015-2017 Red Hat, Inc.), rewritten in Rust in
  `crates/perihub-core/src/peripheral/logitech_hidpp10.rs` (see `docs/protocols/logitech-hidpp10.md`);
- the extended DPI (`0x2202`) and polling rate (`0x8061`) features and the layout of Logitech profile
  format 7, read in `crates/perihub-core/src/hidpp/mod.rs` and `profile.rs` (see
  `docs/protocols/logitech-onboard-profiles.md`), from libratbag's open pull requests #1676 (Nathan
  Rossi) and #1806 (nt-slug), which are contributed under the project's MIT license;
- the functions of the HID++ 2.0 RGB effects feature 0x8071 and the layout of its device, zone and effect
  information (`src/hidpp20.c`, `src/hidpp20.h`, Copyright © 2015-2017 Red Hat, Inc.), read in
  `crates/perihub-core/src/hidpp/lighting.rs` (see `docs/protocols/logitech-keyboard-lighting.md`).

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
Panna); nothing else was taken from it. The CrazyLight mice's protocol (`src/pulsar_mouse/drivers/nordic.py`,
`x2_crazylight*.py`, `x3_lhd_crazylight.py`, `docs/protocol-x2-crazylight.md`) is rewritten in Rust in
`crates/perihub-core/src/peripheral/nordic.rs` (see `docs/protocols/nordic-mice.md`).

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

### lamzu-cfg

<https://github.com/LeadSun/lamzu-cfg> (commit 51c0879), MIT or Apache-2.0, taken under MIT: the Lamzu Atlantis's
USB ids, frames, checksum and addresses (`src/atlantis.rs`, `src/atlantis/hid.rs`), read in
`crates/perihub-core/src/peripheral/nordic.rs` (see `docs/protocols/nordic-mice.md`).

```
Copyright (c) 2024 Contributors of Project lamzu-cfg

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

### claws-mouse-linux

<https://github.com/naevui/claws-mouse-linux> (commit ed9e0e9), MIT license: how the mice on the YJX receiver
`a8a5:2255` take their reports (the configuration interface, a `55` byte in front of each request, answers that start
with `aa`), the version and settings commands, and the layout of the settings (`README.md`, `clawsctl.py`), read in
`crates/perihub-core/src/peripheral/yjx.rs` (see `docs/protocols/yjx-mice.md`).

```
MIT License

Copyright (c) 2026 tuxdevv

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

### EWEADN-GS02-Pro-Control

<https://github.com/mimuruu/EWEADN-GS02-Pro-Control> (commit f31692d), MIT license: that some mice of the same
platform number their reports (`f0`), which PeriHub does not send to, and the battery command
(`backend/hid.py`). See `docs/protocols/yjx-mice.md`.

```
MIT License

Copyright (c) 2026 yudhis

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

### AulaControl

<https://github.com/sgtflixy/AulaControl> (commit 9a526ee), Apache License 2.0: the configuration collection,
report and commands of the AULA WIN60 HE, which the Wraith W60 shares, how its model and lighting are read
(`re/protocol.md`, `cli/Protocol/AulaProtocol.cs`), read in `crates/perihub-core/src/peripheral/wraith.rs` (see
`docs/protocols/wraith-he.md`). The project has no NOTICE file. Its license:

```
                                 Apache License
                           Version 2.0, January 2004
                        http://www.apache.org/licenses/

   TERMS AND CONDITIONS FOR USE, REPRODUCTION, AND DISTRIBUTION

   1. Definitions.

      "License" shall mean the terms and conditions for use, reproduction,
      and distribution as defined by Sections 1 through 9 of this document.

      "Licensor" shall mean the copyright owner or entity authorized by
      the copyright owner that is granting the License.

      "Legal Entity" shall mean the union of the acting entity and all
      other entities that control, are controlled by, or are under common
      control with that entity. For the purposes of this definition,
      "control" means (i) the power, direct or indirect, to cause the
      direction or management of such entity, whether by contract or
      otherwise, or (ii) ownership of fifty percent (50%) or more of the
      outstanding shares, or (iii) beneficial ownership of such entity.

      "You" (or "Your") shall mean an individual or Legal Entity
      exercising permissions granted by this License.

      "Source" form shall mean the preferred form for making modifications,
      including but not limited to software source code, documentation
      source, and configuration files.

      "Object" form shall mean any form resulting from mechanical
      transformation or translation of a Source form, including but
      not limited to compiled object code, generated documentation,
      and conversions to other media types.

      "Work" shall mean the work of authorship, whether in Source or
      Object form, made available under the License, as indicated by a
      copyright notice that is included in or attached to the work
      (an example is provided in the Appendix below).

      "Derivative Works" shall mean any work, whether in Source or Object
      form, that is based on (or derived from) the Work and for which the
      editorial revisions, annotations, elaborations, or other modifications
      represent, as a whole, an original work of authorship. For the purposes
      of this License, Derivative Works shall not include works that remain
      separable from, or merely link (or bind by name) to the interfaces of,
      the Work and Derivative Works thereof.

      "Contribution" shall mean any work of authorship, including
      the original version of the Work and any modifications or additions
      to that Work or Derivative Works thereof, that is intentionally
      submitted to Licensor for inclusion in the Work by the copyright owner
      or by an individual or Legal Entity authorized to submit on behalf of
      the copyright owner. For the purposes of this definition, "submitted"
      means any form of electronic, verbal, or written communication sent
      to the Licensor or its representatives, including but not limited to
      communication on electronic mailing lists, source code control systems,
      and issue tracking systems that are managed by, or on behalf of, the
      Licensor for the purpose of discussing and improving the Work, but
      excluding communication that is conspicuously marked or otherwise
      designated in writing by the copyright owner as "Not a Contribution."

      "Contributor" shall mean Licensor and any individual or Legal Entity
      on behalf of whom a Contribution has been received by Licensor and
      subsequently incorporated within the Work.

   2. Grant of Copyright License. Subject to the terms and conditions of
      this License, each Contributor hereby grants to You a perpetual,
      worldwide, non-exclusive, no-charge, royalty-free, irrevocable
      copyright license to reproduce, prepare Derivative Works of,
      publicly display, publicly perform, sublicense, and distribute the
      Work and such Derivative Works in Source or Object form.

   3. Grant of Patent License. Subject to the terms and conditions of
      this License, each Contributor hereby grants to You a perpetual,
      worldwide, non-exclusive, no-charge, royalty-free, irrevocable
      (except as stated in this section) patent license to make, have made,
      use, offer to sell, sell, import, and otherwise transfer the Work,
      where such license applies only to those patent claims licensable
      by such Contributor that are necessarily infringed by their
      Contribution(s) alone or by combination of their Contribution(s)
      with the Work to which such Contribution(s) was submitted. If You
      institute patent litigation against any entity (including a
      cross-claim or counterclaim in a lawsuit) alleging that the Work
      or a Contribution incorporated within the Work constitutes direct
      or contributory patent infringement, then any patent licenses
      granted to You under this License for that Work shall terminate
      as of the date such litigation is filed.

   4. Redistribution. You may reproduce and distribute copies of the
      Work or Derivative Works thereof in any medium, with or without
      modifications, and in Source or Object form, provided that You
      meet the following conditions:

      (a) You must give any other recipients of the Work or
          Derivative Works a copy of this License; and

      (b) You must cause any modified files to carry prominent notices
          stating that You changed the files; and

      (c) You must retain, in the Source form of any Derivative Works
          that You distribute, all copyright, patent, trademark, and
          attribution notices from the Source form of the Work,
          excluding those notices that do not pertain to any part of
          the Derivative Works; and

      (d) If the Work includes a "NOTICE" text file as part of its
          distribution, then any Derivative Works that You distribute must
          include a readable copy of the attribution notices contained
          within such NOTICE file, excluding those notices that do not
          pertain to any part of the Derivative Works, in at least one
          of the following places: within a NOTICE text file distributed
          as part of the Derivative Works; within the Source form or
          documentation, if provided along with the Derivative Works; or,
          within a display generated by the Derivative Works, if and
          wherever such third-party notices normally appear. The contents
          of the NOTICE file are for informational purposes only and
          do not modify the License. You may add Your own attribution
          notices within Derivative Works that You distribute, alongside
          or as an addendum to the NOTICE text from the Work, provided
          that such additional attribution notices cannot be construed
          as modifying the License.

      You may add Your own copyright statement to Your modifications and
      may provide additional or different license terms and conditions
      for use, reproduction, or distribution of Your modifications, or
      for any such Derivative Works as a whole, provided Your use,
      reproduction, and distribution of the Work otherwise complies with
      the conditions stated in this License.

   5. Submission of Contributions. Unless You explicitly state otherwise,
      any Contribution intentionally submitted for inclusion in the Work
      by You to the Licensor shall be under the terms and conditions of
      this License, without any additional terms or conditions.
      Notwithstanding the above, nothing herein shall supersede or modify
      the terms of any separate license agreement you may have executed
      with Licensor regarding such Contributions.

   6. Trademarks. This License does not grant permission to use the trade
      names, trademarks, service marks, or product names of the Licensor,
      except as required for reasonable and customary use in describing the
      origin of the Work and reproducing the content of the NOTICE file.

   7. Disclaimer of Warranty. Unless required by applicable law or
      agreed to in writing, Licensor provides the Work (and each
      Contributor provides its Contributions) on an "AS IS" BASIS,
      WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or
      implied, including, without limitation, any warranties or conditions
      of TITLE, NON-INFRINGEMENT, MERCHANTABILITY, or FITNESS FOR A
      PARTICULAR PURPOSE. You are solely responsible for determining the
      appropriateness of using or redistributing the Work and assume any
      risks associated with Your exercise of permissions under this License.

   8. Limitation of Liability. In no event and under no legal theory,
      whether in tort (including negligence), contract, or otherwise,
      unless required by applicable law (such as deliberate and grossly
      negligent acts) or agreed to in writing, shall any Contributor be
      liable to You for damages, including any direct, indirect, special,
      incidental, or consequential damages of any character arising as a
      result of this License or out of the use or inability to use the
      Work (including but not limited to damages for loss of goodwill,
      work stoppage, computer failure or malfunction, or any and all
      other commercial damages or losses), even if such Contributor
      has been advised of the possibility of such damages.

   9. Accepting Warranty or Additional Liability. While redistributing
      the Work or Derivative Works thereof, You may choose to offer,
      and charge a fee for, acceptance of support, warranty, indemnity,
      or other liability obligations and/or rights consistent with this
      License. However, in accepting such obligations, You may act only
      on Your own behalf and on Your sole responsibility, not on behalf
      of any other Contributor, and only if You agree to indemnify,
      defend, and hold each Contributor harmless for any liability
      incurred by, or claims asserted against, such Contributor by reason
      of your accepting any such warranty or additional liability.

   END OF TERMS AND CONDITIONS

   APPENDIX: How to apply the Apache License to your work.

      To apply the Apache License to your work, attach the following
      boilerplate notice, with the fields enclosed by brackets "[]"
      replaced with your own identifying information. (Don't include
      the brackets!)  The text should be enclosed in the appropriate
      comment syntax for the file format. We also recommend that a
      file or class name and description of purpose be included on the
      same "printed page" as the copyright notice for easier
      identification within third-party archives.

   Copyright [yyyy] [name of copyright owner]

   Licensed under the Apache License, Version 2.0 (the "License");
   you may not use this file except in compliance with the License.
   You may obtain a copy of the License at

       http://www.apache.org/licenses/LICENSE-2.0

   Unless required by applicable law or agreed to in writing, software
   distributed under the License is distributed on an "AS IS" BASIS,
   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
   See the License for the specific language governing permissions and
   limitations under the License.
```

### G213Colors

<https://github.com/SebiTimeWaster/G213Colors> (commit 369daa2), MIT license: the byte layouts of the Logitech G213's
fixed colour, breathing and colour cycle commands, which its author captured from the official software, that the
keyboard's answer must be read after each command, that the keyboard does not keep what is sent, and the range of the
times (`G213Colors.py`, `README.md`), read in `crates/perihub-core/src/peripheral/logitech_g213.rs` (see
`docs/protocols/logitech-g213.md`).

```
MIT License

Copyright (c) 2016, 2017 TimeWaster

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

### gdevd

<https://github.com/R1tschY/gdevd> (commit 60fcfc3), MIT or Apache-2.0, taken under MIT: the Logitech G213's
brightness byte, the layout of its wave command and the wave directions, and the command that switches its startup
effect (`src/drivers/g213.rs`, `src/lib.rs`), read in `crates/perihub-core/src/peripheral/logitech_g213.rs` (see
`docs/protocols/logitech-g213.md`).

```
The MIT License (MIT)
Copyright (c) 2016-2019 Sergio Benitez

Permission is hereby granted, free of charge, to any person obtaining a copy of
this software and associated documentation files (the "Software"), to deal in
the Software without restriction, including without limitation the rights to
use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of
the Software, and to permit persons to whom the Software is furnished to do so,
subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS
FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR
COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER
IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN
CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
```

### Phosphor Icons

<https://github.com/phosphor-icons/core>, MIT license: the "Flask" icon (regular weight, `assets/regular/flask.svg` of
`@phosphor-icons/core` 2.1.1), copied unaltered into `app/src/lib/status.ts` for the experimental state.

```
MIT License

Copyright (c) 2023 Phosphor Icons

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

## Facts only, no code

These projects are under the GPL. PeriHub took only facts from them (commands, byte layouts, USB ids,
interface numbers), described in its own words in `docs/protocols`, and wrote its own code. No code
was copied.

- [OpenRGB](https://gitlab.com/CalcProgrammer1/OpenRGB) (GPL-2.0-or-later): the Apex 8-zone lighting
  protocol, the interface and usage page of each ASUS mouse's configuration collection, and for the
  Glorious mice the usage page of theirs, how their reports are split over collections on Windows, and
  that a reading right after a write is incomplete, and the interface and usage page of the Logitech
  G600's, and the name and USB id of a white Corsair K70 PRO MINI WIRELESS (see `docs/protocols/corsair-bragi.md`), and
  the values of the Corsair keyboards' physical layout property and how it reads a Logitech keyboard's layout code (see
  `docs/protocols/keyboard-layouts.md`), and the Logitech G213's USB id, the collection that takes its lighting commands,
  its five lighting areas, the bytes of its lighting commands and that its answer is read after each (see
  `docs/protocols/logitech-g213.md`).
- [g810-led](https://github.com/MatMoul/g810-led) (GPL-3.0), commit efa3c35: the same lighting commands as sent to the
  Logitech G213 (`src/classes/Keyboard.cpp`, `Keyboard.h`): the numbers of the effect groups and of the wave variants,
  that the keyboard needs no "commit", and the byte of an effect that stores it (see `docs/protocols/logitech-g213.md`).
- [OpenRGB](https://gitlab.com/CalcProgrammer1/OpenRGB) (GPL-2.0-or-later), its issues 1782 and 2305: the USB captures
  of Cooler Master's app that users attached there, read as data, and which product ids form the Cooler Master mouse
  family (see `docs/protocols/coolermaster-mm.md`).
- [OpenRGB](https://gitlab.com/CalcProgrammer1/OpenRGB) (GPL-2.0-or-later), its EVGA drivers
  (`Controllers/EVGAUSBController`): the EVGA X12 and X20 USB ids, interfaces and lighting reports (see
  `docs/protocols/evga-mice.md`).
- [OpenRGB](https://gitlab.com/CalcProgrammer1/OpenRGB) (GPL-2.0-or-later), its SteelSeries Apex controller
  (`Controllers/SteelSeriesController`, commit 6b681e3c): the USB ids and interface of the Apex Pro, Apex 7, Apex 7 TKL and
  Apex 5, the report sizes, the firmware, brightness, colour and "back to the keyboard's own lighting" commands, their
  answers, and the keys of the colour frame (see `docs/protocols/steelseries-apex.md`). No code was copied.
- [apex-tux](https://github.com/not-jan/apex-tux) (Unlicense), commit d0911fe: the picture the display of
  these keyboards takes (128 x 40 pixels, one bit each) and the feature report it is sent in (see
  `docs/protocols/steelseries-apex.md`). Public domain, credited although nothing requires it. No code was copied.
- [OpenRGB](https://gitlab.com/CalcProgrammer1/OpenRGB) (GPL-2.0-or-later), its EVision keyboard controllers
  (`Controllers/EVisionKeyboardController`, commit 91714d1): the EVision keyboards' USB ids, interface and usage page,
  report format and checksum, the configuration read and write commands, the lighting header and its modes, and the
  layout of the newer controller's profiles (see `docs/protocols/evision.md`).
- [OpenEVision](https://github.com/iefedev/OpenEVision) (GPL-2.0), commit 1cca304 (`docs/PROTOCOL.md`): what it checked on
  a `320f:505b`: the reads and writes of the configuration memory, the layout of the lighting header and of the key table,
  the regions it never writes, and that every program with the collection open receives every answer (see
  `docs/protocols/evision.md`). No code was copied.
- The [Linux kernel](https://github.com/torvalds/linux) (GPL-2.0): that the Roccat Kone mice are configured
  through their boot mouse interface (`drivers/hid/hid-roccat-konepure.c`, `hid-roccat-koneplus.c`), and the
  product ids of the Logitech receivers (`drivers/hid/hid-logitech-dj.c`), and how a HID++ 2.0 request and an error answer are laid
  out (`drivers/hid/hid-logitech-hidpp.c`, see `docs/protocols/logitech-g213.md`).
- [Solaar](https://github.com/pwr-Solaar/Solaar) (GPL-2.0): the Logitech receiver ids, and how it reads and writes
  onboard profile lists and button entries, compared with libratbag's (see
  `docs/protocols/logitech-onboard-profiles.md`); that a Logitech device on Bluetooth Low Energy is asked on device index
  0xFF and may answer on 0x00 (`base.py`, same page); that the Logitech G9, G9x, G500, G500s, G700 and G700s speak
  HID++ 1.0 on USB interface 1, and how it codes the DPI of the G9 and G9x (see `docs/protocols/logitech-hidpp10.md`);
  the functions and replies of the HID++ 2.0 lighting features 0x8040, 0x8071, 0x8081 and 0x4540, the names of
  the 0x8071 effects, and which of their calls only read; how a program takes a keyboard's lights over and gives
  them back, the key ids of 0x8081, the pauses after a busy reply, and the G HUB captures and hardware tests in
  its pull request 3149 (see `docs/protocols/logitech-keyboard-lighting.md`);
  how it reads a Logitech keyboard's layout code, HID++ `0x4540` (see `docs/protocols/keyboard-layouts.md`); what the
  HID++ 2.0 root feature answers (see `docs/protocols/logitech-g213.md`).
- [logi-ghub-linux](https://github.com/theo-antonescu/logi-ghub-linux) (GPL-2.0): its notes of a USB capture of
  Logitech G HUB on a G915 X and of its tests on a G915 X LS TKL: host mode through 0x8101, the 0x8071 claim with
  both activity notices, and the light timers G HUB writes (see `docs/protocols/logitech-keyboard-lighting.md`).
- Logitech's HID++ 2.0 feature specifications "x8071 – RGB Effects", "x8081 – Per-Key Lighting" and "x8040 –
  Brightness Control", from Logitech's public HID++ folder, read from the copy in
  [logirgb](https://github.com/mevdschee/logirgb) (MIT): the byte layouts of the effects, power modes, activity
  notices and per-key colours (see `docs/protocols/logitech-keyboard-lighting.md`).
- The `lsusb -v` dumps of [linuxhw/LsUSB](https://github.com/linuxhw/LsUSB): which interfaces the Logitech G600
  and the Roccat Kone mice have.
- [OpenRazer](https://github.com/openrazer/openrazer) (GPL-2.0): the Razer report layout and commands, and per mouse
  its USB interface, transaction ids, maximum DPI, lights with a brightness and custom frame width, read as text
  by `tools/openrazer_facts.py` (see `docs/protocols/razer-mice.md`); per keyboard its USB interface, waits
  and transaction ids, and the battery, charging, brightness and game mode reads, read as text by the same tool (see
  `docs/protocols/razer-keyboards.md`); the keyboard layout read and its region codes (see
  `docs/protocols/keyboard-layouts.md`).
- [OpenMouse mouse-protocol](https://github.com/OpenMouse-Project/mouse-protocol) (AGPL-3.0), its K-Snake driver
  (commit 5a4f643): the battery request of the YJX mice and where their settings hold the lift-off distance (see
  `docs/protocols/yjx-mice.md`).
- [ckb-next](https://github.com/ckb-next/ckb-next) (GPL-2.0-only) and
  [OpenLinkHub](https://github.com/jurkovic-nikola/OpenLinkHub) (GPL-3.0): Corsair's protocol; from ckb-next also the
  keyboards' physical layout property (see `docs/protocols/keyboard-layouts.md`).
- [QMK](https://github.com/qmk/qmk_firmware) (GPL-2.0-or-later), [vial-qmk](https://github.com/vial-kb/vial-qmk)
  (GPL-2.0) and [vial-gui](https://github.com/vial-kb/vial-gui) (GPL-2.0): the VIA and Vial commands, the
  lighting values and VialRGB's effect ids, the raw HID collection and interface of QMK keyboards, and the
  serial numbers Vial uses to find its keyboards (see `docs/protocols/qmk-via-vial.md`).

## Downloaded at run time, not shipped

Not part of PeriHub and not shipped with it: downloaded from the project's own release only when you
use the feature that needs it, checked against a pinned SHA-256, and removed afterwards.

- [USBPcap](https://github.com/desowin/usbpcap) (GPL-2.0 driver, BSD-2-Clause USBPcapCMD), version 1.5.4.0,
  by Tomasz Moń: on Windows, **Send a report → Record the official software** downloads
  `USBPcapSetup-1.5.4.0.exe` from the project's GitHub release, installs it for the recording only and
  removes it completely after it (see `docs/captures.md`). PeriHub runs USBPcapCMD and reads what it
  captures; none of USBPcap's code is copied into PeriHub.
