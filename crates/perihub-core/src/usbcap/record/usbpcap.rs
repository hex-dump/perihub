//! The parts of the Windows recorder that do not need Windows: what is downloaded, the PowerShell
//! steps, and how USBPcapCMD's and their answers are read. The steps are the ones checked by hand
//! in a Windows 11 virtual machine with a G502 HERO (see docs/captures.md).
//!
//! USBPcap (driver GPL-2.0, USBPcapCMD BSD-2-Clause, by Tomasz Moń) is not part of PeriHub: it is
//! downloaded from its own GitHub release when a recording starts, checked against the hash below,
//! and removed when it ends.

#![cfg_attr(not(windows), allow(dead_code))]

pub const VERSION: &str = "1.5.4.0";
pub const SETUP_URL: &str = "https://github.com/desowin/usbpcap/releases/download/1.5.4.0/USBPcapSetup-1.5.4.0.exe";
pub const SETUP_FILE: &str = "USBPcapSetup-1.5.4.0.exe";
pub const SETUP_SHA256: &str = "87a7edf9bbbcf07b5f4373d9a192a6770d2ff3add7aa1e276e82e38582ccb622";

/// Restarts the USB host controllers (PCI devices of the USB class): a filter driver is loaded or
/// unloaded without restarting Windows. Their devices disconnect for a couple of seconds.
const RESTART_CONTROLLERS: &str = r#"Get-PnpDevice -PresentOnly -Class USB | Where-Object { $_.InstanceId -like 'PCI\*' } | ForEach-Object { pnputil /restart-device "$($_.InstanceId)" | Out-Null }"#;

/// Installs USBPcap from the checked installer at `{setup}`. The installer's /S only copies the
/// files: the INF's DefaultInstall section registers the filter (rundll32 is waited for), then the
/// controllers restart and the driver runs. Prints `service=Running` when it does.
const INSTALL: &str = r#"$ErrorActionPreference = 'Stop'
$dir = Join-Path $env:ProgramFiles 'USBPcap'
$p = Start-Process -FilePath '{setup}' -ArgumentList '/S' -Wait -PassThru
if ($p.ExitCode -ne 0) { throw "the USBPcap installer ended with code $($p.ExitCode)" }
$inf = Join-Path $dir 'USBPcap.inf'
if (-not (Test-Path -LiteralPath $inf)) { throw 'USBPcap.inf is not in Program Files after the install' }
$r = Start-Process rundll32.exe -ArgumentList @('setupapi.dll,InstallHinfSection', 'DefaultInstall.NTamd64', '128', $inf) -Wait -PassThru
{restart}
$t = 0
while ((Get-Service USBPcap -ErrorAction SilentlyContinue).Status -ne 'Running' -and $t -lt 20) { Start-Sleep 1; $t++ }
'service=' + (Get-Service USBPcap -ErrorAction SilentlyContinue).Status
"#;

/// Removes everything of USBPcap. Its uninstaller removes only the files, not the driver, so this
/// is done by hand: USBPcap out of the USB class's UpperFilters (the other filters stay), the
/// controllers restarted so that it unloads, its service deleted, then its driver file, folder and
/// Apps entry. Prints what is still there, as `name=True` or `name=False`.
const REMOVE: &str = r#"$key = 'HKLM:\SYSTEM\CurrentControlSet\Control\Class\{36FC9E60-C465-11CF-8056-444553540000}'
Get-Process USBPcapCMD -ErrorAction SilentlyContinue | Stop-Process -Force
$f = @(@((Get-ItemProperty -LiteralPath $key -ErrorAction SilentlyContinue).UpperFilters) | Where-Object { $_ -and $_ -ne 'USBPcap' })
if ($f.Count -gt 0) { Set-ItemProperty -LiteralPath $key -Name UpperFilters -Type MultiString -Value $f } else { Remove-ItemProperty -LiteralPath $key -Name UpperFilters -ErrorAction SilentlyContinue }
{restart}
Start-Sleep 3
sc.exe delete USBPcap | Out-Null
Remove-Item (Join-Path $env:SystemRoot 'System32\drivers\USBPcap.sys') -Force -ErrorAction SilentlyContinue
Remove-Item (Join-Path $env:ProgramFiles 'USBPcap') -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\USBPcap' -Recurse -Force -ErrorAction SilentlyContinue
'filter=' + (@((Get-ItemProperty -LiteralPath $key -ErrorAction SilentlyContinue).UpperFilters) -contains 'USBPcap')
'service=' + (Test-Path 'HKLM:\SYSTEM\CurrentControlSet\Services\USBPcap')
'driver=' + (Test-Path (Join-Path $env:SystemRoot 'System32\drivers\USBPcap.sys'))
'folder=' + (Test-Path (Join-Path $env:ProgramFiles 'USBPcap'))
'uninstall=' + (Test-Path 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\USBPcap')
'usb=' + ($null -eq (Get-PnpDevice -PresentOnly -Class USB | Where-Object Status -ne 'OK'))
"#;

pub fn install_script(setup: &str) -> String {
    INSTALL.replace("{setup}", &setup.replace('\'', "''")).replace("{restart}", RESTART_CONTROLLERS)
}

pub fn remove_script() -> String {
    REMOVE.replace("{restart}", RESTART_CONTROLLERS)
}

/// The driver runs after the install script.
pub fn installed(output: &str) -> Result<(), String> {
    match output.lines().rev().find_map(|l| l.trim().strip_prefix("service=")) {
        Some("Running") => Ok(()),
        Some(state) => Err(format!(
            "the USBPcap driver did not start (service: {}): restarting Windows may be needed",
            if state.is_empty() { "missing" } else { state }
        )),
        None => Err(format!("the install did not finish: {}", output.trim())),
    }
}

/// What the removal script printed, in words, and whether nothing of USBPcap is left.
pub fn removal_report(output: &str) -> (bool, String) {
    let value = |name: &str| {
        output.lines().find_map(|l| l.trim().strip_prefix(&format!("{name}="))).map(|v| v.eq_ignore_ascii_case("true"))
    };
    let parts = [
        ("filter", "its entry among the USB filters"),
        ("service", "its driver service"),
        ("driver", "USBPcap.sys"),
        ("folder", "its folder in Program Files"),
        ("uninstall", "its entry in Apps"),
    ];
    let left: Vec<&str> =
        parts.iter().filter(|(name, _)| value(name) != Some(false)).map(|(_, label)| *label).collect();
    let usb_note = if value("usb") == Some(false) {
        " Device Manager shows a USB device with a problem: unplug it and plug it in again."
    } else {
        ""
    };
    if left.is_empty() {
        (true, format!("USBPcap was removed: nothing of it is left.{usb_note}"))
    } else {
        (
            false,
            format!(
                "USBPcap was not removed completely, still there: {}. Restart Windows, then open Send a report again: PeriHub offers to finish the removal.{usb_note}",
                left.join(", ")
            ),
        )
    }
}

/// The root hubs USBPcapCMD lists (`--extcap-interfaces`): `\\.\USBPcapN` with N, which is also
/// the bus number in its packets.
pub fn hubs(output: &str) -> Vec<(String, u16)> {
    let mut found = Vec::new();
    let mut rest = output;
    while let Some(at) = rest.find(r"\\.\USBPcap") {
        rest = &rest[at + r"\\.\USBPcap".len()..];
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        if let Ok(n) = digits.parse::<u16>() {
            let name = format!(r"\\.\USBPcap{n}");
            if !found.iter().any(|(d, _)| *d == name) {
                found.push((name, n));
            }
        }
    }
    found
}

/// What one hub's `--extcap-config` says of the device: whether a line of its device list names
/// it (by its product name, or "VID_xxxx&PID_xxxx"), and the device addresses on those lines.
pub fn device_on_hub(config: &str, product: Option<&str>, vendor_id: u16, product_id: u16) -> (bool, Vec<u16>) {
    let ids = format!("vid_{vendor_id:04x}&pid_{product_id:04x}");
    let product = product.map(str::to_lowercase).filter(|p| p.trim().len() >= 3);
    let mut named = false;
    let mut addresses = Vec::new();
    for line in config.lines().filter(|l| l.starts_with("value ")) {
        let lower = line.to_lowercase();
        let display = field(&lower, "display").unwrap_or_default();
        if !(display.contains(&ids) || lower.contains(&ids) || product.as_deref().is_some_and(|p| display.contains(p)))
        {
            continue;
        }
        named = true;
        if let Some(address) = field(&lower, "value").and_then(|v| v.parse::<u16>().ok()) {
            addresses.push(address);
        }
    }
    (named, addresses)
}

/// `{name=value}` in an extcap line.
fn field<'a>(line: &'a str, name: &str) -> Option<&'a str> {
    let start = line.find(&format!("{{{name}="))? + name.len() + 2;
    let end = line[start..].find('}')? + start;
    Some(&line[start..end])
}

/// USBPcapCMD offers to put the descriptors of the devices already connected at the start of the
/// capture, which tell which device is which.
pub fn injects_descriptors(config: &str) -> bool {
    config.contains("--inject-descriptors")
}

/// PowerShell's -EncodedCommand: the script in UTF-16LE, in base64 (no quoting to get wrong).
pub fn encoded_command(script: &str) -> String {
    let bytes: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    base64(&bytes)
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for k in 0..4 {
            if k <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * k)) as usize & 63] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// The product name, in hex, for the helper's command line (no spaces or quotes to escape).
pub fn hex_text(text: &str) -> String {
    text.bytes().map(|b| format!("{b:02x}")).collect()
}

pub fn text_from_hex(hex: &str) -> Option<String> {
    let bytes: Option<Vec<u8>> =
        (0..hex.len() / 2).map(|i| u8::from_str_radix(hex.get(2 * i..2 * i + 2)?, 16).ok()).collect();
    String::from_utf8(bytes?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_is_the_standard_one() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        // "ls" in UTF-16LE, as PowerShell's help gives for -EncodedCommand.
        assert_eq!(encoded_command("ls"), "bABzAA==");
    }

    #[test]
    fn the_scripts_are_the_checked_steps() {
        let install = install_script(r"C:\Windows\Temp\perihub-usbpcap-1\USBPcapSetup-1.5.4.0.exe");
        assert!(install.contains(
            r"-FilePath 'C:\Windows\Temp\perihub-usbpcap-1\USBPcapSetup-1.5.4.0.exe' -ArgumentList '/S' -Wait"
        ));
        assert!(install.contains("'DefaultInstall.NTamd64', '128', $inf) -Wait"));
        assert!(install.contains("pnputil /restart-device"));
        let remove = remove_script();
        // The other filters stay; the driver unloads before its service is deleted.
        assert!(remove.contains("Where-Object { $_ -and $_ -ne 'USBPcap' }"));
        assert!(remove.find("pnputil").unwrap() < remove.find("sc.exe delete USBPcap").unwrap());
        assert!(!remove.contains("{restart}") && !install.contains("{restart}"));
    }

    #[test]
    fn the_install_is_over_when_the_driver_runs() {
        assert!(installed("service=Running\r\n").is_ok());
        assert!(installed("service=Stopped").unwrap_err().contains("did not start (service: Stopped)"));
        assert!(installed("service=").unwrap_err().contains("service: missing"));
        assert!(installed("boom").unwrap_err().contains("did not finish: boom"));
    }

    #[test]
    fn the_removal_says_what_is_left() {
        let clean = "filter=False\nservice=False\ndriver=False\nfolder=False\nuninstall=False\nusb=True\n";
        assert_eq!(removal_report(clean), (true, "USBPcap was removed: nothing of it is left.".to_string()));
        let (done, text) =
            removal_report(&clean.replace("service=False", "service=True").replace("usb=True", "usb=False"));
        assert!(!done);
        assert!(text.contains("still there: its driver service."), "{text}");
        assert!(text.contains("Device Manager"));
        // A script that printed nothing removed nothing as far as PeriHub knows.
        assert!(!removal_report("").0);
    }

    #[test]
    fn hubs_and_devices_are_read_from_usbpcapcmd() {
        let interfaces = "extcap {version=1.5.4.0}{help=https://desowin.org/usbpcap/}\ninterface {value=\\\\.\\USBPcap1}{display=USBPcap1}\ninterface {value=\\\\.\\USBPcap3}{display=USBPcap3}\n";
        assert_eq!(hubs(interfaces), [(r"\\.\USBPcap1".to_string(), 1), (r"\\.\USBPcap3".to_string(), 3)]);
        let config = "arg {number=4}{call=--inject-descriptors}{display=Inject already connected devices descriptors into capture data}\n\
                      value {arg=99}{value=1}{display=[1] Generic USB Hub}{enabled=true}\n\
                      value {arg=99}{value=2}{display=[2] G502 HERO Gaming Mouse}{enabled=true}{parent=1}\n";
        assert!(injects_descriptors(config));
        assert_eq!(device_on_hub(config, Some("G502 HERO Gaming Mouse"), 0x046d, 0xc08b), (true, vec![2]));
        assert_eq!(device_on_hub(config, Some("Other"), 0x046d, 0xc08b), (false, vec![]));
        let by_id = "value {arg=99}{value=5}{display=USB\\VID_046D&PID_C08B}\n";
        assert_eq!(device_on_hub(by_id, None, 0x046d, 0xc08b), (true, vec![5]));
    }

    #[test]
    fn the_name_travels_in_hex() {
        assert_eq!(
            text_from_hex(&hex_text("G502 HERO \"Gaming\" Mouse")).as_deref(),
            Some("G502 HERO \"Gaming\" Mouse")
        );
        assert_eq!(text_from_hex("4"), Some(String::new()));
        assert_eq!(text_from_hex("zz"), None);
    }
}
