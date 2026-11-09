//! Other programs that talk to the same devices. Logitech G HUB, for example, puts the mouse
//! under software control when it starts and after every wake-up, so the mouse stops using its
//! onboard profiles. PeriHub does not fight such a program: it tells the user instead.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Conflict {
    LogitechGHub,
    LogitechGamingSoftware,
}

impl Conflict {
    pub fn name(self) -> &'static str {
        match self {
            Self::LogitechGHub => "Logitech G HUB",
            Self::LogitechGamingSoftware => "Logitech Gaming Software",
        }
    }

    /// Executable names, lowercase. Only names seen on a real PC: lghub_agent and
    /// lghub_system_tray in the Windows test of 0.3.0, lghub is G HUB's window. Logitech Gaming
    /// Software runs as LCore.exe ("Logitech Gaming Framework" in Task Manager, issue #21); its
    /// LogiRegistryService.exe is a sound service that keeps running after it is quit, so it is
    /// not listed.
    fn processes(self) -> &'static [&'static str] {
        match self {
            Self::LogitechGHub => &["lghub.exe", "lghub_agent.exe", "lghub_system_tray.exe"],
            Self::LogitechGamingSoftware => &["lcore.exe"],
        }
    }
}

const ALL: &[Conflict] = &[Conflict::LogitechGHub, Conflict::LogitechGamingSoftware];

/// Programs that drive Corsair mice, by executable name (lowercase). Two programs driving the same
/// Corsair mouse can leave it unable to click (OpenRGB issue 4843), so PeriHub's experimental
/// lighting stays off while one of them runs. Names from their build files and packages:
/// openrgb (Arch package), ckb-next and ckb-next-daemon (CMake targets), OpenLinkHub (its systemd
/// unit). iCUE's is the name of its main executable, not yet seen on a real PC.
const CORSAIR_PROGRAMS: &[(&str, &str)] = &[
    ("openrgb", "OpenRGB"),
    ("openrgb.exe", "OpenRGB"),
    ("ckb-next", "ckb-next"),
    ("ckb-next-daemon", "ckb-next"),
    ("openlinkhub", "OpenLinkHub"),
    ("icue.exe", "Corsair iCUE"),
];

/// Programs that drive Corsair mice and are running now, by name, without repeats.
pub fn corsair_programs_running() -> Vec<&'static str> {
    corsair_programs_in(&process_names())
}

fn corsair_programs_in(names: &[String]) -> Vec<&'static str> {
    let mut found: Vec<&'static str> =
        CORSAIR_PROGRAMS.iter().filter(|(exe, _)| names.iter().any(|n| n == exe)).map(|&(_, name)| name).collect();
    found.dedup();
    found
}

/// Conflicting programs that are running now. They only exist on Windows.
pub fn running() -> Vec<Conflict> {
    running_in(&process_names())
}

fn running_in(names: &[String]) -> Vec<Conflict> {
    ALL.iter().copied().filter(|c| c.processes().iter().any(|p| names.iter().any(|n| n == p))).collect()
}

/// Lowercase executable names of the running processes.
#[cfg(windows)]
fn process_names() -> Vec<String> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
    };

    let mut names = Vec::new();
    // SAFETY: plain Win32 calls; the entry carries its size and the snapshot is closed.
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return names;
        }
        let mut entry = PROCESSENTRY32W { dwSize: size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut ok = Process32FirstW(snapshot, &mut entry);
        while ok != 0 {
            let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
            names.push(String::from_utf16_lossy(&entry.szExeFile[..len]).to_lowercase());
            ok = Process32NextW(snapshot, &mut entry);
        }
        CloseHandle(snapshot);
    }
    names
}

/// Lowercase names of the running processes, from /proc (empty where there is none).
#[cfg(not(windows))]
fn process_names() -> Vec<String> {
    let Ok(dir) = std::fs::read_dir("/proc") else { return Vec::new() };
    dir.flatten()
        .filter(|e| e.file_name().to_string_lossy().bytes().all(|b| b.is_ascii_digit()))
        .filter_map(|e| std::fs::read_to_string(e.path().join("comm")).ok())
        .map(|name| name.trim().to_lowercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn programs_that_drive_corsair_mice_are_found_by_name() {
        let names = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(corsair_programs_in(&names(&["zsh", "perihub", "firefox"])).is_empty());
        assert_eq!(corsair_programs_in(&names(&["ckb-next", "ckb-next-daemon"])), ["ckb-next"]);
        assert_eq!(corsair_programs_in(&names(&["openlinkhub", "icue.exe"])), ["OpenLinkHub", "Corsair iCUE"]);
        // Only whole names: a process that merely contains the name does not count.
        assert!(corsair_programs_in(&names(&["openrgb-helper"])).is_empty());
    }

    #[test]
    fn logitech_programs_are_found_by_their_process() {
        let names = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(running_in(&names(&["explorer.exe", "perihub.exe"])).is_empty());
        assert_eq!(running_in(&names(&["lghub_agent.exe", "lghub.exe"])), [Conflict::LogitechGHub]);
        // The processes of the PC in issue #21, with Logitech Gaming Software open.
        let lgs = names(&["lcore.exe", "logiregistryservice.exe", "logi_lamparray_service.amd64.exe"]);
        assert_eq!(running_in(&lgs), [Conflict::LogitechGamingSoftware]);
        // Its sound service alone, which stays after it is quit.
        assert!(running_in(&names(&["logiregistryservice.exe"])).is_empty());
        assert_eq!(Conflict::LogitechGamingSoftware.name(), "Logitech Gaming Software");
    }

    #[test]
    fn process_names_can_be_read() {
        // This test itself is running.
        if cfg!(target_os = "linux") {
            assert!(!process_names().is_empty());
        }
    }
}
