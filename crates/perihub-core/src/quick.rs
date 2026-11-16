//! Quick actions: the few changes that are safe to ask a running PeriHub for, from its tray menu
//! or from `perihub-cli app ...` (so they can be bound to a desktop hotkey).
//!
//! They only switch what is already saved: nothing here writes to a device's memory, and there is
//! no way to ask for a save, a restore, a factory reset or a key remap.
//!
//! The command line reaches the app over a local connection (see [`serve`] and [`send`]). The app
//! listens on 127.0.0.1 only, on a port it picks at start, and every request must carry a random
//! token that the app wrote to a file only the current user can read (`ipc` in the settings folder).
//! A web page that tries the port cannot send a valid request: it does not know the token, and
//! what it sends is not in the format below.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::config::config_dir;
use crate::error::{Error, Result};

/// A change PeriHub does when asked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum QuickAction {
    /// Switches the mouse to one of its onboard profiles (1, 2, 3...). Nothing is saved.
    Profile { number: u8 },
    /// Switches the mouse to a DPI level of its active profile (1 is the first). Nothing is saved.
    DpiLevel { number: u8 },
    /// Shows a saved keyboard lighting preset.
    Preset { name: String },
}

impl QuickAction {
    /// Reads the words after `perihub-cli app`: `profile 2`, `dpi-level 3`, `preset Name`.
    pub fn from_args(args: &[&str]) -> Result<Self> {
        let number = |text: &str, what: &str| {
            text.parse::<u8>().ok().filter(|n| *n >= 1).ok_or_else(|| Error::Invalid(format!("{what} number")))
        };
        match args {
            ["profile", n] => Ok(Self::Profile { number: number(n, "profile")? }),
            ["dpi-level", n] => Ok(Self::DpiLevel { number: number(n, "DPI level")? }),
            ["preset", name @ ..] if !name.is_empty() => Ok(Self::Preset { name: name.join(" ") }),
            _ => Err(Error::Invalid("not a quick action".into())),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct Request {
    token: String,
    action: QuickAction,
}

#[derive(Serialize, Deserialize)]
struct Reply {
    ok: bool,
    #[serde(default)]
    error: Option<String>,
}

/// Where the app says how to reach it: "port token".
fn endpoint_path() -> PathBuf {
    config_dir().join("ipc")
}

/// A request is one short line.
const MAX_REQUEST: u64 = 4096;

fn token() -> Result<String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| Error::Io(std::io::Error::other(e.to_string())))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// Compares without stopping at the first difference.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0, |diff, (x, y)| diff | (x ^ y)) == 0
}

fn write_private(path: &std::path::Path, text: &str) -> Result<()> {
    std::fs::create_dir_all(path.parent().unwrap())?;
    let temp = path.with_extension("tmp");
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    // On Windows the settings folder is in the user's profile, which other users cannot read.
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(&temp)?.write_all(text.as_bytes())?;
    std::fs::rename(temp, path)?;
    Ok(())
}

/// Starts listening for quick actions in a thread of its own. `handler` runs one at a time per
/// connection and answers with an error message to show the user, or nothing.
pub fn serve(handler: impl Fn(QuickAction) -> std::result::Result<(), String> + Send + Sync + 'static) -> Result<()> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))?;
    let port = listener.local_addr()?.port();
    let token = token()?;
    write_private(&endpoint_path(), &format!("{port} {token}"))?;
    let handler = Arc::new(handler);
    std::thread::Builder::new().name("quick-actions".into()).spawn(move || {
        for stream in listener.incoming().flatten() {
            let (handler, token) = (handler.clone(), token.clone());
            std::thread::spawn(move || {
                if let Err(error) = answer(stream, &token, &*handler) {
                    log::warn!("quick action request: {error}");
                }
            });
        }
    })?;
    Ok(())
}

fn answer(
    mut stream: TcpStream,
    token: &str,
    handler: &dyn Fn(QuickAction) -> std::result::Result<(), String>,
) -> Result<()> {
    // Only the same computer: the listener is on the loopback address, this is a second check.
    if !stream.peer_addr()?.ip().is_loopback() {
        return Ok(());
    }
    stream.set_read_timeout(Some(Duration::from_secs(3)))?;
    stream.set_write_timeout(Some(Duration::from_secs(3)))?;
    let mut line = String::new();
    BufReader::new((&stream).take(MAX_REQUEST)).read_line(&mut line)?;
    let reply = match serde_json::from_str::<Request>(&line) {
        Ok(request) if same(&request.token, token) => match handler(request.action) {
            Ok(()) => Reply { ok: true, error: None },
            Err(error) => Reply { ok: false, error: Some(error) },
        },
        // Nothing is said about why: a wrong token and a wrong format look the same.
        _ => return Ok(()),
    };
    writeln!(stream, "{}", serde_json::to_string(&reply)?)?;
    Ok(())
}

/// Removes the file that says how to reach the app, when it quits.
pub fn stop() {
    let _ = std::fs::remove_file(endpoint_path());
}

/// Asks the running PeriHub to do `action` and waits for it.
pub fn send(action: QuickAction) -> Result<()> {
    let not_running = || Error::Invalid("PeriHub is not running: open it first (it can stay in the tray)".into());
    let text = std::fs::read_to_string(endpoint_path()).map_err(|_| not_running())?;
    let (port, token) = text.trim().split_once(' ').ok_or_else(not_running)?;
    let port: u16 = port.parse().map_err(|_| not_running())?;
    let mut stream = TcpStream::connect_timeout(&SocketAddr::from((Ipv4Addr::LOCALHOST, port)), Duration::from_secs(2))
        .map_err(|_| not_running())?;
    stream.set_read_timeout(Some(Duration::from_secs(15)))?;
    stream.set_write_timeout(Some(Duration::from_secs(3)))?;
    writeln!(stream, "{}", serde_json::to_string(&Request { token: token.to_string(), action })?)?;
    let mut line = String::new();
    BufReader::new(&stream).read_line(&mut line)?;
    match serde_json::from_str::<Reply>(&line) {
        Ok(Reply { ok: true, .. }) => Ok(()),
        Ok(Reply { error, .. }) => Err(Error::Invalid(error.unwrap_or_else(|| "PeriHub refused".into()))),
        Err(_) => Err(Error::Invalid("PeriHub did not answer: it may have been closed and opened again".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_command_line_words() {
        assert_eq!(QuickAction::from_args(&["profile", "2"]).unwrap(), QuickAction::Profile { number: 2 });
        assert_eq!(QuickAction::from_args(&["dpi-level", "1"]).unwrap(), QuickAction::DpiLevel { number: 1 });
        assert_eq!(
            QuickAction::from_args(&["preset", "Night", "mode"]).unwrap(),
            QuickAction::Preset { name: "Night mode".into() }
        );
        for bad in
            [&["profile", "0"][..], &["profile", "x"], &["profile"], &["preset"], &["factory", "1"], &["restore", "1"]]
        {
            assert!(QuickAction::from_args(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn only_the_three_safe_actions_exist_on_the_wire() {
        for bad in [r#"{"type":"factory","number":1}"#, r#"{"type":"restore","id":"x"}"#, r#"{"type":"save-profile"}"#]
        {
            assert!(serde_json::from_str::<QuickAction>(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn compares_tokens() {
        assert!(same("abc", "abc"));
        assert!(!same("abc", "abd"));
        assert!(!same("abc", "ab"));
    }

    #[test]
    fn a_request_with_the_token_runs_and_others_do_not() {
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let log = seen.clone();
        serve(move |action| {
            log.lock().unwrap().push(action.clone());
            match action {
                QuickAction::Preset { .. } => Err("no such preset".into()),
                _ => Ok(()),
            }
        })
        .unwrap();
        send(QuickAction::Profile { number: 2 }).unwrap();
        let error = send(QuickAction::Preset { name: "x".into() }).unwrap_err();
        assert!(error.to_string().contains("no such preset"));

        let (port, token) = std::fs::read_to_string(endpoint_path())
            .unwrap()
            .split_once(' ')
            .map(|(p, t)| (p.to_string(), t.to_string()))
            .unwrap();
        // Wrong token, and an HTTP request from a web page: no answer, nothing runs.
        for body in [
            format!("{{\"token\":\"{}\",\"action\":{{\"type\":\"profile\",\"number\":3}}}}\n", "0".repeat(token.len())),
            "POST / HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n".to_string(),
        ] {
            let mut stream = TcpStream::connect(("127.0.0.1", port.parse::<u16>().unwrap())).unwrap();
            stream.write_all(body.as_bytes()).unwrap();
            let mut reply = String::new();
            stream.read_to_string(&mut reply).unwrap();
            assert!(reply.is_empty());
        }
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 2);
        stop();
        assert!(send(QuickAction::Profile { number: 1 }).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn the_endpoint_file_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let path = config_dir().join("ipc-test");
        write_private(&path, "1 x").unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        let _ = std::fs::remove_file(path);
    }
}
