//! Which app is in focus, for app profiles. Only the app's identifier is read, never window titles.
//!
//! Linux: Hyprland's event socket; on GNOME, PeriHub's own Shell extension ([`gnome`]). Other
//! desktops do not tell apps which window is focused.
//! Windows: the executable of the foreground window, checked twice a second.

use std::thread;

use serde::Serialize;

#[cfg(target_os = "linux")]
mod gnome;

/// Whether the focused app can be detected here.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FocusSupport {
    Supported {
        desktop: &'static str,
    },
    Unsupported {
        reason: &'static str,
    },
    /// Possible after a one-time step the user starts with a button labelled `action`, see [`setup`].
    Setup {
        desktop: &'static str,
        reason: &'static str,
        action: &'static str,
    },
}

pub fn support() -> FocusSupport {
    #[cfg(target_os = "linux")]
    if hyprland::socket_dir().is_none() {
        if gnome::detected() {
            return gnome::support();
        }
        return FocusSupport::Unsupported {
            reason: "On Linux, detecting the app in focus works on Hyprland and GNOME only for now.",
        };
    }
    FocusSupport::Supported { desktop: if cfg!(windows) { "Windows" } else { "Hyprland" } }
}

/// Starts watching in the background. `on_change` gets the focused app's identifier, in lowercase:
/// the window class on Linux, the executable name on Windows. Returns `false` if not supported.
pub fn watch(on_change: impl FnMut(Option<String>) + Send + 'static) -> bool {
    #[cfg(target_os = "linux")]
    let run: Box<dyn FnOnce() + Send> = if let Some(dir) = hyprland::socket_dir() {
        Box::new(move || hyprland::run(dir, on_change))
    } else if gnome::detected() {
        Box::new(move || gnome::run(on_change))
    } else {
        return false;
    };
    #[cfg(windows)]
    let run = move || windows::run(on_change);
    thread::Builder::new().name("perihub-focus".into()).spawn(run).is_ok()
}

/// Runs the one-time step a [`FocusSupport::Setup`] offers. Returns what to tell the user.
pub fn setup() -> Result<String, String> {
    #[cfg(target_os = "linux")]
    if hyprland::socket_dir().is_none() && gnome::detected() {
        return gnome::setup();
    }
    Err("Nothing needs to be set up here.".into())
}

#[cfg(target_os = "linux")]
pub(crate) mod hyprland {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::os::unix::net::UnixStream;
    use std::path::{Path, PathBuf};
    use std::thread;
    use std::time::Duration;

    const RETRY: Duration = Duration::from_secs(3);

    pub fn socket_dir() -> Option<PathBuf> {
        let signature = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE")?;
        let runtime = std::env::var_os("XDG_RUNTIME_DIR")?;
        Some(PathBuf::from(runtime).join("hypr").join(signature))
    }

    fn app_id(class: &str) -> Option<String> {
        let class = class.trim();
        (!class.is_empty()).then(|| class.to_lowercase())
    }

    /// Asks Hyprland's command socket, e.g. `j/devices`, and returns its JSON reply.
    pub(crate) fn request(dir: &Path, command: &str) -> Option<serde_json::Value> {
        let mut stream = UnixStream::connect(dir.join(".socket.sock")).ok()?;
        stream.write_all(command.as_bytes()).ok()?;
        let mut reply = String::new();
        stream.read_to_string(&mut reply).ok()?;
        serde_json::from_str(&reply).ok()
    }

    /// Class of the window focused right now, asked on the command socket.
    fn current(dir: &Path) -> Option<String> {
        let window = request(dir, "j/activewindow")?;
        app_id(window.get("class")?.as_str()?)
    }

    /// Parses an `activewindow>>CLASS,TITLE` event (the class has no commas, the title may).
    pub fn parse_event(line: &str) -> Option<Option<String>> {
        let rest = line.strip_prefix("activewindow>>")?;
        Some(app_id(rest.split(',').next().unwrap_or("")))
    }

    pub fn run(dir: PathBuf, mut on_change: impl FnMut(Option<String>)) {
        loop {
            if let Ok(events) = UnixStream::connect(dir.join(".socket2.sock")) {
                on_change(current(&dir));
                for line in BufReader::new(events).lines() {
                    let Ok(line) = line else { break };
                    if let Some(app) = parse_event(&line) {
                        on_change(app);
                    }
                }
            }
            // Hyprland is restarting, or the socket went away: try again later.
            thread::sleep(RETRY);
        }
    }

    #[cfg(test)]
    mod tests {
        use super::parse_event;

        #[test]
        fn parses_active_window_events() {
            assert_eq!(
                parse_event("activewindow>>Steam_App_730,Counter-Strike 2, live"),
                Some(Some("steam_app_730".into()))
            );
            assert_eq!(parse_event("activewindow>>,"), Some(None));
            assert_eq!(parse_event("activewindowv2>>5589200da7c0"), None);
            assert_eq!(parse_event("windowtitle>>5589200da7c0"), None);
        }
    }
}

#[cfg(windows)]
mod windows {
    use std::thread;
    use std::time::Duration;

    use windows_sys::Win32::Foundation::{CloseHandle, MAX_PATH};
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

    const POLL: Duration = Duration::from_millis(500);

    /// File name of the foreground window's executable, e.g. `cs2.exe`.
    fn foreground_exe() -> Option<String> {
        // SAFETY: plain Win32 calls; the buffer and its length are passed together and the
        // process handle is closed before returning.
        unsafe {
            let window = GetForegroundWindow();
            if window.is_null() {
                return None;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(window, &mut pid);
            if pid == 0 {
                return None;
            }
            let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if process.is_null() {
                return None;
            }
            let mut path = [0u16; MAX_PATH as usize];
            let mut len = path.len() as u32;
            let ok = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, path.as_mut_ptr(), &mut len);
            CloseHandle(process);
            if ok == 0 {
                return None;
            }
            let path = String::from_utf16_lossy(&path[..len as usize]);
            path.rsplit('\\').next().map(str::to_lowercase)
        }
    }

    pub fn run(mut on_change: impl FnMut(Option<String>)) {
        let mut last = None;
        loop {
            let app = foreground_exe();
            if app != last {
                on_change(app.clone());
                last = app;
            }
            thread::sleep(POLL);
        }
    }
}
