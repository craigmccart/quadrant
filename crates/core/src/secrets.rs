//! Where the Gemini API key lives: Windows Credential Manager, under the
//! signed-in user. The key never goes to the web view and is never logged.
//!
//! No function here puts the key into an error message, a log line or a
//! Debug print. Errors carry only the Windows error number.

use std::path::{Path, PathBuf};

use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::ERROR_NOT_FOUND;
use windows::Win32::Security::Credentials::{
    CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE,
    CRED_TYPE_GENERIC,
};

use crate::util::wide;
use crate::{Error, Result};

const TARGET: &str = "Quadrant/GeminiApiKey";
const MAX_KEY_BYTES: usize = 512;
const LEGACY_FILE: &str = ".winscan_key";

/// The low 16 bits of an HRESULT built from a Win32 error.
fn win32_code(e: &windows::core::Error) -> u32 {
    (e.code().0 as u32) & 0xFFFF
}

fn validate(key: &str) -> Result<&str> {
    let key = key.trim();
    if key.is_empty() {
        return Err(Error::msg("Enter an API key first."));
    }
    if key.len() > MAX_KEY_BYTES {
        return Err(Error::msg("That API key is too long. Check it was pasted correctly."));
    }
    Ok(key)
}

fn get_at(target: &str) -> Result<Option<String>> {
    let target_w = wide(target);
    let mut cred: *mut CREDENTIALW = std::ptr::null_mut();
    // SAFETY: target_w is NUL-terminated; on success Windows allocates the
    // credential and we free it with CredFree below, exactly once.
    let read = unsafe { CredReadW(PCWSTR(target_w.as_ptr()), CRED_TYPE_GENERIC, None, &mut cred) };
    if let Err(e) = read {
        if win32_code(&e) == ERROR_NOT_FOUND.0 {
            return Ok(None);
        }
        return Err(Error::msg(format!(
            "Could not read the saved API key from Credential Manager (error {}).",
            win32_code(&e)
        )));
    }
    if cred.is_null() {
        return Ok(None);
    }
    // SAFETY: `cred` is a valid CREDENTIALW from CredReadW. The blob pointer
    // and size describe memory inside that allocation, which we copy out
    // before freeing it.
    let bytes = unsafe {
        let c = &*cred;
        let bytes = if c.CredentialBlob.is_null() || c.CredentialBlobSize == 0 {
            Vec::new()
        } else {
            std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize).to_vec()
        };
        CredFree(cred as *const _);
        bytes
    };
    let key = String::from_utf8_lossy(&bytes).trim().to_string();
    Ok(if key.is_empty() { None } else { Some(key) })
}

fn set_at(target: &str, key: &str) -> Result<()> {
    let key = validate(key)?;
    let mut target_w = wide(target);
    let mut blob = key.as_bytes().to_vec();
    let cred = CREDENTIALW {
        Type: CRED_TYPE_GENERIC,
        TargetName: PWSTR(target_w.as_mut_ptr()),
        CredentialBlobSize: blob.len() as u32,
        CredentialBlob: blob.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        ..Default::default()
    };
    // SAFETY: every pointer in `cred` points at a live local buffer of the
    // stated size, and CredWriteW copies what it needs before returning.
    let result = unsafe { CredWriteW(&cred, 0) };
    // Do not leave a copy of the key lying in this buffer.
    blob.iter_mut().for_each(|b| *b = 0);
    result.map_err(|e| {
        Error::msg(format!(
            "Could not save the API key to Credential Manager (error {}).",
            win32_code(&e)
        ))
    })
}

fn clear_at(target: &str) -> Result<()> {
    let target_w = wide(target);
    // SAFETY: target_w is a NUL-terminated buffer that outlives the call.
    match unsafe { CredDeleteW(PCWSTR(target_w.as_ptr()), CRED_TYPE_GENERIC, None) } {
        Ok(()) => Ok(()),
        Err(e) if win32_code(&e) == ERROR_NOT_FOUND.0 => Ok(()),
        Err(e) => Err(Error::msg(format!(
            "Could not remove the saved API key (error {}).",
            win32_code(&e)
        ))),
    }
}

/// The saved key, if there is one.
pub fn get_api_key() -> Result<Option<String>> {
    get_at(TARGET)
}

/// Save the key, replacing any existing one. Rejects an empty key.
pub fn set_api_key(key: &str) -> Result<()> {
    set_at(TARGET, key)
}

/// Forget the saved key. Fine to call when there is none.
pub fn clear_api_key() -> Result<()> {
    clear_at(TARGET)
}

/// `.winscan_key` in `dir`, if it exists and is not empty.
fn legacy_key_in(dir: &Path) -> Option<PathBuf> {
    let path = dir.join(LEGACY_FILE);
    let meta = std::fs::metadata(&path).ok()?;
    (meta.is_file() && meta.len() > 0).then_some(path)
}

/// The old plain-text `.winscan_key` file from the Python version, if one
/// sits next to the running EXE.
pub fn legacy_key_file() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    legacy_key_in(exe.parent()?)
}

fn import_from(path: &Path, target: &str) -> Result<bool> {
    let text = std::fs::read_to_string(path)
        .map_err(|_| Error::msg("Could not read the old key file."))?;
    let key = text.trim();
    if key.is_empty() {
        return Ok(false);
    }
    set_at(target, key)?;
    Ok(true)
}

/// Copy the key from the legacy file into Credential Manager. Leaves the
/// file where it is. Ok(false) when there is no legacy file.
pub fn import_legacy_key() -> Result<bool> {
    match legacy_key_file() {
        Some(path) => import_from(&path, TARGET),
        None => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Every test uses its own throwaway target, never the real one, and
    // never reads the real `.winscan_key` file.

    /// Deletes the test credential even if an assertion fails.
    struct TestTarget(String);
    impl TestTarget {
        fn new(tag: &str) -> Self {
            TestTarget(format!("Quadrant/Test-{}-{tag}", std::process::id()))
        }
    }
    impl Drop for TestTarget {
        fn drop(&mut self) {
            let _ = clear_at(&self.0);
        }
    }

    #[test]
    fn round_trip() {
        let t = TestTarget::new("roundtrip");
        assert_eq!(get_at(&t.0).unwrap(), None);
        set_at(&t.0, "  first-test-value  ").unwrap();
        assert_eq!(get_at(&t.0).unwrap().as_deref(), Some("first-test-value"));
        set_at(&t.0, "second-test-value").unwrap();
        assert_eq!(get_at(&t.0).unwrap().as_deref(), Some("second-test-value"));
        clear_at(&t.0).unwrap();
        assert_eq!(get_at(&t.0).unwrap(), None);
        // Clearing again is fine.
        clear_at(&t.0).unwrap();
    }

    #[test]
    fn rejects_bad_keys_without_echoing_them() {
        let t = TestTarget::new("reject");
        assert!(set_at(&t.0, "").is_err());
        assert!(set_at(&t.0, "   \t ").is_err());
        let long = "k".repeat(MAX_KEY_BYTES + 1);
        let err = set_at(&t.0, &long).unwrap_err().to_string();
        assert!(!err.contains(&long[..20]));
        assert_eq!(get_at(&t.0).unwrap(), None);
        set_at(&t.0, &"k".repeat(MAX_KEY_BYTES)).unwrap();
    }

    #[test]
    fn legacy_file_handling() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(legacy_key_in(dir.path()), None);
        std::fs::write(dir.path().join(LEGACY_FILE), "").unwrap();
        assert_eq!(legacy_key_in(dir.path()), None, "empty file is ignored");
        std::fs::write(dir.path().join(LEGACY_FILE), "  legacy-test-value\r\n").unwrap();
        let path = legacy_key_in(dir.path()).unwrap();

        let t = TestTarget::new("legacy");
        assert!(import_from(&path, &t.0).unwrap());
        assert_eq!(get_at(&t.0).unwrap().as_deref(), Some("legacy-test-value"));
        assert!(path.exists(), "the file is left where it is");
    }
}
