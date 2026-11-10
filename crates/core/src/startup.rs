//! Startup apps: what launches when Windows starts, and switching them off.
//!
//! Switching works the way Task Manager does it: the Run value or shortcut is
//! never touched. Instead a 12-byte record under `StartupApproved` says
//! whether it is enabled, so it can always be switched back.

use std::io;
use std::path::PathBuf;

use winreg::enums::{
    RegType, KEY_READ, KEY_SET_VALUE, KEY_WOW64_64KEY, REG_BINARY,
};
use winreg::{RegKey, RegValue, HKCU, HKLM};

use crate::model::{Scope, StartupItem, StartupSource};
use crate::{Error, Result};

const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_ONCE: &str = r"Software\Microsoft\Windows\CurrentVersion\RunOnce";
const RUN_32: &str = r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Run";
const RUN_ONCE_32: &str = r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\RunOnce";
const APPROVED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved";

/// Seconds between 1601-01-01 (FILETIME epoch) and 1970-01-01.
const FILETIME_UNIX_OFFSET_SECS: u64 = 11_644_473_600;

/// One place startup entries can live.
struct Location {
    source: StartupSource,
    scope: Scope,
    /// "64" or "32": which registry view, or "" for a folder.
    view: &'static str,
    /// Registry path of the Run key, or a `%VAR%` folder template.
    path: &'static str,
    /// Value list under `StartupApproved` that holds the on/off records.
    /// None for RunOnce, which cannot be switched.
    approved: Option<&'static str>,
    label: &'static str,
}

const LOCATIONS: &[Location] = &[
    Location {
        source: StartupSource::RegistryRun,
        scope: Scope::User,
        view: "64",
        path: RUN,
        approved: Some("Run"),
        label: "Registry, this user",
    },
    Location {
        source: StartupSource::RegistryRun,
        scope: Scope::Machine,
        view: "64",
        path: RUN,
        approved: Some("Run"),
        label: "Registry, all users",
    },
    Location {
        source: StartupSource::RegistryRun,
        scope: Scope::Machine,
        view: "32",
        path: RUN_32,
        approved: Some("Run32"),
        label: "Registry, all users (32-bit)",
    },
    Location {
        source: StartupSource::RegistryRunOnce,
        scope: Scope::User,
        view: "64",
        path: RUN_ONCE,
        approved: None,
        label: "Registry, this user, runs once",
    },
    Location {
        source: StartupSource::RegistryRunOnce,
        scope: Scope::Machine,
        view: "64",
        path: RUN_ONCE,
        approved: None,
        label: "Registry, all users, runs once",
    },
    Location {
        source: StartupSource::RegistryRunOnce,
        scope: Scope::Machine,
        view: "32",
        path: RUN_ONCE_32,
        approved: None,
        label: "Registry, all users (32-bit), runs once",
    },
    Location {
        source: StartupSource::StartupFolder,
        scope: Scope::User,
        view: "",
        path: r"%APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup",
        approved: Some("StartupFolder"),
        label: "Startup folder, this user",
    },
    Location {
        source: StartupSource::StartupFolder,
        scope: Scope::Machine,
        view: "",
        path: r"%ProgramData%\Microsoft\Windows\Start Menu\Programs\Startup",
        approved: Some("StartupFolder"),
        label: "Startup folder, all users",
    },
];

fn hive(scope: Scope) -> &'static RegKey {
    match scope {
        Scope::User => HKCU,
        Scope::Machine => HKLM,
    }
}

fn source_tag(source: StartupSource) -> &'static str {
    match source {
        StartupSource::RegistryRun => "run",
        StartupSource::RegistryRunOnce => "runonce",
        StartupSource::StartupFolder => "folder",
    }
}

fn scope_tag(scope: Scope) -> &'static str {
    match scope {
        Scope::User => "user",
        Scope::Machine => "machine",
    }
}

/// Stable id: source, scope, view and the value or file name.
fn make_id(loc: &Location, name: &str) -> String {
    format!("{}|{}|{}|{}", source_tag(loc.source), scope_tag(loc.scope), loc.view, name)
}

// ── The 12-byte StartupApproved record ───────────────────────────────────────

/// Build the record Task Manager writes. Enabled: `02 00 00 00` and eight
/// zero bytes. Disabled: `03 00 00 00` and the FILETIME it was switched off.
pub fn encode_approved(enabled: bool, disabled_at_filetime: u64) -> [u8; 12] {
    let mut out = [0u8; 12];
    if enabled {
        out[0] = 0x02;
    } else {
        out[0] = 0x03;
        out[4..12].copy_from_slice(&disabled_at_filetime.to_le_bytes());
    }
    out
}

/// Read a record: byte 0 even means enabled, odd means disabled. An empty
/// value counts as enabled, the same as no value at all.
pub fn decode_approved(bytes: &[u8]) -> bool {
    match bytes.first() {
        Some(b) => b % 2 == 0,
        None => true,
    }
}

/// Now, as a Windows FILETIME (100 ns ticks since 1601).
fn filetime_now() -> u64 {
    let unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    (unix.as_secs() + FILETIME_UNIX_OFFSET_SECS) * 10_000_000 + u64::from(unix.subsec_nanos() / 100)
}

/// Write one record under `base\subkey`. Kept separate so tests can point it
/// at a scratch key instead of the real StartupApproved lists.
fn write_approved(base: &RegKey, subkey: &str, value_name: &str, enabled: bool) -> io::Result<()> {
    let (key, _) = base.create_subkey_with_flags(subkey, KEY_SET_VALUE | KEY_WOW64_64KEY)?;
    let bytes = encode_approved(enabled, filetime_now());
    key.set_raw_value(value_name, &RegValue { bytes: bytes.to_vec().into(), vtype: REG_BINARY })
}

/// Read one record. No key or no value means enabled.
fn read_approved(base: &RegKey, subkey: &str, value_name: &str) -> bool {
    let Ok(key) = base.open_subkey_with_flags(subkey, KEY_READ | KEY_WOW64_64KEY) else {
        return true;
    };
    match key.get_raw_value(value_name) {
        Ok(v) => decode_approved(&v.bytes),
        Err(_) => true,
    }
}

// ── Working out the program from a command line ──────────────────────────────

/// Best guess at the program a command line runs. Handles a quoted first
/// token, unquoted paths with spaces that end in `.exe`, and `%VARS%`.
pub fn parse_target(command: &str) -> String {
    let cmd = command.trim();
    if cmd.is_empty() {
        return String::new();
    }
    let raw = if let Some(rest) = cmd.strip_prefix('"') {
        match rest.find('"') {
            Some(end) => &rest[..end],
            None => rest,
        }
    } else {
        let lower = cmd.to_ascii_lowercase();
        match find_exe_end(&lower) {
            Some(end) => &cmd[..end],
            None => cmd.split_whitespace().next().unwrap_or(""),
        }
    };
    let raw = raw.trim();
    match crate::util::expand_env(raw) {
        Some(p) => p.to_string_lossy().into_owned(),
        None => raw.to_string(),
    }
}

/// Where the first `.exe` that ends a token finishes, for unquoted paths
/// such as `C:\Program Files\App\app.exe --tray`.
fn find_exe_end(lower: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(pos) = lower[from..].find(".exe") {
        let end = from + pos + 4;
        match lower[end..].chars().next() {
            None => return Some(end),
            Some(c) if c.is_whitespace() || c == ',' => return Some(end),
            _ => from = end,
        }
    }
    None
}

// ── Listing ──────────────────────────────────────────────────────────────────

fn registry_items(loc: &Location, out: &mut Vec<StartupItem>) {
    let base = hive(loc.scope);
    let Ok(key) = base.open_subkey_with_flags(loc.path, KEY_READ | KEY_WOW64_64KEY) else {
        return;
    };
    for (name, value) in key.enum_values().filter_map(|v| v.ok()) {
        // The unnamed default value is not a startup entry.
        if name.is_empty() {
            continue;
        }
        let command = match value.vtype {
            RegType::REG_SZ | RegType::REG_EXPAND_SZ => {
                winreg::types::FromRegValue::from_reg_value(&value).unwrap_or_else(|_| String::new())
            }
            _ => continue,
        };
        let enabled = match loc.approved {
            Some(list) => read_approved(base, &format!(r"{APPROVED}\{list}"), &name),
            None => true,
        };
        out.push(StartupItem {
            id: make_id(loc, &name),
            target: parse_target(&command),
            name,
            command,
            source: loc.source,
            scope: loc.scope,
            location: loc.label.to_string(),
            enabled,
            can_toggle: loc.approved.is_some(),
            needs_admin: loc.scope == Scope::Machine,
        });
    }
}

fn folder_items(loc: &Location, out: &mut Vec<StartupItem>) {
    let Some(dir) = crate::util::expand_env(loc.path) else { return };
    let Ok(entries) = std::fs::read_dir(&dir) else { return };
    let base = hive(loc.scope);
    for entry in entries.filter_map(|e| e.ok()) {
        let is_file = entry.file_type().map(|t| t.is_file()).unwrap_or(false);
        if !is_file {
            continue;
        }
        let file_name = entry.file_name().to_string_lossy().into_owned();
        if file_name.eq_ignore_ascii_case("desktop.ini") {
            continue;
        }
        let path: PathBuf = entry.path();
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| file_name.clone());
        let full = path.to_string_lossy().into_owned();
        let enabled = match loc.approved {
            Some(list) => read_approved(base, &format!(r"{APPROVED}\{list}"), &file_name),
            None => true,
        };
        out.push(StartupItem {
            id: make_id(loc, &file_name),
            name,
            command: full.clone(),
            target: full,
            source: loc.source,
            scope: loc.scope,
            location: loc.label.to_string(),
            enabled,
            can_toggle: loc.approved.is_some(),
            needs_admin: loc.scope == Scope::Machine,
        });
    }
}

/// Everything registered to start with Windows: the Run and RunOnce registry
/// keys (this user and all users, 64-bit and 32-bit views) and both Startup
/// folders. Sorted by name.
pub fn list() -> Vec<StartupItem> {
    let mut out = Vec::new();
    for loc in LOCATIONS {
        match loc.source {
            StartupSource::StartupFolder => folder_items(loc, &mut out),
            _ => registry_items(loc, &mut out),
        }
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then(a.id.cmp(&b.id)));
    out
}

/// Switch a startup item on or off the same way Task Manager does, by
/// writing its `StartupApproved` record. Nothing is deleted, so it can always
/// be switched back.
///
/// `id` must match an item in a fresh `list()`, otherwise `Error::NotFound`.
/// Machine-wide items need administrator rights: `Error::Denied` without.
pub fn set_enabled(id: &str, enabled: bool) -> Result<()> {
    let items = list();
    let item = items
        .iter()
        .find(|i| i.id == id)
        .ok_or_else(|| Error::not_found("That startup item is no longer there. Refresh the list."))?;
    if !item.can_toggle {
        return Err(Error::refused(
            "This item runs only once and Windows removes it by itself, so it cannot be switched off.",
        ));
    }
    let loc = LOCATIONS
        .iter()
        .find(|l| id.starts_with(&make_id(l, "")))
        .ok_or_else(|| Error::not_found("That startup item is no longer there. Refresh the list."))?;
    let list_name = loc.approved.ok_or_else(|| Error::refused("This item cannot be switched."))?;
    // The value name is whatever follows the location prefix in the id.
    let value_name = &id[make_id(loc, "").len()..];

    write_approved(hive(loc.scope), &format!(r"{APPROVED}\{list_name}"), value_name, enabled)
        .map_err(|e| {
            if e.kind() == io::ErrorKind::PermissionDenied {
                Error::denied(
                    "Changing this startup item needs administrator rights. Restart Quadrant as administrator and try again.",
                )
            } else {
                Error::Io(e)
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_and_decodes_records() {
        let on = encode_approved(true, 12345);
        assert_eq!(on, [2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        assert!(decode_approved(&on));

        let off = encode_approved(false, 0x0102_0304_0506_0708);
        assert_eq!(off[0], 3);
        assert_eq!(&off[4..12], &0x0102_0304_0506_0708u64.to_le_bytes());
        assert!(!decode_approved(&off));

        assert!(decode_approved(&[]));
        assert!(decode_approved(&[6, 0, 0, 0]));
        assert!(!decode_approved(&[7]));
    }

    #[test]
    fn filetime_is_after_2020() {
        // 2020-01-01 as a FILETIME.
        assert!(filetime_now() > 132_223_104_000_000_000);
    }

    #[test]
    fn parses_targets() {
        assert_eq!(
            parse_target(r#""C:\Program Files\App\app.exe" --tray"#),
            r"C:\Program Files\App\app.exe"
        );
        assert_eq!(
            parse_target(r"C:\Program Files\App\app.exe --tray"),
            r"C:\Program Files\App\app.exe"
        );
        assert_eq!(parse_target(r"C:\Tools\thing.exe"), r"C:\Tools\thing.exe");
        assert_eq!(parse_target(r"rundll32.exe,foo bar"), "rundll32.exe");
        assert_eq!(parse_target(r"C:\a\script.cmd /q"), r"C:\a\script.cmd");
        assert_eq!(parse_target("   "), "");
        assert_eq!(parse_target(r#""C:\unterminated\app.exe"#), r"C:\unterminated\app.exe");

        std::env::set_var("QUADRANT_STARTUP_TEST", r"C:\Expanded");
        assert_eq!(
            parse_target(r#""%QUADRANT_STARTUP_TEST%\app.exe" -x"#),
            r"C:\Expanded\app.exe"
        );
        // A missing variable is left as written rather than guessed.
        assert_eq!(
            parse_target(r"%QUADRANT_NOT_SET_EVER%\app.exe"),
            r"%QUADRANT_NOT_SET_EVER%\app.exe"
        );
    }

    #[test]
    fn ids_are_stable_and_distinct() {
        let a = make_id(&LOCATIONS[1], "Thing");
        let b = make_id(&LOCATIONS[2], "Thing");
        assert_ne!(a, b);
        assert_eq!(a, "run|machine|64|Thing");
        assert_eq!(b, "run|machine|32|Thing");
    }

    /// Deletes the scratch key even if an assertion fails.
    struct Scratch(String);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = HKCU.delete_subkey_all(&self.0);
        }
    }

    #[test]
    fn writes_records_to_a_scratch_key() {
        let root = format!(r"Software\QuadrantTest-{}", std::process::id());
        let _guard = Scratch(root.clone());
        let sub = format!(r"{root}\StartupApproved\Run");

        assert!(read_approved(HKCU, &sub, "Example"), "missing means enabled");
        write_approved(HKCU, &sub, "Example", false).unwrap();
        assert!(!read_approved(HKCU, &sub, "Example"));
        let raw = HKCU.open_subkey(&sub).unwrap().get_raw_value("Example").unwrap();
        assert_eq!(raw.bytes.len(), 12);
        assert_eq!(raw.vtype, REG_BINARY);
        write_approved(HKCU, &sub, "Example", true).unwrap();
        assert!(read_approved(HKCU, &sub, "Example"));
    }

    #[test]
    fn real_list_is_sorted_and_sane() {
        let items = list();
        for w in items.windows(2) {
            assert!(w[0].name.to_lowercase() <= w[1].name.to_lowercase());
        }
        for i in &items {
            assert_eq!(i.needs_admin, i.scope == Scope::Machine);
            if i.source == StartupSource::RegistryRunOnce {
                assert!(!i.can_toggle);
            }
        }
        eprintln!("STARTUP count={}", items.len());
    }

    #[test]
    fn unknown_id_is_not_found() {
        assert!(matches!(set_enabled("run|user|64|no-such-thing-xyz", false), Err(Error::NotFound(_))));
    }
}
