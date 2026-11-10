//! Opening Windows' own pages and folders. Everything goes through a fixed
//! allowlist: the UI sends an id, never a URI or a command.

use std::path::{Path, PathBuf};

use windows::core::PCWSTR;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

use crate::util::{hidden_command, wide};
use crate::{Error, Result};

/// Ids accepted by `open_known`.
pub const KNOWN_PAGES: &[&str] = &[
    "windows-security",
    "firewall",
    "windows-update",
    "device-encryption",
    "uac",
    "remote-desktop",
    "installed-apps",
    "startup-apps",
    "storage",
    "task-scheduler",
    "startup-folder",
    "recycle-bin",
];

/// The fixed target for each id. The caller's string is only a lookup key.
fn target_for(id: &str) -> Option<&'static str> {
    Some(match id {
        "windows-security" => "windowsdefender:",
        "firewall" => "windowsdefender://network/",
        "windows-update" => "ms-settings:windowsupdate",
        "device-encryption" => "ms-settings:deviceencryption",
        "uac" => "UserAccountControlSettings.exe",
        "remote-desktop" => "ms-settings:remotedesktop",
        "installed-apps" => "ms-settings:appsfeatures",
        "startup-apps" => "ms-settings:startupapps",
        "storage" => "ms-settings:storagesense",
        "task-scheduler" => "taskschd.msc",
        "startup-folder" => "shell:startup",
        "recycle-bin" => "shell:RecycleBinFolder",
        _ => return None,
    })
}

/// Open a Windows settings page, tool or folder by id. Unknown ids are an
/// `Error::NotFound`.
pub fn open_known(id: &str) -> Result<()> {
    let target = target_for(id).ok_or_else(|| Error::not_found("That Windows page is not one Quadrant knows."))?;
    let verb = wide("open");
    let file = wide(target);
    // SAFETY: both strings are NUL-terminated buffers that outlive the call.
    let result = unsafe {
        ShellExecuteW(None, PCWSTR(verb.as_ptr()), PCWSTR(file.as_ptr()), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL)
    };
    // ShellExecuteW returns a fake HINSTANCE: anything above 32 is success.
    if result.0 as usize > 32 {
        Ok(())
    } else {
        Err(Error::msg("Windows could not open that page."))
    }
}

/// Check a path is safe to hand to File Explorer: absolute, existing, and
/// free of the quote character (which Windows paths cannot contain anyway).
fn validate_reveal_path(path: &Path) -> Result<PathBuf> {
    let text = path.to_string_lossy();
    if !path.is_absolute() || text.contains('"') {
        return Err(Error::not_found(format!("{} is not a full path that exists", path.display())));
    }
    if !path.exists() {
        return Err(Error::not_found(format!("{} no longer exists", path.display())));
    }
    Ok(path.to_path_buf())
}

/// The single argument for `explorer.exe`: `/select,"<path>"`.
fn select_arg(path: &Path) -> String {
    format!("/select,\"{}\"", path.display())
}

/// Show a file or folder in File Explorer, selected. The path must be
/// absolute and must exist.
pub fn reveal_in_explorer(path: &Path) -> Result<()> {
    let path = validate_reveal_path(path)?;
    // raw_arg passes the argument exactly as built (Explorer wants the quotes
    // after the comma). No shell is involved, and the path cannot contain a
    // quote, so it cannot break out of its argument.
    use std::os::windows::process::CommandExt;
    hidden_command("explorer.exe")
        .raw_arg(select_arg(&path))
        .spawn()
        .map(|_| ())
        .map_err(|_| Error::msg("Windows could not open File Explorer."))
}

#[cfg(test)]
mod tests {
    use super::*;

    // open_known and reveal_in_explorer are never called for real here: they
    // would open windows on the desktop.

    #[test]
    fn every_known_page_has_a_target() {
        for id in KNOWN_PAGES {
            assert!(target_for(id).is_some(), "{id} has no target");
        }
    }

    #[test]
    fn unknown_ids_are_refused() {
        assert!(target_for("cmd.exe").is_none());
        assert!(target_for("ms-settings:windowsupdate").is_none());
        assert!(target_for("").is_none());
        assert!(target_for("Firewall").is_none());
        assert!(matches!(open_known("not-a-page"), Err(Error::NotFound(_))));
    }

    #[test]
    fn validates_reveal_paths() {
        let dir = tempfile::tempdir().unwrap();
        assert!(validate_reveal_path(dir.path()).is_ok());
        assert!(matches!(validate_reveal_path(Path::new(r"relative\thing")), Err(Error::NotFound(_))));
        assert!(matches!(validate_reveal_path(&dir.path().join("missing.txt")), Err(Error::NotFound(_))));
        assert!(validate_reveal_path(Path::new("C:\\a\"b")).is_err());
        assert!(matches!(reveal_in_explorer(Path::new("nope")), Err(Error::NotFound(_))));
        assert_eq!(select_arg(Path::new(r"C:\A B\c.txt")), r#"/select,"C:\A B\c.txt""#);
    }
}
