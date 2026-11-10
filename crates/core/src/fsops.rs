//! Removing files the user picked in Large Files or Duplicates.
//!
//! This file also holds the low-level pieces that every scanner and cleaner
//! in the crate shares: reading file attributes, turning paths into
//! comparable keys, and deleting through an open handle so that the thing we
//! checked is exactly the thing we remove.

use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::{Component, Path, PathBuf, Prefix};

use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, FILETIME, HANDLE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FileDispositionInfo, FileDispositionInfoEx, GetFileInformationByHandle,
    GetFinalPathNameByHandleW, SetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, DELETE,
    FILE_DISPOSITION_FLAG_DELETE, FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE,
    FILE_DISPOSITION_FLAG_POSIX_SEMANTICS, FILE_DISPOSITION_INFO, FILE_DISPOSITION_INFO_EX, FILE_DISPOSITION_INFO_EX_FLAGS,
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_NAME_NORMALIZED,
    FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};

use crate::model::{RemoveFailure, RemoveReport};
use crate::util::expand_env;
use crate::Ctx;

pub(crate) mod walk;

// ── File attributes ──────────────────────────────────────────────────────────

pub(crate) const ATTR_DIRECTORY: u32 = 0x10;
pub(crate) const ATTR_REPARSE_POINT: u32 = 0x400;
pub(crate) const ATTR_OFFLINE: u32 = 0x1000;
pub(crate) const ATTR_RECALL_ON_OPEN: u32 = 0x4_0000;
pub(crate) const ATTR_RECALL_ON_DATA_ACCESS: u32 = 0x40_0000;

/// Junctions, symlinks, mount points and cloud files all carry this bit.
/// We never walk into a directory that has it.
pub(crate) fn is_reparse(attrs: u32) -> bool {
    attrs & ATTR_REPARSE_POINT != 0
}

pub(crate) fn is_dir_attr(attrs: u32) -> bool {
    attrs & ATTR_DIRECTORY != 0
}

/// A cloud file (OneDrive and the like) whose content is not on this disk.
/// It takes no local space, and reading it would start a download.
pub(crate) fn is_cloud_placeholder(attrs: u32) -> bool {
    attrs & (ATTR_OFFLINE | ATTR_RECALL_ON_OPEN | ATTR_RECALL_ON_DATA_ACCESS) != 0
}

/// Space a file really uses on this disk, as far as we count it.
pub(crate) fn local_bytes(attrs: u32, len: u64) -> u64 {
    if is_cloud_placeholder(attrs) {
        0
    } else {
        len
    }
}

// ── Path keys ────────────────────────────────────────────────────────────────

/// A path as lower-cased components, for case-insensitive, component-wise
/// comparison. The first element is the volume (`c:` or `\\server\share`),
/// whether or not the path had a `\\?\` prefix, so a canonical path and a
/// plain one compare equal.
///
/// None for anything we cannot reason about safely: relative paths, paths
/// containing `.` or `..`, and device paths.
pub(crate) fn path_key(path: &Path) -> Option<Vec<String>> {
    let mut comps = path.components();
    let volume = match comps.next()? {
        Component::Prefix(p) => match p.kind() {
            Prefix::Disk(d) | Prefix::VerbatimDisk(d) => {
                format!("{}:", (d as char).to_ascii_lowercase())
            }
            Prefix::UNC(server, share) | Prefix::VerbatimUNC(server, share) => format!(
                r"\\{}\{}",
                server.to_string_lossy().to_lowercase(),
                share.to_string_lossy().to_lowercase()
            ),
            _ => return None,
        },
        _ => return None,
    };
    if comps.next()? != Component::RootDir {
        return None;
    }
    let mut key = vec![volume];
    for c in comps {
        match c {
            Component::Normal(name) => key.push(name.to_string_lossy().to_lowercase()),
            _ => return None,
        }
    }
    Some(key)
}

/// True when `inner` is `outer` or sits somewhere below it.
pub(crate) fn key_starts_with(inner: &[String], outer: &[String]) -> bool {
    inner.len() >= outer.len() && inner[..outer.len()] == outer[..]
}

/// True when `inner` sits strictly below `outer`.
pub(crate) fn key_strictly_inside(inner: &[String], outer: &[String]) -> bool {
    inner.len() > outer.len() && key_starts_with(inner, outer)
}

// ── Protected locations ──────────────────────────────────────────────────────

/// The system folders Quadrant never removes anything from, read once so a
/// loop over thousands of files does not re-read the environment each time.
pub(crate) struct Protector {
    roots: Vec<Vec<String>>,
}

impl Protector {
    pub(crate) fn from_env() -> Self {
        let vars = [
            "%SystemRoot%",
            "%windir%",
            "%ProgramFiles%",
            "%ProgramFiles(x86)%",
            "%ProgramW6432%",
            "%ProgramData%",
        ];
        let roots = vars
            .iter()
            .filter_map(|v| expand_env(v))
            .filter_map(|p| path_key(&p))
            .collect();
        Self { roots }
    }

    pub(crate) fn is_protected(&self, path: &Path) -> bool {
        // A path we cannot reason about is treated as protected.
        let Some(key) = path_key(path) else { return true };
        if key.len() <= 1 {
            return true;
        }
        if key.len() == 2 && matches!(key[1].as_str(), "pagefile.sys" | "hiberfil.sys" | "swapfile.sys")
        {
            return true;
        }
        if key[1..].iter().any(|c| c == "$recycle.bin" || c == "system volume information") {
            return true;
        }
        self.roots.iter().any(|r| key_starts_with(&key, r))
    }
}

/// True when Quadrant must never remove this path: anything inside the Windows
/// folder, Program Files, ProgramData, the page, hibernation and swap files,
/// and drive roots themselves.
pub fn is_protected(path: &Path) -> bool {
    Protector::from_env().is_protected(path)
}

// ── Deleting through a handle ────────────────────────────────────────────────

/// What an entry looked like through the handle we are about to delete with.
#[derive(Debug, Clone)]
pub(crate) struct HandleInfo {
    pub attrs: u32,
    pub bytes: u64,
    pub created: Option<i64>,
    pub modified: Option<i64>,
    /// Where the handle really points, with any junctions above it resolved.
    pub final_path: PathBuf,
}

#[derive(Debug)]
pub(crate) enum DeleteError {
    Io(io::Error),
    /// The check refused it. The text is shown to the user.
    Refused(&'static str),
}

impl DeleteError {
    pub(crate) fn reason(&self) -> String {
        match self {
            DeleteError::Refused(r) => (*r).to_string(),
            DeleteError::Io(e) => io_reason(e),
        }
    }
}

/// A plain reason for an OS error, without blaming anyone.
pub(crate) fn io_reason(e: &io::Error) -> String {
    match e.raw_os_error() {
        Some(32) | Some(33) => "In use by another program".to_string(),
        Some(5) => "Access denied, or in use".to_string(),
        Some(2) | Some(3) => "No longer there".to_string(),
        Some(145) => "Folder is not empty".to_string(),
        _ => match e.kind() {
            io::ErrorKind::PermissionDenied => "Access denied, or in use".to_string(),
            io::ErrorKind::NotFound => "No longer there".to_string(),
            _ => format!("Windows could not remove it ({e})"),
        },
    }
}

/// Closes the handle however we leave the function.
struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // SAFETY: the handle came from a successful CreateFileW and is closed
        // exactly once, here.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// FILETIME (100 ns ticks since 1601) as Unix seconds, rounded down like
/// `util::to_unix` so the two agree. Times before 1970 become 0.
fn filetime_unix(ft: FILETIME) -> Option<i64> {
    let ticks = ((ft.dwHighDateTime as u64) << 32) | ft.dwLowDateTime as u64;
    if ticks == 0 {
        return None;
    }
    const EPOCH_DIFF: u64 = 116_444_736_000_000_000;
    Some((ticks.saturating_sub(EPOCH_DIFF) / 10_000_000) as i64)
}

/// The path as a NUL-terminated wide string, with a `\\?\` prefix so paths
/// longer than 260 characters still work. Only done for clean absolute paths,
/// because the prefix switches off Windows' own path tidying.
fn wide_for_open(path: &Path) -> Vec<u16> {
    let clean = path_key(path).is_some();
    let raw: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .map(|c| if c == u16::from(b'/') { u16::from(b'\\') } else { c })
        .collect();
    let s = String::from_utf16_lossy(&raw);
    let mut out: Vec<u16> = Vec::with_capacity(raw.len() + 8);
    if clean && !s.starts_with(r"\\?\") && !s.starts_with(r"\\.\") {
        if let Some(rest) = s.strip_prefix(r"\\") {
            out.extend(r"\\?\UNC\".encode_utf16());
            out.extend(rest.encode_utf16());
        } else {
            out.extend(r"\\?\".encode_utf16());
            out.extend(raw.iter().copied());
        }
    } else {
        out.extend(raw.iter().copied());
    }
    out.push(0);
    out
}

fn final_path(handle: HANDLE) -> io::Result<PathBuf> {
    let mut buf = vec![0u16; 1024];
    loop {
        // SAFETY: the handle is open for the duration of the call and the
        // buffer is a valid, writable slice whose length is passed with it.
        let n = unsafe { GetFinalPathNameByHandleW(handle, &mut buf, FILE_NAME_NORMALIZED) } as usize;
        if n == 0 {
            return Err(io::Error::last_os_error());
        }
        if n < buf.len() {
            return Ok(PathBuf::from(String::from_utf16_lossy(&buf[..n])));
        }
        if buf.len() >= 65_536 {
            return Err(io::Error::other("path too long"));
        }
        buf.resize(n + 1, 0);
    }
}

/// Open `path` without following any link, show what we found to `check`,
/// and delete it only if `check` agrees. Because the check and the delete use
/// the same handle, nobody can swap the file for a link in between.
///
/// A link (junction, symlink) is deleted as a link: the target is untouched.
/// A folder is only removed if it is empty.
pub(crate) fn delete_checked<F>(path: &Path, check: F) -> Result<HandleInfo, DeleteError>
where
    F: FnOnce(&HandleInfo) -> Result<(), &'static str>,
{
    let wide = wide_for_open(path);
    // SAFETY: `wide` is NUL-terminated and outlives the call. No security
    // attributes or template handle are passed.
    let handle = unsafe {
        CreateFileW(
            PCWSTR(wide.as_ptr()),
            DELETE.0 | FILE_READ_ATTRIBUTES.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS,
            None,
        )
    }
    .map_err(|e| DeleteError::Io(io::Error::from_raw_os_error(e.code().0 & 0xFFFF)))?;
    let handle = OwnedHandle(handle);

    let mut raw = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: the handle is open and `raw` is a valid struct for the call.
    unsafe { GetFileInformationByHandle(handle.0, &mut raw) }
        .map_err(|_| DeleteError::Io(io::Error::last_os_error()))?;
    let info = HandleInfo {
        attrs: raw.dwFileAttributes,
        bytes: ((raw.nFileSizeHigh as u64) << 32) | raw.nFileSizeLow as u64,
        created: filetime_unix(raw.ftCreationTime),
        modified: filetime_unix(raw.ftLastWriteTime),
        final_path: final_path(handle.0).map_err(DeleteError::Io)?,
    };
    check(&info).map_err(DeleteError::Refused)?;

    // Prefer the extended form: it ignores the read-only flag and removes the
    // name straight away. Older Windows and FAT drives only have the basic one.
    let ex = FILE_DISPOSITION_INFO_EX {
        Flags: FILE_DISPOSITION_INFO_EX_FLAGS(
            FILE_DISPOSITION_FLAG_DELETE.0
                | FILE_DISPOSITION_FLAG_POSIX_SEMANTICS.0
                | FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE.0,
        ),
    };
    // SAFETY: the pointer and size describe `ex`, which lives for the call.
    let done = unsafe {
        SetFileInformationByHandle(
            handle.0,
            FileDispositionInfoEx,
            &ex as *const _ as *const core::ffi::c_void,
            std::mem::size_of::<FILE_DISPOSITION_INFO_EX>() as u32,
        )
    };
    if done.is_err() {
        let basic = FILE_DISPOSITION_INFO { DeleteFile: true };
        // SAFETY: as above, for `basic`.
        unsafe {
            SetFileInformationByHandle(
                handle.0,
                FileDispositionInfo,
                &basic as *const _ as *const core::ffi::c_void,
                std::mem::size_of::<FILE_DISPOSITION_INFO>() as u32,
            )
        }
        .map_err(|e| DeleteError::Io(io::Error::from_raw_os_error(e.code().0 & 0xFFFF)))?;
    }
    // The entry goes when the handle closes.
    drop(handle);
    Ok(info)
}

// ── Removing user-picked files ───────────────────────────────────────────────

/// One file the user asked to remove, with what it looked like when scanned.
#[derive(Clone, Debug)]
pub struct RemoveTarget {
    pub id: u32,
    pub path: PathBuf,
    /// Size at scan time. If the file is a different size now, it has changed
    /// and must be skipped.
    pub bytes: u64,
    /// Modified time at scan time, Unix seconds. Same rule as `bytes`.
    pub modified: Option<i64>,
}

const MAX_FAILURES: usize = 50;

/// Remove files, to the Recycle Bin when `to_recycle_bin` is true, otherwise
/// permanently. Never fails as a whole: each problem is recorded in the
/// report. Skips anything protected, missing, changed since the scan, or that
/// is not a plain file.
pub fn remove_files(targets: &[RemoveTarget], to_recycle_bin: bool, ctx: &Ctx) -> RemoveReport {
    let mut report = RemoveReport {
        removed: 0,
        failed: 0,
        freed_bytes: 0,
        failures: Vec::new(),
        to_recycle_bin,
        removed_ids: Vec::new(),
    };
    let protector = Protector::from_env();
    let total = targets.len().max(1) as f32;
    for (i, t) in targets.iter().enumerate() {
        if ctx.cancelled() {
            break;
        }
        let name = t.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        ctx.step(format!("Removing {name}"), i as f32 / total);
        match remove_one(t, to_recycle_bin, &protector) {
            Ok(()) => {
                report.removed += 1;
                report.freed_bytes += t.bytes;
                report.removed_ids.push(t.id);
            }
            Err(reason) => {
                report.failed += 1;
                if report.failures.len() < MAX_FAILURES {
                    report
                        .failures
                        .push(RemoveFailure { path: t.path.to_string_lossy().into_owned(), reason });
                }
            }
        }
    }
    ctx.step("Done", 1.0);
    report
}

fn remove_one(t: &RemoveTarget, to_recycle_bin: bool, protector: &Protector) -> Result<(), String> {
    const PROTECTED: &str = "Protected system location";
    const NOT_PLAIN: &str = "Not a plain file";
    const CHANGED: &str = "Changed since the scan";

    if protector.is_protected(&t.path) {
        return Err(PROTECTED.into());
    }
    let meta = std::fs::symlink_metadata(&t.path).map_err(|e| io_reason(&e))?;
    let attrs = std::os::windows::fs::MetadataExt::file_attributes(&meta);
    if meta.is_dir() || is_dir_attr(attrs) || is_reparse(attrs) {
        return Err(NOT_PLAIN.into());
    }
    if meta.len() != t.bytes || crate::util::modified_unix(&meta) != t.modified {
        return Err(CHANGED.into());
    }

    if to_recycle_bin {
        match trash::delete(&t.path) {
            Ok(()) => Ok(()),
            // The shell can report a problem after the file has gone, so
            // the receipt goes by what is really on disk.
            Err(e) if std::fs::symlink_metadata(&t.path).is_err() => {
                let _ = e;
                Ok(())
            }
            Err(e) => Err(format!("Could not move it to the Recycle Bin ({e})")),
        }
    } else {
        delete_checked(&t.path, |info| {
            if is_dir_attr(info.attrs) || is_reparse(info.attrs) {
                return Err(NOT_PLAIN);
            }
            if protector.is_protected(&info.final_path) {
                return Err(PROTECTED);
            }
            if info.bytes != t.bytes || info.modified != t.modified {
                return Err(CHANGED);
            }
            Ok(())
        })
        .map(|_| ())
        .map_err(|e| e.reason())
    }
}

#[cfg(test)]
pub(crate) mod test_util {
    use std::fs::{File, FileTimes, OpenOptions};
    use std::os::windows::fs::{FileTimesExt, OpenOptionsExt};
    use std::path::Path;
    use std::time::{Duration, SystemTime};

    /// Make a junction with `mklink /J`. False if this machine will not.
    pub fn make_junction(link: &Path, target: &Path) -> bool {
        std::process::Command::new("cmd")
            .arg("/C")
            .arg("mklink")
            .arg("/J")
            .arg(link)
            .arg(target)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
            && link.exists()
    }

    /// Set both created and modified times to `days` days ago. Works on
    /// files and folders.
    pub fn age(path: &Path, days: u64) {
        let when = SystemTime::now() - Duration::from_secs(days * 86_400);
        let times = FileTimes::new().set_modified(when).set_created(when);
        let f = OpenOptions::new()
            .access_mode(0x100) // FILE_WRITE_ATTRIBUTES
            .custom_flags(0x0200_0000) // FILE_FLAG_BACKUP_SEMANTICS, for folders
            .open(path)
            .expect("open for times");
        f.set_times(times).expect("set times");
    }

    pub fn write(path: &Path, bytes: usize) {
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p).expect("mkdir");
        }
        std::fs::write(path, vec![7u8; bytes]).expect("write");
    }

    pub fn keep_open_exclusive(path: &Path) -> File {
        OpenOptions::new().read(true).share_mode(0).open(path).expect("open exclusive")
    }
}

#[cfg(test)]
mod tests {
    use super::test_util::*;
    use super::*;

    fn target(id: u32, path: &Path) -> RemoveTarget {
        let meta = std::fs::metadata(path).expect("meta");
        RemoveTarget {
            id,
            path: path.to_path_buf(),
            bytes: meta.len(),
            modified: crate::util::modified_unix(&meta),
        }
    }

    #[test]
    fn protects_system_locations() {
        let win = expand_env("%SystemRoot%").expect("SystemRoot");
        assert!(is_protected(&win));
        assert!(is_protected(&win.join(r"System32\kernel32.dll")));
        let upper = PathBuf::from(win.to_string_lossy().to_uppercase()).join("x.dll");
        assert!(is_protected(&upper));
        if let Some(pf) = expand_env("%ProgramFiles%") {
            assert!(is_protected(&pf.join(r"App\app.exe")));
        }
        assert!(is_protected(Path::new(r"C:\")));
        assert!(is_protected(Path::new(r"D:\pagefile.sys")));
        assert!(is_protected(Path::new(r"C:\HIBERFIL.SYS")));
        assert!(is_protected(Path::new(r"E:\$Recycle.Bin\S-1-5\file.txt")));
        assert!(is_protected(Path::new(r"C:\System Volume Information\x")));
        assert!(is_protected(Path::new(r"C:\Users\x\..\..\Windows\notepad.exe")));
        assert!(is_protected(Path::new(r"relative\file.txt")));
        assert!(!is_protected(Path::new(r"C:\Users\someone\Downloads\film.mkv")));
        assert!(!is_protected(Path::new(r"D:\stuff\pagefile.sys")));
    }

    #[test]
    fn removes_unchanged_and_skips_changed_missing_and_links() {
        let dir = tempfile::tempdir().expect("tempdir");
        let keep_same = dir.path().join("same.bin");
        let changed = dir.path().join("changed.bin");
        let missing = dir.path().join("missing.bin");
        write(&keep_same, 100);
        write(&changed, 100);
        write(&missing, 100);
        let targets = vec![target(1, &keep_same), target(2, &changed), target(3, &missing)];
        write(&changed, 150);
        std::fs::remove_file(&missing).expect("rm");

        let report = remove_files(&targets, false, &Ctx::none());
        assert_eq!(report.removed, 1);
        assert_eq!(report.removed_ids, vec![1]);
        assert_eq!(report.freed_bytes, 100);
        assert_eq!(report.failed, 2);
        assert!(!keep_same.exists());
        assert!(changed.exists());
        assert!(report.failures.iter().any(|f| f.reason == "Changed since the scan"));

        // A junction or a folder is never removed this way.
        let sub = dir.path().join("sub");
        std::fs::create_dir(&sub).expect("mkdir");
        let outside = tempfile::tempdir().expect("tempdir");
        write(&outside.path().join("precious.txt"), 10);
        let link = dir.path().join("link");
        let mut more = vec![RemoveTarget { id: 9, path: sub.clone(), bytes: 0, modified: None }];
        if make_junction(&link, outside.path()) {
            more.push(RemoveTarget { id: 10, path: link.clone(), bytes: 0, modified: None });
        }
        let report = remove_files(&more, false, &Ctx::none());
        assert_eq!(report.removed, 0);
        assert!(report.failures.iter().all(|f| f.reason == "Not a plain file"));
        assert!(sub.exists());
        assert!(outside.path().join("precious.txt").exists());
    }

    #[test]
    fn protected_paths_are_refused() {
        let win = expand_env("%SystemRoot%").expect("SystemRoot");
        let t = RemoveTarget { id: 1, path: win.join("win.ini"), bytes: 0, modified: None };
        let report = remove_files(&[t], false, &Ctx::none());
        assert_eq!(report.removed, 0);
        assert_eq!(report.failures[0].reason, "Protected system location");
    }

    #[test]
    fn locked_files_are_reported_not_fatal() {
        let dir = tempfile::tempdir().expect("tempdir");
        let f = dir.path().join("locked.bin");
        write(&f, 10);
        let t = target(1, &f);
        let guard = keep_open_exclusive(&f);
        let report = remove_files(&[t], false, &Ctx::none());
        drop(guard);
        assert_eq!(report.removed, 0);
        assert_eq!(report.failed, 1);
        assert!(f.exists());
    }

    #[test]
    fn delete_checked_removes_a_junction_not_its_target() {
        let dir = tempfile::tempdir().expect("tempdir");
        let outside = tempfile::tempdir().expect("tempdir");
        write(&outside.path().join("precious.txt"), 10);
        let link = dir.path().join("link");
        if !make_junction(&link, outside.path()) {
            eprintln!("skipped: could not create a junction");
            return;
        }
        let info = delete_checked(&link, |_| Ok(())).expect("delete link");
        assert!(is_reparse(info.attrs));
        assert!(std::fs::symlink_metadata(&link).is_err());
        assert!(outside.path().join("precious.txt").exists());
    }
}
