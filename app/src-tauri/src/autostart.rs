//! Starting with Windows, which PeriHub does in the tray (`--hidden`).
//!
//! On Windows PeriHub keeps the autostart entry in `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`
//! and manages it itself. auto-launch 0.6 (through tauri-plugin-autostart) prefers `HKLM` in its
//! `Dynamic` mode and succeeds there whenever it can write, so 0.4.2 left machine-wide entries:
//! they would also start PeriHub for other accounts, pointing at this user's folder, which does
//! not exist for them. Entries that those versions left in `HKLM` are moved to `HKCU` at startup,
//! or deleted when Task Manager had turned them off; one that Windows does not let us delete still
//! starts PeriHub, so it counts as enabled and turning autostart off says how to remove it.
//!
//! The Run value is written with quotes around the exe path: without them, with a space in the
//! path like `C:\Users\Mario Rossi\…`, Windows may split the command at the first space. The
//! value in `Explorer\StartupApproved\Run` that Task Manager needs to show the entry as enabled
//! is written too, like auto-launch does. Linux keeps tauri-plugin-autostart.

use tauri::AppHandle;

#[cfg(windows)]
const E_ACCESSDENIED: windows_result::HRESULT = windows_result::HRESULT::from_win32(0x80070005_u32);
#[cfg(windows)]
const E_FILENOTFOUND: windows_result::HRESULT = windows_result::HRESULT::from_win32(0x80070002_u32);

#[cfg(windows)]
const RUN_PATH: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run";

#[cfg(windows)]
const TASK_MANAGER_PATH: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";

/// The command line the entry runs, for example `"C:\PeriHub\perihub.exe" --hidden`.
#[cfg(windows)]
fn command_line() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    Some(format!("\"{}\" {}", exe.display(), crate::HIDDEN_ARG))
}

/// Task Manager can suppress a Run entry. Without a value there, Task Manager shows the entry
/// as enabled, and this value does the same: the flag 2, then 8 zero bytes.
#[cfg(windows)]
const TASK_MANAGER_ENABLED: [u8; 12] = [0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];

/// What Task Manager's value for a Run entry says about it: the first byte is the state, 2 or 6
/// while the entry starts and 3 or 7 once Task Manager turned it off. The bytes after it hold a
/// time, which says nothing about the state, so read the first byte and not the tail.
#[cfg(windows)]
fn task_manager_state(value: &[u8]) -> Option<bool> {
    match value.first()? {
        0x02 | 0x06 => Some(true),
        0x03 | 0x07 => Some(false),
        _ => None,
    }
}

/// Whether Task Manager had turned the entry `name` under `key` off. A value that is missing, or
/// one whose state this does not know, counts as enabled, which is how Task Manager shows it.
#[cfg(windows)]
fn task_manager_disabled(key: &windows_registry::Key, name: &str) -> bool {
    key.get_value(name).ok().and_then(|value| task_manager_state(&value)) == Some(false)
}

/// Windows reopens the apps that were open at sign-out ("Restart apps" in the sign-in options).
/// On Windows 11 with 0.4.1 PeriHub came back without arguments, so with its window, before
/// autostart started it hidden. Registering the restart with `--hidden` makes it come back in the
/// tray. It runs again when the window is created, in case the web view registers its own restart,
/// and logs a registration it replaces, to find out who made it.
#[cfg(windows)]
pub fn restart_hidden() {
    use windows_sys::Win32::System::Recovery::{
        GetApplicationRestartSettings, RESTART_NO_CRASH, RESTART_NO_HANG, RegisterApplicationRestart,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    // Only restarts after a sign-out, a reboot or an update: not after a crash or a hang.
    const FLAGS: u32 = RESTART_NO_CRASH | RESTART_NO_HANG;
    // RESTART_MAX_CMD_LINE.
    let mut current = [0u16; 1024];
    let (mut len, mut flags) = (current.len() as u32, 0);
    // SAFETY: the buffer holds `len` characters; the process handle is the current process.
    let registered =
        unsafe { GetApplicationRestartSettings(GetCurrentProcess(), current.as_mut_ptr(), &mut len, &mut flags) } == 0;
    if registered {
        let used = &current[..(len as usize).min(current.len())];
        let command = String::from_utf16_lossy(&used[..used.iter().position(|&c| c == 0).unwrap_or(used.len())]);
        if command == crate::HIDDEN_ARG && flags == FLAGS {
            return;
        }
        log::info!("restart after sign-in was registered as {command:?} (flags {flags}): registering it hidden");
    }
    let command: Vec<u16> = crate::HIDDEN_ARG.encode_utf16().chain([0]).collect();
    // SAFETY: `command` is a NUL-terminated UTF-16 string that outlives the call.
    let result = unsafe { RegisterApplicationRestart(command.as_ptr(), FLAGS) };
    if result != 0 {
        log::warn!("restart after sign-in not registered: {result:#x}");
    }
}

#[cfg(not(windows))]
pub fn restart_hidden() {}

/// One autostart entry: its name and the registry keys that hold it, under `HKCU` and `HKLM`.
/// The tests use keys of their own, which leaves the real entry alone.
#[cfg(windows)]
struct Entry<'a> {
    name: &'a str,
    run: &'a str,
    task_manager: &'a str,
}

/// Whether PeriHub starts on its own at sign-in.
#[cfg(windows)]
pub fn is_enabled(app: &AppHandle) -> bool {
    Entry::of(app).is_enabled()
}

#[cfg(not(windows))]
pub fn is_enabled(app: &AppHandle) -> bool {
    use tauri_plugin_autostart::ManagerExt;

    app.autolaunch().is_enabled().unwrap_or(false)
}

/// Starts or stops PeriHub starting on its own at sign-in. When `enabled` already holds, both
/// sides still write the entries again with the current exe path and arguments.
#[cfg(windows)]
pub fn set_enabled(app: &AppHandle, enabled: bool) -> Result<(), String> {
    let entry = Entry::of(app);
    if enabled { entry.enable() } else { entry.disable() }
}

#[cfg(not(windows))]
pub fn set_enabled(app: &AppHandle, enabled: bool) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;

    let launcher = app.autolaunch();
    let result = if enabled { launcher.enable() } else { launcher.disable() };
    result.map_err(|e| e.to_string())
}

#[cfg(windows)]
pub fn startup(app: &AppHandle) {
    let entry = Entry::of(app);
    entry.move_from_local_machine();
    // Entries written by 0.3.0 and earlier are not quoted.
    if entry.is_enabled() {
        entry.quote_entry();
    }
}

#[cfg(not(windows))]
pub fn startup(_app: &AppHandle) {}

/// Whether a Run value starts with this copy of PeriHub: the exe path, then nothing or a space
/// with the arguments. One that starts the same way, like `perihub.exe.old`, is another program.
#[cfg(windows)]
fn is_our_entry(entry: &str) -> bool {
    let Ok(exe) = std::env::current_exe() else { return false };
    let entry = entry.replace('"', "").to_lowercase();
    let exe = exe.display().to_string().to_lowercase();
    if !entry.starts_with(&exe) {
        return false;
    }
    let rest = &entry[exe.len()..];
    rest.is_empty() || rest.starts_with(' ')
}

#[cfg(windows)]
impl<'a> Entry<'a> {
    fn of(app: &'a AppHandle) -> Self {
        Self { name: &app.package_info().name, run: RUN_PATH, task_manager: TASK_MANAGER_PATH }
    }

    fn is_enabled(&self) -> bool {
        use windows_registry::CURRENT_USER;

        let user_entry = CURRENT_USER.open(self.run).is_ok_and(|run| run.get_string(self.name).is_ok());
        if user_entry {
            // Without a value there, or with one whose state this does not know, the entry counts as
            // enabled, which is how Task Manager shows it.
            return !CURRENT_USER.open(self.task_manager).is_ok_and(|key| task_manager_disabled(&key, self.name));
        }
        // An HKLM entry that Windows does not let this user delete still starts PeriHub, so it counts
        // as enabled; `disable` then says how to remove it.
        self.machine_entry_starts()
    }

    /// Whether an entry under `HKLM` starts this copy of PeriHub: it is there and Task Manager did not
    /// turn it off. Reading needs no rights, unlike deleting it.
    fn machine_entry_starts(&self) -> bool {
        use windows_registry::LOCAL_MACHINE;

        self.machine_entry().is_some()
            && !LOCAL_MACHINE.open(self.task_manager).is_ok_and(|key| task_manager_disabled(&key, self.name))
    }

    /// The Run entry under `HKLM` when it points at this copy of PeriHub: 0.4.2 and earlier wrote it
    /// there, and Windows does not always let us delete it.
    fn machine_entry(&self) -> Option<String> {
        use windows_registry::LOCAL_MACHINE;

        let entry = LOCAL_MACHINE.options().read().open(self.run).ok()?.get_string(self.name).ok()?;
        is_our_entry(&entry).then_some(entry)
    }

    /// Moves the Run entry that 0.4.2 and earlier wrote under `HKLM` back to `HKCU`, where a
    /// per-user app belongs. Only an entry pointing at PeriHub's own exe is moved, and one that Task
    /// Manager had turned off is only deleted. It stays in `HKLM` when Windows does not let us delete
    /// it, and then no `HKCU` copy is written either, which PeriHub logs.
    fn move_from_local_machine(&self) {
        use windows_registry::{CURRENT_USER, LOCAL_MACHINE};

        let name = self.name;
        if self.machine_entry().is_none() {
            return;
        }
        let Some(value) = command_line() else { return };
        let mut options = LOCAL_MACHINE.options();
        let machine = options.read().write();
        // Task Manager can have turned the entry off: moving it would turn it on.
        let disabled = machine.open(self.task_manager).is_ok_and(|key| task_manager_disabled(&key, name));
        match machine.open(self.run).and_then(|key| key.remove_value(name)) {
            Ok(_) => {
                if disabled {
                    log::info!("removed the HKLM autostart entry that Task Manager had turned off");
                } else {
                    // The HKCU entry is written only once the HKLM one is gone. Writing it first
                    // leaves both behind when Windows refuses the delete, and this copy would be
                    // written again at every start, also after the user turned autostart off.
                    match CURRENT_USER.create(self.run).and_then(|key| key.set_string(name, &value)) {
                        Ok(_) => log::info!("moved the autostart entry from HKLM to HKCU"),
                        Err(error) => log::warn!(
                            "could not write the HKCU autostart entry that replaces the removed HKLM one: {error}"
                        ),
                    }
                }
                // The Task Manager value auto-launch wrote for the HKLM entry would stay behind.
                if let Ok(key) = machine.open(self.task_manager)
                    && let Err(error) = key.remove_value(name)
                    && error.code() != E_FILENOTFOUND
                {
                    log::warn!("could not remove the HKLM Task Manager value of the removed entry: {error}");
                }
            }
            Err(error) if error.code() == E_ACCESSDENIED => {
                log::warn!("autostart entry left in HKLM: not allowed to delete it, so no HKCU entry was written");
            }
            Err(error) => log::warn!("could not delete the HKLM autostart entry: {error}"),
        }
    }

    /// Rewrites the entry with the current quoted command line, both to quote the unquoted entries
    /// 0.3.0 and earlier wrote and to follow the exe after an update moved it.
    fn quote_entry(&self) {
        use windows_registry::CURRENT_USER;

        let Some(value) = command_line() else { return };
        let Ok(key) = CURRENT_USER.options().read().write().open(self.run) else { return };
        // auto-launch only checks that the entry exists, so a wrong value still counts as enabled.
        if key.get_string(self.name).is_ok_and(|current| current != value)
            && let Err(error) = key.set_string(self.name, &value)
        {
            log::warn!("autostart entry not quoted: {error}");
        }
    }

    fn enable(&self) -> Result<(), String> {
        use windows_registry::{CURRENT_USER, Type};

        let name = self.name;
        // The HKLM entry already starts PeriHub: a copy here would start it twice, and would stay
        // behind once that one is gone.
        if self.machine_entry_starts() {
            return Ok(());
        }
        let Some(value) = command_line() else { return Err("could not find PeriHub's own path".to_string()) };
        // create opens the key when it already exists.
        CURRENT_USER
            .create(self.run)
            .and_then(|key| key.set_string(name, &value))
            .map_err(|error| format!("{error}"))?;
        match CURRENT_USER.options().write().open(self.task_manager) {
            Ok(key) => {
                key.set_bytes(name, Type::Bytes, &TASK_MANAGER_ENABLED).map_err(|error| format!("{error}"))?;
            }
            Err(error) if error.code() == E_FILENOTFOUND => {}
            Err(error) => return Err(format!("{error}")),
        }
        Ok(())
    }

    fn disable(&self) -> Result<(), String> {
        use windows_registry::{CURRENT_USER, LOCAL_MACHINE};

        let name = self.name;
        // Like auto-launch, only the Run value is removed: the Task Manager value only matters
        // while the Run entry exists.
        match CURRENT_USER.options().write().open(self.run).and_then(|key| key.remove_value(name)) {
            Ok(_) => {}
            Err(error) if error.code() == E_FILENOTFOUND => {}
            Err(error) => return Err(format!("{error}")),
        }
        if self.machine_entry().is_none() {
            return Ok(());
        }
        // The entry under HKLM still starts PeriHub, so saying it is off would be wrong.
        match LOCAL_MACHINE.options().read().write().open(self.run).and_then(|key| key.remove_value(name)) {
            Ok(_) => {}
            Err(error) if error.code() == E_FILENOTFOUND => return Ok(()),
            // Turned off in Task Manager it does not start PeriHub, so autostart is off as asked.
            Err(error) if error.code() == E_ACCESSDENIED && !self.machine_entry_starts() => return Ok(()),
            Err(error) if error.code() == E_ACCESSDENIED => {
                return Err(
                    "it is set for all accounts in HKEY_LOCAL_MACHINE and Windows does not let this account remove it. Quit PeriHub from the tray, then start it once as administrator, signed in as this same account, which moves the entry here. Otherwise disable it in Task Manager."
                        .to_string(),
                );
            }
            Err(error) => return Err(format!("{error}")),
        }
        if let Ok(key) = LOCAL_MACHINE.options().read().write().open(self.task_manager)
            && let Err(error) = key.remove_value(name)
            && error.code() != E_FILENOTFOUND
        {
            log::warn!("could not remove the HKLM Task Manager value of the removed entry: {error}");
        }
        Ok(())
    }
}

#[cfg(all(test, windows))]
mod tests {
    use windows_registry::{CURRENT_USER, LOCAL_MACHINE, Type};

    use super::*;

    const NAME: &str = "PeriHubTest";
    const TURNED_OFF: [u8; 12] = [0x03, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];

    /// Registry keys of one test, gone again when it ends.
    struct Keys {
        root: String,
        run: String,
        task_manager: String,
    }

    impl Keys {
        fn new(test: &str) -> Self {
            let root = format!(r"SOFTWARE\PeriHubTest\{test}");
            let keys = Self { run: format!(r"{root}\Run"), task_manager: format!(r"{root}\StartupApproved"), root };
            keys.remove();
            CURRENT_USER.create(&keys.run).unwrap();
            CURRENT_USER.create(&keys.task_manager).unwrap();
            keys
        }

        /// The same keys under `HKLM`, which needs an administrator: CI runs as one, a normal
        /// account skips these tests.
        fn with_machine(test: &str) -> Option<Self> {
            let keys = Self::new(test);
            let created = LOCAL_MACHINE.create(&keys.run).and_then(|_| LOCAL_MACHINE.create(&keys.task_manager));
            // In CI a skipped test would look like a passed one.
            assert!(created.is_ok() || std::env::var_os("CI").is_none(), "cannot write HKLM: {created:?}");
            created.is_ok().then_some(keys)
        }

        fn entry(&self) -> Entry<'_> {
            Entry { name: NAME, run: &self.run, task_manager: &self.task_manager }
        }

        fn remove(&self) {
            let _ = CURRENT_USER.remove_tree(&self.root);
            let _ = LOCAL_MACHINE.remove_tree(&self.root);
        }

        fn user_run(&self) -> Option<String> {
            CURRENT_USER.open(&self.run).ok()?.get_string(NAME).ok()
        }

        fn set_user_run(&self, value: &str) {
            CURRENT_USER.create(&self.run).unwrap().set_string(NAME, value).unwrap();
        }

        fn set_user_task_manager(&self, value: &[u8]) {
            CURRENT_USER.create(&self.task_manager).unwrap().set_bytes(NAME, Type::Bytes, value).unwrap();
        }

        fn machine_run(&self) -> Option<String> {
            LOCAL_MACHINE.open(&self.run).ok()?.get_string(NAME).ok()
        }

        fn set_machine_run(&self, value: &str) {
            LOCAL_MACHINE.create(&self.run).unwrap().set_string(NAME, value).unwrap();
        }

        fn set_machine_task_manager(&self, value: &[u8]) {
            LOCAL_MACHINE.create(&self.task_manager).unwrap().set_bytes(NAME, Type::Bytes, value).unwrap();
        }

        fn machine_task_manager(&self) -> bool {
            LOCAL_MACHINE.open(&self.task_manager).is_ok_and(|key| key.get_value(NAME).is_ok())
        }
    }

    impl Drop for Keys {
        fn drop(&mut self) {
            self.remove();
        }
    }

    fn our_command() -> String {
        command_line().unwrap()
    }

    #[test]
    fn the_first_byte_tells_what_task_manager_did() {
        for on in [0x02, 0x06] {
            assert_eq!(task_manager_state(&[on, 0xff, 0xff]), Some(true));
        }
        for off in [0x03, 0x07] {
            assert_eq!(task_manager_state(&[off, 0, 0]), Some(false));
        }
        assert_eq!(task_manager_state(&[0x09]), None);
        assert_eq!(task_manager_state(&[]), None);
    }

    #[test]
    fn only_this_exe_is_our_entry() {
        let exe = std::env::current_exe().unwrap().display().to_string();
        assert!(is_our_entry(&exe));
        assert!(is_our_entry(&exe.to_uppercase()));
        assert!(is_our_entry(&format!("\"{exe}\" --hidden")));
        assert!(!is_our_entry(&format!("{exe}.old")));
        assert!(!is_our_entry(r#""C:\Other\program.exe" --hidden"#));
    }

    #[test]
    fn enabling_writes_a_quoted_entry_shown_as_enabled_in_task_manager() {
        let keys = Keys::new("enable");
        assert!(!keys.entry().is_enabled());

        keys.entry().enable().unwrap();

        let command = keys.user_run().unwrap();
        assert_eq!(command, our_command());
        assert!(command.starts_with('"') && command.ends_with(crate::HIDDEN_ARG));
        let value = CURRENT_USER.open(&keys.task_manager).unwrap().get_value(NAME).unwrap();
        assert_eq!(&*value, &TASK_MANAGER_ENABLED[..]);
        assert!(keys.entry().is_enabled());
    }

    #[test]
    fn enabling_works_without_the_task_manager_key() {
        let keys = Keys::new("enable-no-task-manager");
        CURRENT_USER.remove_tree(&keys.task_manager).unwrap();

        keys.entry().enable().unwrap();

        assert!(keys.entry().is_enabled());
    }

    #[test]
    fn disabling_removes_the_entry_and_is_fine_without_one() {
        let keys = Keys::new("disable");
        keys.entry().enable().unwrap();

        keys.entry().disable().unwrap();
        assert_eq!(keys.user_run(), None);
        assert!(!keys.entry().is_enabled());

        keys.entry().disable().unwrap();
    }

    #[test]
    fn an_entry_turned_off_in_task_manager_counts_as_off() {
        let keys = Keys::new("task-manager");
        keys.set_user_run(&our_command());
        assert!(keys.entry().is_enabled(), "no value in Task Manager means enabled");

        keys.set_user_task_manager(&TURNED_OFF);
        assert!(!keys.entry().is_enabled());

        keys.set_user_task_manager(&TASK_MANAGER_ENABLED);
        assert!(keys.entry().is_enabled());
    }

    #[test]
    fn old_entries_get_quotes_and_a_missing_one_stays_missing() {
        let keys = Keys::new("quote");
        keys.entry().quote_entry();
        assert_eq!(keys.user_run(), None);

        keys.set_user_run(r"C:\Old Path\perihub.exe --hidden");
        keys.entry().quote_entry();
        assert_eq!(keys.user_run().unwrap(), our_command());
    }

    #[test]
    fn a_machine_entry_moves_to_the_user() {
        let Some(keys) = Keys::with_machine("move") else { return };
        keys.set_machine_run(&our_command());
        keys.set_machine_task_manager(&TASK_MANAGER_ENABLED);

        keys.entry().move_from_local_machine();

        assert_eq!(keys.machine_run(), None);
        assert!(!keys.machine_task_manager());
        assert_eq!(keys.user_run().unwrap(), our_command());
    }

    #[test]
    fn a_machine_entry_turned_off_in_task_manager_is_only_deleted() {
        let Some(keys) = Keys::with_machine("move-off") else { return };
        keys.set_machine_run(&our_command());
        keys.set_machine_task_manager(&TURNED_OFF);

        keys.entry().move_from_local_machine();

        assert_eq!(keys.machine_run(), None);
        assert!(!keys.machine_task_manager());
        assert_eq!(keys.user_run(), None);
    }

    #[test]
    fn another_programs_machine_entry_is_left_alone() {
        let Some(keys) = Keys::with_machine("move-other") else { return };
        let other = r#""C:\Other\program.exe" --hidden"#;
        keys.set_machine_run(other);

        keys.entry().move_from_local_machine();

        assert_eq!(keys.machine_run().unwrap(), other);
        assert_eq!(keys.user_run(), None);
    }

    #[test]
    fn a_machine_entry_that_starts_needs_no_copy() {
        let Some(keys) = Keys::with_machine("machine-on") else { return };
        keys.set_machine_run(&our_command());
        assert!(keys.entry().is_enabled());

        keys.entry().enable().unwrap();

        assert_eq!(keys.user_run(), None, "a copy would start PeriHub twice");
    }

    #[test]
    fn a_machine_entry_turned_off_counts_as_off_and_enabling_writes_the_user_entry() {
        let Some(keys) = Keys::with_machine("machine-off") else { return };
        keys.set_machine_run(&our_command());
        keys.set_machine_task_manager(&TURNED_OFF);
        assert!(!keys.entry().is_enabled());

        keys.entry().enable().unwrap();

        assert_eq!(keys.user_run().unwrap(), our_command());
    }

    #[test]
    fn disabling_also_removes_a_machine_entry() {
        let Some(keys) = Keys::with_machine("machine-disable") else { return };
        keys.set_machine_run(&our_command());
        keys.set_machine_task_manager(&TASK_MANAGER_ENABLED);

        keys.entry().disable().unwrap();

        assert_eq!(keys.machine_run(), None);
        assert!(!keys.machine_task_manager());
        assert!(!keys.entry().is_enabled());
    }
}
