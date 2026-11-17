// SPDX-License-Identifier: MIT
// Copyright (c) hex-dump and PeriHub contributors
//
// PeriHub focus: GNOME Shell does not tell other apps which window is focused, so this extension
// shares the focused window's app id, window class and process id on the session bus, for
// PeriHub's app profiles. Never window titles or contents. It only reads: it changes nothing.
// Bundled in PeriHub and installed by its App profiles page (crates/perihub-core/src/focus/gnome.rs).

import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Shell from 'gi://Shell';

import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';

const BUS_NAME = 'io.github.hex_dump.perihub.GnomeFocus';
const OBJECT_PATH = '/io/github/hex_dump/perihub/GnomeFocus';
const INTERFACE = `<node>
  <interface name="io.github.hex_dump.perihub.GnomeFocus">
    <method name="GetFocus">
      <arg type="s" direction="out" name="app_id"/>
      <arg type="s" direction="out" name="wm_class"/>
      <arg type="u" direction="out" name="pid"/>
    </method>
    <signal name="FocusChanged">
      <arg type="s" name="app_id"/>
      <arg type="s" name="wm_class"/>
      <arg type="u" name="pid"/>
    </signal>
  </interface>
</node>`;

const NONE = ['', '', 0];

/** [app id, window class, pid] of a window; empty values when unknown. */
function describe(window) {
    if (!window)
        return NONE;
    const app = Shell.WindowTracker.get_default().get_window_app(window);
    let appId = app?.get_id() ?? '';
    // Windows without a .desktop file get a made-up "window:N" id.
    appId = appId.startsWith('window:') ? '' : appId.replace(/\.desktop$/, '');
    const pid = window.get_pid();
    return [appId, window.get_wm_class() ?? '', pid > 0 ? pid : 0];
}

export default class PeriHubFocusExtension extends Extension {
    enable() {
        this._focus = NONE;
        this._dbus = Gio.DBusExportedObject.wrapJSObject(INTERFACE, {
            GetFocus: () => this._focus,
        });
        this._dbus.export(Gio.DBus.session, OBJECT_PATH);
        this._focusId = global.display.connect('notify::focus-window', () => this._update());
        // The first signal goes out once the name is ours: a Flatpak only receives signals from
        // names it may talk to.
        this._nameId = Gio.bus_own_name_on_connection(Gio.DBus.session, BUS_NAME,
            Gio.BusNameOwnerFlags.NONE, () => this._update(true), null);
    }

    disable() {
        global.display.disconnect(this._focusId);
        // Also when the screen locks: PeriHub then puts the lighting and the mouse profile back.
        this._focus = NONE;
        this._emit();
        Gio.bus_unown_name(this._nameId);
        this._dbus.unexport();
        this._dbus = null;
        this._focusId = 0;
        this._nameId = 0;
    }

    _update(always = false) {
        const focus = describe(global.display.focus_window);
        if (!always && focus.every((value, i) => value === this._focus[i]))
            return;
        this._focus = focus;
        this._emit();
    }

    _emit() {
        this._dbus.emit_signal('FocusChanged', new GLib.Variant('(ssu)', this._focus));
    }
}
