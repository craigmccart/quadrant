//! Small shared helpers. This file is shared by every module, so keep it
//! stable: put module-specific helpers in the module that needs them.

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

/// `CREATE_NO_WINDOW`: stops a console window flashing up when we run a
/// command-line tool such as `schtasks` or `netstat`.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Now, as Unix seconds.
pub fn now_unix() -> i64 {
    to_unix(SystemTime::now())
}

/// A `SystemTime` as Unix seconds. Times before 1970 come back as 0.
pub fn to_unix(t: SystemTime) -> i64 {
    t.duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// Last-modified time of a file as Unix seconds, if Windows will tell us.
pub fn modified_unix(meta: &std::fs::Metadata) -> Option<i64> {
    meta.modified().ok().map(to_unix)
}

/// Expand `%NAME%` environment variables in a path, e.g.
/// `%LOCALAPPDATA%\Temp`. Returns None if any variable is missing or empty,
/// so a broken environment can never turn into a surprising path.
pub fn expand_env(template: &str) -> Option<PathBuf> {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let end = after.find('%')?;
        let name = &after[..end];
        let value = std::env::var(name).ok()?;
        if value.trim().is_empty() {
            return None;
        }
        out.push_str(&value);
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    Some(PathBuf::from(out))
}

/// A `Command` that will not flash a console window.
pub fn hidden_command(program: &str) -> Command {
    let mut cmd = Command::new(program);
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

/// Encode a string as a NUL-terminated UTF-16 buffer for Win32 calls.
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Where Quadrant keeps its own small data files (history and the like):
/// `%LOCALAPPDATA%\Quadrant`. Created on demand.
pub fn data_dir() -> Option<PathBuf> {
    let dir = expand_env(r"%LOCALAPPDATA%\Quadrant")?;
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

/// A byte count as people read it: "512 B", "4.2 GB". Binary units, to match
/// what File Explorer shows.
pub fn fmt_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// A duration as people say it: "3 days 4 hours", "25 minutes".
pub fn fmt_duration(secs: u64) -> String {
    let days = secs / 86_400;
    let hours = (secs % 86_400) / 3_600;
    let mins = (secs % 3_600) / 60;
    let plural = |n: u64, word: &str| format!("{n} {word}{}", if n == 1 { "" } else { "s" });
    if days > 0 {
        format!("{} {}", plural(days, "day"), plural(hours, "hour"))
    } else if hours > 0 {
        format!("{} {}", plural(hours, "hour"), plural(mins, "minute"))
    } else {
        plural(mins.max(1), "minute")
    }
}

/// Sort a file into a broad kind by its extension. The vocabulary here is
/// what the UI shows, so keep it in step with `model::TypeUsage`.
pub fn file_kind(path: &Path) -> &'static str {
    let ext = match path.extension().and_then(|e| e.to_str()) {
        Some(e) => e.to_ascii_lowercase(),
        None => return "Other",
    };
    match ext.as_str() {
        "mp4" | "mkv" | "mov" | "avi" | "wmv" | "flv" | "m4v" | "mpg" | "mpeg" | "webm" => "Video",
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "tif" | "tiff" | "heic" | "heif" | "raw" | "cr2"
        | "nef" | "arw" | "dng" | "webp" | "svg" | "psd" | "ico" => "Images",
        "mp3" | "wav" | "flac" | "aac" | "ogg" | "wma" | "m4a" | "opus" => "Audio",
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "odt" | "ods" | "odp" | "rtf"
        | "txt" | "md" | "csv" | "epub" | "one" | "pst" | "ost" => "Documents",
        "zip" | "7z" | "rar" | "tar" | "gz" | "bz2" | "xz" | "zst" | "cab" | "iso" | "img"
        | "vhd" | "vhdx" | "vmdk" | "wim" | "esd" => "Archives",
        "exe" | "msi" | "msix" | "appx" | "dll" | "sys" | "ocx" | "drv" | "msp" | "msu" => "Apps",
        "py" | "js" | "mjs" | "ts" | "tsx" | "jsx" | "cs" | "cpp" | "c" | "h" | "hpp" | "java"
        | "go" | "rs" | "rb" | "php" | "swift" | "kt" | "json" | "xml" | "yml" | "yaml" | "toml"
        | "html" | "css" | "sql" | "sh" | "ps1" | "bat" | "pdb" | "obj" | "lib" | "rlib" | "o" => {
            "Code"
        }
        "log" | "etl" | "dmp" | "tmp" | "bak" | "old" | "db" | "sqlite" | "dat" | "bin" => "System",
        _ => "Other",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_known_variables() {
        std::env::set_var("QUADRANT_TEST_VAR", r"C:\Somewhere");
        assert_eq!(
            expand_env(r"%QUADRANT_TEST_VAR%\Temp"),
            Some(PathBuf::from(r"C:\Somewhere\Temp"))
        );
    }

    #[test]
    fn refuses_missing_variables() {
        assert_eq!(expand_env(r"%QUADRANT_DEFINITELY_NOT_SET%\Temp"), None);
        assert_eq!(expand_env(r"%UNTERMINATED\Temp"), None);
    }

    #[test]
    fn formats_sizes_and_durations() {
        assert_eq!(fmt_bytes(512), "512 B");
        assert_eq!(fmt_bytes(1536), "1.5 KB");
        assert_eq!(fmt_bytes(5 * 1024 * 1024 * 1024), "5.0 GB");
        assert_eq!(fmt_duration(90_000), "1 day 1 hour");
        assert_eq!(fmt_duration(30), "1 minute");
    }

    #[test]
    fn sorts_files_into_kinds() {
        assert_eq!(file_kind(Path::new(r"C:\a\film.MKV")), "Video");
        assert_eq!(file_kind(Path::new(r"C:\a\setup.exe")), "Apps");
        assert_eq!(file_kind(Path::new(r"C:\a\noext")), "Other");
    }
}
