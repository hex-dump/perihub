# Quick actions: tray menu and command line

PeriHub can switch a few things without opening its window. They only switch what is already
saved: nothing here writes to a device's memory, and there is no quick action to save, restore,
reset or remap anything.

## Tray menu

Right-click the tray icon. Each connected device with something to switch has its own submenu:

- Logitech mouse (models PeriHub fully supports): its onboard profiles, and the DPI levels of the
  active profile.
- Keyboard: the lighting presets you saved in PeriHub.

## Command line

`perihub-cli app ...` asks the running PeriHub to do the same, so it works while the app is open
(the other `perihub-cli` commands need it closed). PeriHub can stay in the tray.

```sh
perihub-cli app profile 2        # mouse onboard profile 2 (1 is the first)
perihub-cli app dpi-level 3      # third DPI level of the active profile
perihub-cli app preset Night     # keyboard lighting preset called "Night"
```

Bind them to a keyboard shortcut in your desktop's settings (on Windows, a shortcut to
`perihub-cli.exe app profile 2` with a shortcut key). If PeriHub is not running, or the device is
not connected, the command prints why and exits with an error.

## Read-only mode

Quick actions send a change to a device, so read-only mode stops them: the submenus leave the tray
menu while it is on, and `perihub-cli app ...` gets "Read-only mode is on". Each refusal is in the
list of writes of the session (Settings, and test reports). When a quick action fails, for this
or any other reason, PeriHub also says so in a notification.

## How the command reaches the app

PeriHub listens on 127.0.0.1 only, on a port it picks at every start. It writes the port and a
random token to the file `ipc` in its settings folder, readable only by you (mode 600 on Linux; on
Windows the folder is in your profile). A request without the token is ignored, and only the three
actions above are understood.
