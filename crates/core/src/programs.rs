//! Installed programs, and launching their uninstallers.

use std::collections::HashSet;

use winreg::enums::{KEY_READ, KEY_WOW64_64KEY};
use winreg::{RegKey, HKCU, HKLM};

use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, ERROR_CANCELLED, ERROR_ELEVATION_REQUIRED};
use windows::Win32::System::Threading::{
    CreateProcessW, PROCESS_CREATION_FLAGS, PROCESS_INFORMATION, STARTUPINFOW,
};
use windows::Win32::UI::Shell::{
    ShellExecuteExW, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

use crate::model::{Program, Scope};
use crate::util::wide;
use crate::{Error, Result};

/// One Uninstall key we read.
struct Source {
    tag: &'static str,
    scope: Scope,
    path: &'static str,
}

const SOURCES: &[Source] = &[
    Source {
        tag: "hklm64",
        scope: Scope::Machine,
        path: r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
    },
    Source {
        tag: "hklm32",
        scope: Scope::Machine,
        path: r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
    },
    Source {
        tag: "hkcu",
        scope: Scope::User,
        path: r"Software\Microsoft\Windows\CurrentVersion\Uninstall",
    },
];

fn hive(scope: Scope) -> &'static RegKey {
    match scope {
        Scope::User => HKCU,
        Scope::Machine => HKLM,
    }
}

/// Everything we need from one Uninstall subkey, read once.
#[derive(Debug, Default, Clone)]
struct Entry {
    subkey: String,
    name: String,
    publisher: String,
    version: String,
    estimated_kb: Option<u32>,
    install_date: String,
    install_location: String,
    uninstall_string: String,
    windows_installer: bool,
    system_component: bool,
    has_parent: bool,
    release_type: String,
    no_remove: bool,
}

fn read_entry(key: &RegKey, subkey: &str) -> Entry {
    let s = |n: &str| key.get_value::<String, _>(n).map(|v| v.trim().to_string()).unwrap_or_default();
    let d = |n: &str| key.get_value::<u32, _>(n).ok();
    Entry {
        subkey: subkey.to_string(),
        name: s("DisplayName"),
        publisher: s("Publisher"),
        version: s("DisplayVersion"),
        estimated_kb: d("EstimatedSize"),
        install_date: s("InstallDate"),
        install_location: s("InstallLocation"),
        uninstall_string: s("UninstallString"),
        windows_installer: d("WindowsInstaller") == Some(1),
        system_component: d("SystemComponent") == Some(1),
        has_parent: !s("ParentKeyName").is_empty(),
        release_type: s("ReleaseType"),
        no_remove: d("NoRemove") == Some(1),
    }
}

/// The same filter the Windows "Installed apps" page uses, roughly.
fn is_listed(e: &Entry) -> bool {
    if e.name.is_empty() || e.system_component || e.has_parent {
        return false;
    }
    let rt = e.release_type.to_ascii_lowercase();
    !(rt.contains("update") || rt.contains("hotfix") || rt.contains("service pack"))
}

/// "YYYYMMDD" to "YYYY-MM-DD". None if it is anything else.
fn format_install_date(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.len() != 8 || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let (y, m, d) = (&raw[0..4], &raw[4..6], &raw[6..8]);
    let (mi, di): (u32, u32) = (m.parse().ok()?, d.parse().ok()?);
    if !(1..=12).contains(&mi) || !(1..=31).contains(&di) {
        return None;
    }
    Some(format!("{y}-{m}-{d}"))
}

/// A product code looks like `{8-4-4-4-12 hex}`.
fn is_product_code(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 38 || b[0] != b'{' || b[37] != b'}' {
        return false;
    }
    b[1..37].iter().enumerate().all(|(i, c)| match i {
        8 | 13 | 18 | 23 => *c == b'-',
        _ => c.is_ascii_hexdigit(),
    })
}

/// The command line we would run to uninstall this entry, if any. Always
/// built from registry data, never from anything the caller sent.
fn uninstall_command(e: &Entry) -> Option<String> {
    if e.no_remove {
        return None;
    }
    if e.windows_installer && is_product_code(&e.subkey) {
        return Some(format!("msiexec.exe /x {}", e.subkey));
    }
    let s = e.uninstall_string.trim();
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

fn make_id(src: &Source, subkey: &str) -> String {
    format!("{}|{}", src.tag, subkey)
}

/// Read every listed entry, with its id and scope, de-duplicated.
fn read_all() -> Vec<(String, Scope, Entry)> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for src in SOURCES {
        let Ok(root) = hive(src.scope).open_subkey_with_flags(src.path, KEY_READ | KEY_WOW64_64KEY)
        else {
            continue;
        };
        for sub in root.enum_keys().filter_map(|k| k.ok()) {
            let Ok(key) = root.open_subkey_with_flags(&sub, KEY_READ) else { continue };
            let e = read_entry(&key, &sub);
            if !is_listed(&e) {
                continue;
            }
            if !seen.insert((e.name.to_lowercase(), e.version.to_lowercase())) {
                continue;
            }
            out.push((make_id(src, &sub), src.scope, e));
        }
    }
    out
}

/// Programs registered with Windows as installed, sorted by name. Hides
/// system components, patches and entries with no display name, the same way
/// the Windows "Installed apps" page does.
pub fn list() -> Vec<Program> {
    let mut out: Vec<Program> = read_all()
        .into_iter()
        .map(|(id, scope, e)| Program {
            id,
            can_uninstall: uninstall_command(&e).is_some(),
            size_bytes: e.estimated_kb.filter(|kb| *kb > 0).map(|kb| u64::from(kb) * 1024),
            installed_on: format_install_date(&e.install_date),
            name: e.name,
            publisher: e.publisher,
            version: e.version,
            install_location: e.install_location,
            scope,
        })
        .collect();
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then(a.id.cmp(&b.id)));
    out
}

/// Split a command line into the program and the rest, the way Windows reads
/// the first token: quoted, or up to the first `.exe` that ends a word, or up
/// to the first space.
fn split_program(cmd: &str) -> (String, String) {
    let cmd = cmd.trim();
    if let Some(rest) = cmd.strip_prefix('"') {
        return match rest.find('"') {
            Some(end) => (rest[..end].to_string(), rest[end + 1..].trim().to_string()),
            None => (rest.to_string(), String::new()),
        };
    }
    let lower = cmd.to_ascii_lowercase();
    let mut from = 0;
    while let Some(pos) = lower[from..].find(".exe") {
        let end = from + pos + 4;
        match lower[end..].chars().next() {
            None => return (cmd.to_string(), String::new()),
            Some(c) if c.is_whitespace() => {
                return (cmd[..end].to_string(), cmd[end..].trim().to_string())
            }
            _ => from = end,
        }
    }
    match cmd.split_once(char::is_whitespace) {
        Some((p, a)) => (p.to_string(), a.trim().to_string()),
        None => (cmd.to_string(), String::new()),
    }
}

/// Start the uninstaller the program registered with Windows. Returns as soon
/// as it has started: the uninstaller shows its own window.
///
/// `id` must match an entry in a fresh `list()`, otherwise `Error::NotFound`.
/// The command line comes from the registry, never from the caller.
pub fn uninstall(id: &str) -> Result<()> {
    let (_, _, entry) = read_all()
        .into_iter()
        .find(|(eid, _, _)| eid == id)
        .ok_or_else(|| Error::not_found("That program is no longer listed. Refresh the list."))?;
    let command = uninstall_command(&entry).ok_or_else(|| {
        Error::refused(format!("{} does not offer an uninstaller.", entry.name))
    })?;
    match create_process(&command) {
        Ok(()) => Ok(()),
        Err(code) if code == ERROR_ELEVATION_REQUIRED.0 => shell_execute(&command, &entry.name),
        Err(code) => Err(Error::msg(format!(
            "Windows could not start the uninstaller for {} (error {code}).",
            entry.name
        ))),
    }
}

/// Run a command line exactly as written. Err holds the Win32 error code.
fn create_process(command: &str) -> std::result::Result<(), u32> {
    // CreateProcessW may write into the command line buffer, so it must be a
    // private, mutable, NUL-terminated copy.
    let mut cmd_w = wide(command);
    let si = STARTUPINFOW { cb: std::mem::size_of::<STARTUPINFOW>() as u32, ..Default::default() };
    let mut pi = PROCESS_INFORMATION::default();
    // SAFETY: every pointer is to a live local that outlives the call; the
    // command buffer is NUL-terminated and mutable as the API needs.
    let result = unsafe {
        CreateProcessW(
            PCWSTR::null(),
            Some(PWSTR(cmd_w.as_mut_ptr())),
            None,
            None,
            false,
            PROCESS_CREATION_FLAGS(0),
            None,
            PCWSTR::null(),
            &si,
            &mut pi,
        )
    };
    match result {
        Ok(()) => {
            // SAFETY: both handles were just returned by CreateProcessW and are
            // ours to close. We do not wait for the uninstaller.
            unsafe {
                let _ = CloseHandle(pi.hThread);
                let _ = CloseHandle(pi.hProcess);
            }
            Ok(())
        }
        Err(e) => Err((e.code().0 as u32) & 0xFFFF),
    }
}

/// Start through the shell, which shows the UAC prompt the uninstaller asks for.
fn shell_execute(command: &str, name: &str) -> Result<()> {
    let (program, params) = split_program(command);
    let file_w = wide(&program);
    let params_w = wide(&params);
    let verb_w = wide("open");
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC,
        lpVerb: PCWSTR(verb_w.as_ptr()),
        lpFile: PCWSTR(file_w.as_ptr()),
        lpParameters: if params.is_empty() { PCWSTR::null() } else { PCWSTR(params_w.as_ptr()) },
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    // SAFETY: cbSize is set, and every string is a NUL-terminated buffer that
    // lives until after the call.
    match unsafe { ShellExecuteExW(&mut info) } {
        Ok(()) => {
            if !info.hProcess.is_invalid() {
                // SAFETY: NOCLOSEPROCESS hands us the handle; we close it once.
                let _ = unsafe { CloseHandle(info.hProcess) };
            }
            Ok(())
        }
        Err(e) if (e.code().0 as u32) & 0xFFFF == ERROR_CANCELLED.0 => Err(Error::Cancelled),
        Err(_) => Err(Error::msg(format!("Windows could not start the uninstaller for {name}."))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry() -> Entry {
        Entry { name: "App".into(), subkey: "App".into(), ..Default::default() }
    }

    #[test]
    fn filters_like_installed_apps() {
        assert!(is_listed(&entry()));
        assert!(!is_listed(&Entry { name: String::new(), ..entry() }));
        assert!(!is_listed(&Entry { system_component: true, ..entry() }));
        assert!(!is_listed(&Entry { has_parent: true, ..entry() }));
        assert!(!is_listed(&Entry { release_type: "Security Update".into(), ..entry() }));
        assert!(!is_listed(&Entry { release_type: "Hotfix".into(), ..entry() }));
        assert!(is_listed(&Entry { release_type: "Application".into(), ..entry() }));
    }

    #[test]
    fn formats_dates() {
        assert_eq!(format_install_date("20240131"), Some("2024-01-31".into()));
        assert_eq!(format_install_date("2024-01-31"), None);
        assert_eq!(format_install_date("20241301"), None);
        assert_eq!(format_install_date(""), None);
        assert_eq!(format_install_date("2024013"), None);
    }

    #[test]
    fn chooses_the_uninstall_command() {
        let code = "{12345678-ABCD-ef01-2345-6789ABCDEF01}";
        let msi = Entry {
            subkey: code.into(),
            windows_installer: true,
            uninstall_string: "MsiExec.exe /I{whatever}".into(),
            ..entry()
        };
        assert_eq!(uninstall_command(&msi), Some(format!("msiexec.exe /x {code}")));

        let plain = Entry { uninstall_string: r#""C:\App\unins000.exe""#.into(), ..entry() };
        assert_eq!(uninstall_command(&plain), Some(r#""C:\App\unins000.exe""#.into()));

        assert_eq!(uninstall_command(&Entry { no_remove: true, ..plain.clone() }), None);
        assert_eq!(uninstall_command(&entry()), None);

        // A Windows Installer flag on a key that is not a product code falls
        // back to the registered string rather than inventing a command.
        let odd = Entry { windows_installer: true, uninstall_string: "x.exe".into(), ..entry() };
        assert_eq!(uninstall_command(&odd), Some("x.exe".into()));
    }

    #[test]
    fn recognises_product_codes() {
        assert!(is_product_code("{12345678-ABCD-ef01-2345-6789ABCDEF01}"));
        assert!(!is_product_code("12345678-ABCD-ef01-2345-6789ABCDEF01"));
        assert!(!is_product_code("{12345678-ABCD-ef01-2345-6789ABCDEF0G}"));
        assert!(!is_product_code("{1234 & calc}"));
    }

    #[test]
    fn splits_program_from_arguments() {
        assert_eq!(
            split_program(r#""C:\Program Files\App\unins.exe" /S"#),
            (r"C:\Program Files\App\unins.exe".into(), "/S".into())
        );
        assert_eq!(
            split_program(r"C:\Program Files\App\unins.exe /S /x"),
            (r"C:\Program Files\App\unins.exe".into(), "/S /x".into())
        );
        assert_eq!(split_program("msiexec.exe /x {A}"), ("msiexec.exe".into(), "/x {A}".into()));
        assert_eq!(split_program(r"C:\a\b.cmd"), (r"C:\a\b.cmd".into(), String::new()));
    }

    #[test]
    fn real_list_is_sorted() {
        let progs = list();
        for w in progs.windows(2) {
            assert!(w[0].name.to_lowercase() <= w[1].name.to_lowercase());
        }
        eprintln!("PROGRAMS count={}", progs.len());
    }

    #[test]
    fn unknown_id_is_not_found() {
        // Never reaches an uninstaller: the id matches nothing.
        assert!(matches!(uninstall("hkcu|no-such-program-xyz"), Err(Error::NotFound(_))));
    }
}
