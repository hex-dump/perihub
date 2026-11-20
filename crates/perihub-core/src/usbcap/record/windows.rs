//! Recording on Windows: USBPcap, installed only for the recording by an elevated copy of PeriHub.
//!
//! The app (not elevated) creates two named pipes and starts its own program again with "runas"
//! and [`HELPER_ARG`](super::HELPER_ARG): Windows asks once for administrator rights. The helper
//! connects to the pipes, downloads the USBPcap installer into a folder only administrators can
//! write to, checks its SHA-256, installs it ([`usbpcap`](super::usbpcap)), runs USBPcapCMD on the
//! root hubs that have the device, reading each capture from a pipe of its own and keeping only the
//! device ([`Collector`]), and waits for "stop" or "cancel" from the app. Then it sends the
//! device's frames, and removes USBPcap whatever happened: also on an error, on "cancel", and when
//! the app goes away (its pipe closes). If the helper itself cannot finish (the computer is turned
//! off), a marker in ProgramData says so, and the app offers to remove USBPcap at the next start.
//! A USBPcap that was installed before (by Wireshark, say) is used and left installed.
//!
//! What the helper sends the app, one line each: `S text` (what it does), `R` (recording),
//! `A bus address` (where the device is), `K` (too much data: the end is missing), `D length`
//! then that many bytes (the device's frames, as pcap), `C text` (what the removal left), `E text`
//! (an error).

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::windows::io::{AsRawHandle, FromRawHandle};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_CANCELLED, ERROR_PIPE_CONNECTED, GetLastError, HANDLE, INVALID_HANDLE_VALUE, LocalFree,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows_sys::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
use windows_sys::Win32::Storage::FileSystem::{
    CreateDirectoryW, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAGS_AND_ATTRIBUTES, PIPE_ACCESS_INBOUND,
    PIPE_ACCESS_OUTBOUND,
};
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_WAIT,
};
use windows_sys::Win32::System::Threading::{INFINITE, WaitForSingleObject};
use windows_sys::Win32::UI::Shell::{SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

use super::usbpcap;
use super::{Event, Events, HELPER_ARG, Recorded, Request};
use crate::usbcap::pcap::{self, PcapStream};
use crate::usbcap::{Collector, Frame, Link, Target};

/// No console window for the programs the helper runs.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

const RUNNING: u8 = 0;
const STOP: u8 = 1;
const CANCEL: u8 = 2;

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

fn pipe_name(token: &str, which: &str) -> String {
    format!(r"\\.\pipe\perihub-usb-{token}-{which}")
}

fn system_root() -> PathBuf {
    PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into()))
}

fn program_files() -> PathBuf {
    PathBuf::from(std::env::var_os("ProgramFiles").unwrap_or_else(|| r"C:\Program Files".into()))
}

/// Says that a recording installed USBPcap and has not removed it yet.
fn marker_path() -> PathBuf {
    PathBuf::from(std::env::var_os("ProgramData").unwrap_or_else(|| r"C:\ProgramData".into()))
        .join("PeriHub")
        .join("usb-recording-unfinished")
}

pub fn leftover() -> bool {
    marker_path().exists()
}

/// A named pipe this process serves, one client at a time, local only.
fn create_pipe(name: &str, access: FILE_FLAGS_AND_ATTRIBUTES) -> Result<File, String> {
    let handle = unsafe {
        CreateNamedPipeW(
            wide(name).as_ptr(),
            access | FILE_FLAG_FIRST_PIPE_INSTANCE,
            PIPE_TYPE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            1 << 16,
            1 << 20,
            0,
            std::ptr::null(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(format!("cannot create a pipe (error {})", unsafe { GetLastError() }));
    }
    Ok(unsafe { File::from_raw_handle(handle as _) })
}

/// Waits for the client of a pipe made by [`create_pipe`].
fn connect(pipe: &File) -> Result<(), String> {
    let ok = unsafe { ConnectNamedPipe(pipe.as_raw_handle() as HANDLE, std::ptr::null_mut()) };
    if ok == 0 && unsafe { GetLastError() } != ERROR_PIPE_CONNECTED {
        return Err("the pipe was not connected".into());
    }
    Ok(())
}

/// A process handle, closed when dropped.
struct Process(HANDLE);

// The handle is only waited on and closed.
unsafe impl Send for Process {}

impl Process {
    fn wait(&self) {
        unsafe { WaitForSingleObject(self.0, INFINITE) };
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

/// Starts PeriHub's own program as administrator (Windows asks).
fn run_elevated(params: &str) -> Result<Process, String> {
    let exe = std::env::current_exe().map_err(|e| format!("cannot find PeriHub's program: {e}"))?;
    let (verb, file, params) = (wide("runas"), wide(&exe.to_string_lossy()), wide(params));
    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC;
    info.lpVerb = verb.as_ptr();
    info.lpFile = file.as_ptr();
    info.lpParameters = params.as_ptr();
    info.nShow = SW_HIDE;
    if unsafe { ShellExecuteExW(&mut info) } == 0 {
        let error = unsafe { GetLastError() };
        return Err(if error == ERROR_CANCELLED {
            "administrator rights were not given: nothing was installed or recorded".into()
        } else {
            format!("cannot start the recorder as administrator (error {error})")
        });
    }
    if info.hProcess.is_null() {
        return Err("Windows did not start the recorder".into());
    }
    Ok(Process(info.hProcess))
}

// ---- The app's side ----

#[derive(Default)]
struct Control {
    pipe: Option<File>,
    /// "stop" or "cancel" asked before the helper connected.
    pending: Option<&'static [u8]>,
}

pub struct Handle {
    control: Arc<Mutex<Control>>,
    cancelled: Arc<AtomicBool>,
}

impl Handle {
    pub fn stop(&self, cancel: bool) {
        if cancel {
            self.cancelled.store(true, Ordering::SeqCst);
        }
        let line: &'static [u8] = if cancel { b"cancel\n" } else { b"stop\n" };
        let mut control = self.control.lock().unwrap_or_else(|e| e.into_inner());
        match control.pipe.as_mut() {
            Some(pipe) => {
                let _ = pipe.write_all(line);
            }
            None => control.pending = Some(line),
        }
    }
}

/// What the helper sent.
#[derive(Default)]
struct Outcome {
    pcap: Option<Vec<u8>>,
    addresses: Vec<(u16, u16)>,
    cut: bool,
    error: Option<String>,
}

/// Starts the helper in `mode` and reads what it sends; `finish` gets it once the helper is done.
fn launch(
    mode: &str,
    args: &str,
    events: Events,
    finish: impl FnOnce(Outcome, bool, &Events) + Send + 'static,
) -> Result<Handle, String> {
    let token = format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)
    );
    let (out_name, in_name) = (pipe_name(&token, "out"), pipe_name(&token, "in"));
    let from_helper = create_pipe(&out_name, PIPE_ACCESS_INBOUND)?;
    let to_helper = create_pipe(&in_name, PIPE_ACCESS_OUTBOUND)?;
    let process = run_elevated(&format!("{HELPER_ARG} {mode} {token} {args}"))?;
    // A helper that ends before it connects would leave the connections waiting: connecting to the
    // pipes from here once it has ended lets them go on, to an empty pipe.
    let names = (out_name, in_name);
    std::thread::spawn(move || {
        process.wait();
        let _ = OpenOptions::new().write(true).open(&names.0);
        let _ = OpenOptions::new().read(true).open(&names.1);
    });
    let control = Arc::new(Mutex::new(Control::default()));
    let cancelled = Arc::new(AtomicBool::new(false));
    let (slot, flag) = (control.clone(), cancelled.clone());
    std::thread::spawn(move || {
        let mut outcome = Outcome::default();
        if connect(&from_helper).is_ok() && connect(&to_helper).is_ok() {
            {
                let mut control = slot.lock().unwrap_or_else(|e| e.into_inner());
                let mut pipe = to_helper;
                if let Some(line) = control.pending.take() {
                    let _ = pipe.write_all(line);
                }
                control.pipe = Some(pipe);
            }
            read_helper(BufReader::new(from_helper), &events, &mut outcome);
        } else {
            outcome.error = Some("the recorder ended before it started".into());
        }
        slot.lock().unwrap_or_else(|e| e.into_inner()).pipe = None;
        finish(outcome, flag.load(Ordering::SeqCst), &events);
    });
    Ok(Handle { control, cancelled })
}

fn read_helper(mut reader: BufReader<File>, events: &Events, outcome: &mut Outcome) {
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
        let text = line.trim_end_matches(['\r', '\n']);
        let (tag, rest) = text.split_once(' ').unwrap_or((text, ""));
        match tag {
            "S" => events(Event::Status(rest.to_string())),
            "R" => events(Event::Recording),
            "C" => events(Event::Cleanup(rest.to_string())),
            "E" => outcome.error = Some(rest.to_string()),
            "K" => outcome.cut = true,
            "A" => {
                if let Some((bus, address)) = rest.split_once(' ')
                    && let (Ok(bus), Ok(address)) = (bus.parse(), address.parse())
                {
                    outcome.addresses.push((bus, address));
                }
            }
            "D" => {
                let Ok(length) = rest.parse::<usize>() else { return };
                let mut data = vec![0; length];
                if reader.read_exact(&mut data).is_err() {
                    return;
                }
                outcome.pcap = Some(data);
            }
            _ => {}
        }
    }
}

pub fn start(request: Request, events: Events) -> Result<Handle, String> {
    let args = format!(
        "{:04x} {:04x} {}",
        request.vendor_id,
        request.product_id,
        usbpcap::hex_text(request.product.as_deref().unwrap_or("-"))
    );
    events(Event::Status("Waiting for administrator rights…".into()));
    launch("record", &args, events, move |outcome, cancelled, events| {
        if cancelled {
            return events(Event::Cancelled);
        }
        if let Some(error) = outcome.error {
            return events(Event::Failed(error));
        }
        let Some(bytes) = outcome.pcap else {
            return events(Event::Failed("the recorder ended without a recording".into()));
        };
        match pcap::read(&bytes) {
            Ok((link, frames)) => events(Event::Finished(Box::new(Recorded {
                link,
                frames,
                target: Target {
                    vendor_id: request.vendor_id,
                    product_id: request.product_id,
                    addresses: outcome.addresses,
                    usb_descriptors: None,
                    report_descriptors: request.report_descriptors,
                },
                source: "USBPcap",
                cut: outcome.cut,
            }))),
            Err(error) => events(Event::Failed(format!("the recording could not be read: {error}"))),
        }
    })
}

pub fn clean_leftover(events: Events) -> Result<Handle, String> {
    launch("cleanup", "", events, |outcome, _, events| match outcome.error {
        Some(error) => events(Event::Failed(error)),
        None => events(Event::Ended),
    })
}

// ---- The elevated helper ----

/// The pipe to the app.
struct Tell(Mutex<File>);

impl Tell {
    fn line(&self, tag: &str, text: &str) {
        let text = text.replace(['\r', '\n'], " ");
        let _ = writeln!(self.0.lock().unwrap_or_else(|e| e.into_inner()), "{tag} {text}");
    }

    fn data(&self, bytes: &[u8]) {
        let mut pipe = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let _ = writeln!(pipe, "D {}", bytes.len());
        let _ = pipe.write_all(bytes);
    }
}

pub fn helper_main(args: &[String]) -> i32 {
    let (Some(mode), Some(token)) = (args.first(), args.get(1)) else { return 2 };
    let Ok(out) = OpenOptions::new().write(true).open(pipe_name(token, "out")) else { return 3 };
    let Ok(control) = OpenOptions::new().read(true).open(pipe_name(token, "in")) else { return 3 };
    let tell = Tell(Mutex::new(out));
    let state = Arc::new(AtomicU8::new(RUNNING));
    let asked = state.clone();
    std::thread::spawn(move || {
        for line in BufReader::new(control).lines() {
            match line.as_deref().map(str::trim) {
                Ok("stop") => {
                    let _ = asked.compare_exchange(RUNNING, STOP, Ordering::SeqCst, Ordering::SeqCst);
                }
                Ok("cancel") => asked.store(CANCEL, Ordering::SeqCst),
                Ok(_) => continue,
                Err(_) => break,
            }
        }
        // The app went away before it said stop: nothing is kept.
        let _ = asked.compare_exchange(RUNNING, CANCEL, Ordering::SeqCst, Ordering::SeqCst);
    });
    match mode.as_str() {
        "record" => {
            let ids = (args.get(2), args.get(3), args.get(4));
            let (Some(vendor), Some(product), Some(name)) = ids else { return 2 };
            let (Ok(vendor_id), Ok(product_id)) = (u16::from_str_radix(vendor, 16), u16::from_str_radix(product, 16))
            else {
                return 2;
            };
            let name = usbpcap::text_from_hex(name).filter(|n| n != "-");
            record_and_clean(&tell, &state, vendor_id, product_id, name.as_deref());
        }
        "cleanup" => {
            if marker_path().exists() {
                tell.line("S", "Removing USBPcap (USB devices disconnect for a couple of seconds)…");
                remove(&tell);
            } else {
                tell.line("C", "Nothing to remove.");
            }
        }
        _ => return 2,
    }
    0
}

fn record_and_clean(tell: &Tell, state: &AtomicU8, vendor_id: u16, product_id: u16, name: Option<&str>) {
    let marker = marker_path();
    // A USBPcap left by an unfinished recording is PeriHub's: it is used, then removed.
    let left_before = marker.exists();
    let present = service_installed();
    let ours = left_before || !present;
    let mut installing = false;
    let job = Job { vendor_id, product_id, name, ours, install: !present };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| record(tell, state, &job, &mut installing)));
    if ours {
        if installing || left_before {
            tell.line("S", "Removing USBPcap (USB devices disconnect for a couple of seconds)…");
            remove(tell);
        } else {
            let _ = std::fs::remove_file(&marker);
        }
    }
    match result {
        Ok(Ok(())) => {}
        Ok(Err(error)) => tell.line("E", &error),
        Err(_) => tell.line("E", "the recorder stopped on an internal error"),
    }
}

/// The USBPcap driver is installed (its service exists).
fn service_installed() -> bool {
    Command::new(system_root().join(r"System32\reg.exe"))
        .args(["query", r"HKLM\SYSTEM\CurrentControlSet\Services\USBPcap"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .is_ok_and(|o| o.status.success())
}

fn powershell(script: &str) -> Result<String, String> {
    let output = Command::new(system_root().join(r"System32\WindowsPowerShell\v1.0\powershell.exe"))
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-EncodedCommand"])
        .arg(usbpcap::encoded_command(script))
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("cannot run PowerShell: {e}"))?;
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    if output.status.success() {
        Ok(text)
    } else {
        Err(format!("{}{}", text.trim(), String::from_utf8_lossy(&output.stderr).trim()))
    }
}

/// A folder only administrators and the system can write to, removed when dropped: what runs from
/// it as administrator cannot be swapped by another program (in the user's own temporary folder it
/// could), nor can a DLL be planted next to it.
struct PrivateDir(PathBuf);

impl PrivateDir {
    fn create() -> Result<Self, String> {
        let nanos =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        let path = system_root().join("Temp").join(format!("perihub-usbpcap-{}-{nanos}", std::process::id()));
        let mut descriptor: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        // Protected: nothing inherited from Temp, whose users may create files.
        let sddl = wide("D:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)");
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                std::ptr::null_mut(),
            )
        } == 0
        {
            return Err("cannot make a private folder for the download".into());
        }
        let attributes = SECURITY_ATTRIBUTES {
            nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: descriptor,
            bInheritHandle: 0,
        };
        let created = unsafe { CreateDirectoryW(wide(&path.to_string_lossy()).as_ptr(), &attributes) };
        unsafe { LocalFree(descriptor as _) };
        if created == 0 {
            return Err(format!("cannot make a private folder for the download (error {})", unsafe { GetLastError() }));
        }
        Ok(Self(path))
    }
}

impl Drop for PrivateDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Downloads the installer with Windows's own curl, and refuses it unless its SHA-256 is the
/// pinned one.
fn download(to: &Path) -> Result<(), String> {
    let output = Command::new(system_root().join(r"System32\curl.exe"))
        .args(["--fail", "--location", "--silent", "--show-error", "--proto", "=https", "--proto-redir", "=https"])
        .args(["--max-time", "300", "--output"])
        .arg(to)
        .arg(usbpcap::SETUP_URL)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("cannot run curl to download USBPcap: {e}"))?;
    if !output.status.success() {
        return Err(format!("the download of USBPcap failed: {}", String::from_utf8_lossy(&output.stderr).trim()));
    }
    use sha2::Digest;
    let bytes = std::fs::read(to).map_err(|e| format!("cannot read the download: {e}"))?;
    let hash: String = sha2::Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect();
    if hash != usbpcap::SETUP_SHA256 {
        return Err(format!("the USBPcap installer is not the expected file (SHA-256 {hash}): it was not run"));
    }
    Ok(())
}

/// One root hub's capture: USBPcapCMD writing into a pipe that a thread reads.
struct Capture {
    child: Child,
    pipe: String,
    reader: JoinHandle<Collector>,
}

impl Capture {
    fn start(command: &Path, hub: &str, n: usize, inject: bool, collector: Collector) -> Result<Self, String> {
        let pipe = format!(r"\\.\pipe\perihub-usbpcap-{}-{n}", std::process::id());
        let server = create_pipe(&pipe, PIPE_ACCESS_INBOUND)?;
        let mut cmd = Command::new(command);
        cmd.args([
            "--extcap-interface",
            hub,
            "--capture",
            "--fifo",
            &pipe,
            "--capture-from-all-devices",
            "--capture-from-new-devices",
        ]);
        if inject {
            cmd.arg("--inject-descriptors");
        }
        let child =
            cmd.creation_flags(CREATE_NO_WINDOW).spawn().map_err(|e| format!("cannot start USBPcapCMD: {e}"))?;
        let reader = std::thread::spawn(move || {
            let mut collector = collector;
            if connect(&server).is_err() {
                return collector;
            }
            let (mut server, mut stream, mut buf) = (server, PcapStream::default(), vec![0u8; 1 << 16]);
            while let Ok(n) = server.read(&mut buf) {
                if n == 0 {
                    break;
                }
                for frame in stream.push(&buf[..n]) {
                    collector.push(frame);
                }
            }
            collector
        });
        Ok(Self { child, pipe, reader })
    }

    fn running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    fn finish(mut self) -> Option<Collector> {
        let _ = self.child.kill();
        let _ = self.child.wait();
        // A reader still waiting for USBPcapCMD to connect goes on.
        let _ = OpenOptions::new().write(true).open(&self.pipe);
        self.reader.join().ok()
    }
}

fn usbpcapcmd(args: &[&str]) -> Result<String, String> {
    let output = Command::new(program_files().join(r"USBPcap\USBPcapCMD.exe"))
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("cannot run USBPcapCMD: {e}"))?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// What a recording is for, and what it has to do first.
struct Job<'a> {
    vendor_id: u16,
    product_id: u16,
    name: Option<&'a str>,
    /// USBPcap is PeriHub's to remove at the end.
    ours: bool,
    /// USBPcap has to be installed first.
    install: bool,
}

/// Installs USBPcap if needed, records until the app says stop or cancel, and sends the device's
/// frames. `installing` is set once the install has started: from then on it has to be removed.
fn record(tell: &Tell, state: &AtomicU8, job: &Job, installing: &mut bool) -> Result<(), String> {
    let Job { vendor_id, product_id, name, ours, install } = *job;
    let asked = || state.load(Ordering::SeqCst);
    if ours {
        let marker = marker_path();
        if let Some(dir) = marker.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(&marker, b"A USB recording installed USBPcap; PeriHub removes it.\r\n");
    }
    if install {
        tell.line("S", &format!("Downloading USBPcap {} from its GitHub release…", usbpcap::VERSION));
        let dir = PrivateDir::create()?;
        let setup = dir.0.join(usbpcap::SETUP_FILE);
        download(&setup)?;
        if asked() == CANCEL {
            return Ok(());
        }
        tell.line("S", "Installing USBPcap: USB devices disconnect for a couple of seconds…");
        *installing = true;
        usbpcap::installed(&powershell(&usbpcap::install_script(&setup.to_string_lossy()))?)?;
    } else if !ours {
        tell.line("S", "USBPcap is already installed on this computer: PeriHub uses it and leaves it installed.");
    }
    if asked() == CANCEL {
        return Ok(());
    }
    tell.line("S", "Looking for the device…");
    let hubs = usbpcap::hubs(&usbpcapcmd(&["--extcap-interfaces"])?);
    if hubs.is_empty() {
        return Err("USBPcap shows no USB root hub: restarting Windows may be needed".into());
    }
    let mut chosen = Vec::new();
    for (hub, bus) in &hubs {
        let config = usbpcapcmd(&["--extcap-interface", hub, "--extcap-config"]).unwrap_or_default();
        let (named, addresses) = usbpcap::device_on_hub(&config, name, vendor_id, product_id);
        chosen.push((hub.clone(), *bus, named, addresses, usbpcap::injects_descriptors(&config)));
    }
    // Only the hubs whose device list names the device; all of them when none does.
    if chosen.iter().any(|c| c.2) {
        chosen.retain(|c| c.2);
    }
    let command = program_files().join(r"USBPcap\USBPcapCMD.exe");
    let mut captures = Vec::new();
    for (n, (hub, bus, _, addresses, inject)) in chosen.iter().enumerate() {
        let collector = Collector::new(Link::UsbPcap, vendor_id, product_id, addresses.iter().map(|a| (*bus, *a)));
        captures.push(Capture::start(&command, hub, n, *inject, collector)?);
    }
    tell.line("R", "");
    let mut failed = None;
    while asked() == RUNNING {
        std::thread::sleep(Duration::from_millis(200));
        if captures.iter_mut().any(|c| !c.running()) {
            failed = Some("USBPcapCMD stopped during the recording".to_string());
            break;
        }
    }
    // What the last change sent is still on its way through the pipes.
    std::thread::sleep(Duration::from_millis(500));
    let (mut frames, mut addresses, mut cut) = (Vec::<Frame>::new(), Vec::new(), false);
    for capture in captures {
        if let Some(collector) = capture.finish() {
            let (f, a, k) = collector.finish();
            frames.extend(f);
            addresses.extend(a);
            cut |= k;
        }
    }
    if let Some(error) = failed {
        return Err(error);
    }
    if asked() != STOP {
        return Ok(());
    }
    frames.sort_by_key(|f| f.time_us);
    for (bus, address) in addresses {
        tell.line("A", &format!("{bus} {address}"));
    }
    if cut {
        tell.line("K", "");
    }
    tell.data(&pcap::write(Link::UsbPcap, &frames));
    Ok(())
}

/// Removes USBPcap and says what is left; the marker goes once nothing is.
fn remove(tell: &Tell) {
    let (clean, text) = match powershell(&usbpcap::remove_script()) {
        Ok(output) => usbpcap::removal_report(&output),
        Err(error) => (false, format!("USBPcap could not be removed: {error}")),
    };
    if clean {
        let _ = std::fs::remove_file(marker_path());
    }
    tell.line("C", &text);
}
