# Keyboard layouts

How PeriHub tells which layout a keyboard has, so that it draws the right keys (an ISO Enter, the key next to the
left Shift, the JIS keys) with the right legends. Two halves, from two places:

- **The physical layout** (ANSI, ISO, JIS, ABNT2): read from the keyboard, where its protocol has a read for it, with
  a command that is already in that driver's list of reads. PeriHub never sends anything to a keyboard to find out its
  layout. Where there is no read, the shape the system's layout usually goes with is used.
- **The logical layout** (what each key types, so the legends to draw): read from the system, as the user, without
  root and without opening any device.

The user can set either half for a keyboard on its page, when detection is wrong; the choice is kept for that
keyboard (by USB id). Test reports carry what was read and what is shown, by layout name only.

Facts found in GPL projects are written here in PeriHub's own words; no code was copied.

## Reads from the keyboard

### Corsair (Bragi)

Property `0x41`, read with the usual get property command (`0x02`, see `corsair-bragi.md`), only on keyboards:

| Value | Layout | Source |
|---|---|---|
| 1 | ANSI | ckb-next, OpenRGB |
| 2 | ISO | ckb-next, OpenRGB |
| 3 | ABNT | ckb-next, OpenRGB |
| 4 | JIS | OpenRGB |

ckb-next reads it when it opens a Bragi keyboard and warns about any other value. A keyboard that does not answer it
shows its layout as not read; nothing else changes.

### Razer

Class `00`, command `86`, size `02`, no arguments, transaction id `ff` (OpenRazer, `kbd_layout`, which it
calls "the physical layout of the keyboard" and creates for its keyboards). The first byte of the answer is the
region of the keyboard:

| Code | Region | Shape PeriHub takes |
|---|---|---|
| `01` | US | ANSI |
| `02` | Greek | not given: the system's |
| `03` | German | ISO |
| `04` | French | ISO |
| `05` | Russian | not given: the system's |
| `06` | UK | ISO |
| `07` | Nordic | ISO |
| `0a` | Turkish | ISO |
| `0c` | Japanese | JIS |
| `0f` | Swiss | ISO |
| `10` | Spanish | ISO |
| `11` | Italian | ISO |
| `12` | Portuguese | ISO |
| `81` | US (Mac) | ANSI |

The region names are OpenRazer's (its daemon's `layout_ids`); the shapes are our reading of them. A keyboard that
answers status `05` (not supported) leaves the layout to the system.

### Logitech (HID++ 2.0)

Feature `0x4540` (Keyboard International Layouts, in Logitech's public feature list), function 0, no parameters:
the first byte of the answer is a layout code. Solaar and OpenRGB both read it this way. What the codes mean is
**not known to PeriHub**: the two projects disagree (Solaar has 1 ANSI, 2 ISO, 3 German, 4 French; OpenRGB has 1 US,
2 international, 3 UK, 4 German, 5 French) and OpenRGB says its table comes from Logitech's own software, which PeriHub
takes no facts from. So PeriHub reads the code only on a Logitech keyboard paired with a receiver that it describes
for a test report, puts it in the report next to the system's layout, and takes the shape from the system. Test
reports will tell what the codes mean.

### No read

- **SteelSeries Apex 3 TKL** and **Apex 3**: no layout read is documented, and their USB ids do not tell the
  region.
- **Wraith W60**: its model string (`wraith-he.md`) does not include the layout.
- **QMK keyboards**: VIA and Vial do not report the physical layout as a value.

For these the shape comes from the system's layout.

## The system's layout

### Linux

The first of these that answers:

1. Hyprland: `j/devices` on its command socket, the main keyboard's active layout (`layout`, `variant` and
   `active_layout_index`).
2. Sway (`SWAYSOCK` set): `swaymsg -t get_inputs -r`, the first keyboard's `xkb_active_layout_name`. Sway gives
   only the description ("Italian", "English (UK)"), which PeriHub turns into the xkb name for the layouts it draws.
3. GNOME (`XDG_CURRENT_DESKTOP` has GNOME): `gsettings get org.gnome.desktop.input-sources mru-sources` (the
   first is the active one), else `sources`; the first `xkb` source, such as `it` or `us+intl` (layout+variant).
4. KDE Plasma (`XDG_CURRENT_DESKTOP` has KDE): `~/.config/kxkbrc`, section `[Layout]`, the first of
  `LayoutList` and `VariantList` when `Use=true`.
5. `XKB_DEFAULT_LAYOUT` and `XKB_DEFAULT_VARIANT`.
6. On X11 (`XDG_SESSION_TYPE=x11`): `setxkbmap -query`.
7. `localectl status`: `X11 Layout` and `X11 Variant`.

The same layout is used to type the text of software macros (`softmacro/layout.rs`).

### Windows

`GetKeyboardLayout` of the thread of the window in front (PeriHub's own while the user looks at it). The low word
of the handle is the language; the high word is the layout's language id, unless it starts with `F` (a variant
such as US-International) or `E` (an input method), and then the language says it. `GetKeyboardType(0)` returning 7
means a Japanese keyboard, so JIS.

### Legends PeriHub draws

US, UK, Italian, German, French, Spanish, Portuguese, Swedish/Finnish, Norwegian, Danish, Japanese and Brazilian
(xkb `us`, `gb`/`ie`, `it`, `de`/`at`, `fr`, `es`, `pt`, `se`/`fi`, `no`, `dk`, `jp`, `br`). A
variant that moves the letters (Dvorak, Colemak, BÉPO…) and any other layout are drawn with the legends of the system
language, else US, and the page says that PeriHub has no legends for that layout. Swiss, Belgian, Canadian French and
Latin American layouts are not drawn yet.

## Sources

- [ckb-next](https://github.com/ckb-next/ckb-next) (GPL-2.0-only): `BRAGI_HWLAYOUT` (`0x41`) and the layout
  numbers.
- [OpenRGB](https://gitlab.com/CalcProgrammer1/OpenRGB) (GPL-2.0-or-later): the Corsair layout values 1 to 4, and how
  it reads Logitech's `0x4540` (its code table is not used).
- [OpenRazer](https://github.com/openrazer/openrazer) (GPL-2.0): command `00 86` and its region codes.
- [Solaar](https://github.com/pwr-Solaar/Solaar) (GPL-2.0): how it reads `0x4540` (its code table is not used).
- [Logitech cpg-docs](https://github.com/Logitech/cpg-docs): the name of feature `0x4540` in the public feature list.
- Microsoft's documentation of `GetKeyboardLayout` and `GetKeyboardType`; the man pages of `localectl`,
  `setxkbmap`, `gsettings` and `swaymsg`.
