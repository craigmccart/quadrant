//! A small log of what Quadrant has removed, kept in
//! `%LOCALAPPDATA%\Quadrant\history.json`. It holds counts and sizes only,
//! never file names.

use std::fs;
use std::path::PathBuf;

use crate::model::{CleanItemResult, CleanReport, History, HistoryEntry, RemoveReport};
use crate::util;

/// Oldest entries are dropped beyond this many.
const MAX_ENTRIES: usize = 200;

fn history_file() -> Option<PathBuf> {
    Some(util::data_dir()?.join("history.json"))
}

fn read_entries(path: &PathBuf) -> Vec<HistoryEntry> {
    // A missing or damaged file just means "no history yet".
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn summarise(mut entries: Vec<HistoryEntry>) -> History {
    entries.sort_by(|a, b| b.at.cmp(&a.at));
    History {
        total_freed_bytes: entries.iter().map(|e| e.freed_bytes).sum(),
        total_removed: entries.iter().map(|e| e.removed).sum(),
        entries,
    }
}

/// Everything recorded so far, newest first.
pub fn load() -> History {
    summarise(history_file().map(|p| read_entries(&p)).unwrap_or_default())
}

/// Add one entry. Failing to write history is never worth interrupting the
/// user for, so problems are swallowed.
pub fn append(entry: HistoryEntry) {
    let Some(path) = history_file() else { return };
    let mut entries = read_entries(&path);
    entries.push(entry);
    entries.sort_by(|a, b| b.at.cmp(&a.at));
    entries.truncate(MAX_ENTRIES);
    let Ok(json) = serde_json::to_string_pretty(&entries) else { return };
    // Write to a sibling file first so a crash cannot leave half a file.
    let tmp = path.with_extension("json.tmp");
    if fs::write(&tmp, json).is_ok() {
        let _ = fs::rename(&tmp, &path);
    }
}

/// Record a junk or privacy clean. Cleans that removed nothing are not kept.
pub fn record_clean(kind: &str, report: &CleanReport) {
    if report.removed == 0 {
        return;
    }
    append(HistoryEntry {
        at: report.finished_at,
        kind: kind.to_string(),
        freed_bytes: report.freed_bytes,
        removed: report.removed,
        skipped: report.skipped,
        items: report.items.iter().filter(|i| i.removed > 0).cloned().collect(),
    });
}

/// Record files removed from Large Files or Duplicates.
pub fn record_removal(kind: &str, label: &str, report: &RemoveReport) {
    if report.removed == 0 {
        return;
    }
    append(HistoryEntry {
        at: util::now_unix(),
        kind: kind.to_string(),
        freed_bytes: report.freed_bytes,
        removed: report.removed,
        skipped: report.failed,
        items: vec![CleanItemResult {
            id: kind.to_string(),
            label: label.to_string(),
            freed_bytes: report.freed_bytes,
            removed: report.removed,
            skipped: report.failed,
        }],
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(at: i64, freed: u64) -> HistoryEntry {
        HistoryEntry {
            at,
            kind: "junk".into(),
            freed_bytes: freed,
            removed: 1,
            skipped: 0,
            items: Vec::new(),
        }
    }

    #[test]
    fn newest_first_with_totals() {
        let h = summarise(vec![entry(10, 100), entry(30, 300), entry(20, 200)]);
        assert_eq!(h.entries.iter().map(|e| e.at).collect::<Vec<_>>(), vec![30, 20, 10]);
        assert_eq!(h.total_freed_bytes, 600);
        assert_eq!(h.total_removed, 3);
    }

    #[test]
    fn damaged_file_reads_as_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.json");
        fs::write(&path, "{ not json").unwrap();
        assert!(read_entries(&path).is_empty());
    }
}
