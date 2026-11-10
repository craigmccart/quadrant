//! Scheduled tasks that did not come with Windows.
//!
//! Read through `schtasks /query /fo CSV /v`. Columns are found by header
//! name, so a change in column order does not break anything.

use std::collections::HashSet;

use crate::model::TaskItem;
use crate::util::hidden_command;
use crate::{Error, Result};

/// Columns we read, by their English header names.
struct Columns {
    name: usize,
    status: Option<usize>,
    next_run: Option<usize>,
    last_run: Option<usize>,
    last_result: Option<usize>,
    author: Option<usize>,
    action: Option<usize>,
    state: Option<usize>,
}

impl Columns {
    fn from_header(header: &csv::StringRecord) -> Option<Self> {
        let find = |want: &str| header.iter().position(|h| h.trim().eq_ignore_ascii_case(want));
        Some(Columns {
            name: find("TaskName")?,
            status: find("Status"),
            next_run: find("Next Run Time"),
            last_run: find("Last Run Time"),
            last_result: find("Last Result"),
            author: find("Author"),
            action: find("Task To Run"),
            state: find("Scheduled Task State"),
        })
    }
}

fn field(rec: &csv::StringRecord, idx: Option<usize>) -> String {
    idx.and_then(|i| rec.get(i)).map(|s| s.trim().to_string()).unwrap_or_default()
}

/// Split `\Folder\Sub\Name` into (`\Folder\Sub`, `Name`). The root is `\`.
fn split_path(path: &str) -> (String, String) {
    match path.rfind('\\') {
        Some(0) | None => (r"\".to_string(), path.trim_start_matches('\\').to_string()),
        Some(i) => (path[..i].to_string(), path[i + 1..].to_string()),
    }
}

/// Turn schtasks' verbose CSV into one row per task, outside `\Microsoft\`,
/// sorted by path.
pub fn parse_csv(text: &str) -> Result<Vec<TaskItem>> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(text.as_bytes());
    let mut cols: Option<Columns> = None;
    let mut seen = HashSet::new();
    let mut out = Vec::new();

    for rec in reader.records() {
        let Ok(rec) = rec else { continue };
        if rec.iter().all(|f| f.trim().is_empty()) {
            continue;
        }
        // The first row is the header, and schtasks repeats it for every
        // folder. Any row that names a TaskName column is a header.
        if rec.iter().any(|f| f.trim().eq_ignore_ascii_case("TaskName")) {
            if cols.is_none() {
                cols = Columns::from_header(&rec);
            }
            continue;
        }
        let Some(c) = cols.as_ref() else { continue };
        let path = field(&rec, Some(c.name));
        if !path.starts_with('\\') {
            continue;
        }
        if path.to_ascii_lowercase().starts_with(r"\microsoft\") {
            continue;
        }
        // Verbose output repeats a task once per trigger: keep the first.
        if !seen.insert(path.to_ascii_lowercase()) {
            continue;
        }
        let state = field(&rec, c.state);
        let status = field(&rec, c.status);
        let enabled = if state.is_empty() {
            !status.eq_ignore_ascii_case("Disabled")
        } else {
            !state.eq_ignore_ascii_case("Disabled")
        };
        let (folder, name) = split_path(&path);
        out.push(TaskItem {
            path,
            name,
            folder,
            status,
            enabled,
            next_run: field(&rec, c.next_run),
            last_run: field(&rec, c.last_run),
            last_result: field(&rec, c.last_result),
            author: field(&rec, c.author),
            action: field(&rec, c.action),
        });
    }
    if cols.is_none() && !text.trim().is_empty() {
        return Err(Error::msg("Could not read the list of scheduled tasks from Windows."));
    }
    out.sort_by_key(|t| t.path.to_lowercase());
    Ok(out)
}

/// Scheduled tasks outside `\Microsoft\`, one row per task, sorted by path.
pub fn list() -> Result<Vec<TaskItem>> {
    let output = hidden_command("schtasks.exe")
        .args(["/query", "/fo", "CSV", "/v"])
        .output()
        .map_err(|_| Error::msg("Could not run the Windows task scheduler tool."))?;
    // The output is in the OEM code page. Lossy UTF-8 keeps the ASCII parts
    // (paths, states) intact, which is what matters here.
    let text = String::from_utf8_lossy(&output.stdout);
    if !output.status.success() && text.trim().is_empty() {
        return Err(Error::msg("Windows would not list the scheduled tasks."));
    }
    parse_csv(&text)
}

/// Map a schtasks failure to an error the user can act on.
fn change_error(stderr: &str) -> Error {
    if stderr.to_ascii_lowercase().contains("access is denied") {
        Error::denied(
            "Changing this task needs administrator rights. Restart Quadrant as administrator and try again.",
        )
    } else {
        Error::msg("Windows could not change that task.")
    }
}

/// Enable or disable a task. `path` must match a task in a fresh `list()`,
/// otherwise `Error::NotFound`. Tasks owned by the system need administrator
/// rights: `Error::Denied` without.
pub fn set_enabled(path: &str, enabled: bool) -> Result<()> {
    let task = list()?
        .into_iter()
        .find(|t| t.path == path)
        .ok_or_else(|| Error::not_found("That task is no longer there. Refresh the list."))?;
    let flag = if enabled { "/ENABLE" } else { "/DISABLE" };
    // Each argument is passed on its own: no shell is involved.
    let output = hidden_command("schtasks.exe")
        .args(["/Change", "/TN", task.path.as_str(), flag])
        .output()
        .map_err(|_| Error::msg("Could not run the Windows task scheduler tool."))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(change_error(&String::from_utf8_lossy(&output.stderr)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = concat!(
        "\"HostName\",\"TaskName\",\"Next Run Time\",\"Status\",\"Logon Mode\",\"Last Run Time\",\"Last Result\",\"Author\",\"Task To Run\",\"Start In\",\"Comment\",\"Scheduled Task State\"\r\n",
        "\"PC\",\"\\Adobe Acrobat Update Task\",\"10/11/2026 18:00:00\",\"Ready\",\"Interactive only\",\"09/11/2026 18:00:00\",\"0\",\"Adobe Systems\",\"C:\\Program Files\\Adobe\\armsvc.exe\",\"N/A\",\"N/A\",\"Enabled\"\r\n",
        "\"PC\",\"\\Adobe Acrobat Update Task\",\"10/11/2026 18:00:00\",\"Ready\",\"Interactive only\",\"09/11/2026 18:00:00\",\"0\",\"Adobe Systems\",\"C:\\Program Files\\Adobe\\armsvc.exe\",\"N/A\",\"N/A\",\"Enabled\"\r\n",
        "\"HostName\",\"TaskName\",\"Next Run Time\",\"Status\",\"Logon Mode\",\"Last Run Time\",\"Last Result\",\"Author\",\"Task To Run\",\"Start In\",\"Comment\",\"Scheduled Task State\"\r\n",
        "\"PC\",\"\\Microsoft\\Windows\\Defrag\\ScheduledDefrag\",\"N/A\",\"Ready\",\"Background\",\"N/A\",\"0\",\"Microsoft\",\"defrag.exe\",\"N/A\",\"N/A\",\"Enabled\"\r\n",
        "\"PC\",\"\\Vendor\\Sub\\Updater, nightly\",\"N/A\",\"Disabled\",\"Background\",\"N/A\",\"267011\",\"Vendor\",\"\"\"C:\\V\\up.exe\"\" /quiet\",\"N/A\",\"N/A\",\"Disabled\"\r\n",
    );

    #[test]
    fn parses_verbose_csv() {
        let tasks = parse_csv(SAMPLE).unwrap();
        assert_eq!(tasks.len(), 2, "header repeat, duplicate and Microsoft task are dropped");

        let a = &tasks[0];
        assert_eq!(a.path, r"\Adobe Acrobat Update Task");
        assert_eq!(a.folder, r"\");
        assert_eq!(a.name, "Adobe Acrobat Update Task");
        assert!(a.enabled);
        assert_eq!(a.status, "Ready");
        assert_eq!(a.author, "Adobe Systems");
        assert_eq!(a.action, r"C:\Program Files\Adobe\armsvc.exe");
        assert_eq!(a.last_result, "0");

        let b = &tasks[1];
        assert_eq!(b.path, r"\Vendor\Sub\Updater, nightly");
        assert_eq!(b.folder, r"\Vendor\Sub");
        assert_eq!(b.name, "Updater, nightly");
        assert!(!b.enabled);
        assert_eq!(b.action, r#""C:\V\up.exe" /quiet"#);
    }

    #[test]
    fn empty_and_garbage_input() {
        assert!(parse_csv("").unwrap().is_empty());
        assert!(parse_csv("ERROR: something odd\r\n").is_err());
    }

    #[test]
    fn splits_paths() {
        assert_eq!(split_path(r"\Task"), (r"\".into(), "Task".into()));
        assert_eq!(split_path(r"\A\B\Task"), (r"\A\B".into(), "Task".into()));
    }

    #[test]
    fn maps_change_errors() {
        assert!(matches!(change_error("ERROR: Access is denied."), Error::Denied(_)));
        assert!(matches!(change_error("ERROR: other"), Error::Message(_)));
    }

    #[test]
    fn real_list_reads() {
        // Read-only: no task is changed.
        let tasks = list().unwrap();
        assert!(tasks.iter().all(|t| !t.path.to_ascii_lowercase().starts_with(r"\microsoft\")));
        eprintln!("TASKS count={}", tasks.len());
    }

    #[test]
    fn unknown_task_is_not_found() {
        assert!(matches!(set_enabled(r"\No Such Task Quadrant xyz", false), Err(Error::NotFound(_))));
    }
}
