//! Large file finder.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use crate::fsops::walk::{in_pool, par_walk_files, Tally};
use crate::fsops::{is_cloud_placeholder, is_reparse, Protector, RemoveTarget};
use crate::model::{FoundFile, LargeScan};
use crate::util::{file_kind, modified_unix};
use crate::{Ctx, Error, Result};

/// The most files a scan returns. `LargeScan::total_count` still counts all.
pub const MAX_RESULTS: usize = 2000;

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct Candidate {
    bytes: u64,
    path: PathBuf,
    modified: Option<i64>,
}

/// Fill in the display fields shared by large files and duplicates.
pub(crate) fn found_file(id: u32, path: &Path, bytes: u64, modified: Option<i64>, protector: &Protector) -> FoundFile {
    FoundFile {
        id,
        path: path.to_string_lossy().into_owned(),
        name: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        dir: path.parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default(),
        bytes,
        modified,
        kind: file_kind(path).to_string(),
        protected: protector.is_protected(path),
    }
}

/// Find files of at least `min_bytes` under `root`, biggest first.
pub fn scan(root: &Path, min_bytes: u64, ctx: &Ctx) -> Result<LargeScan> {
    scan_capped(root, min_bytes, MAX_RESULTS, ctx)
}

fn scan_capped(root: &Path, min_bytes: u64, cap: usize, ctx: &Ctx) -> Result<LargeScan> {
    let started = Instant::now();
    let count = AtomicU64::new(0);
    let total = AtomicU64::new(0);
    // Only the biggest `cap` are kept, in a min-heap. Once it is full, `floor`
    // lets most files skip the lock entirely.
    let heap: Mutex<BinaryHeap<Reverse<Candidate>>> = Mutex::new(BinaryHeap::with_capacity(cap + 1));
    let floor = AtomicU64::new(0);
    let full = AtomicBool::new(false);

    let tally = Tally::new(ctx, format!("Scanning {}", root.display()));
    let visit = |path: &Path, meta: &std::fs::Metadata| {
        let attrs = meta.file_attributes();
        // Links cannot be removed safely, and cloud placeholders use no space.
        if is_reparse(attrs) || is_cloud_placeholder(attrs) {
            return;
        }
        let bytes = meta.len();
        if bytes < min_bytes {
            return;
        }
        count.fetch_add(1, Ordering::Relaxed);
        total.fetch_add(bytes, Ordering::Relaxed);
        if cap == 0 || (full.load(Ordering::Relaxed) && bytes <= floor.load(Ordering::Relaxed)) {
            return;
        }
        let mut h = heap.lock().unwrap_or_else(|e| e.into_inner());
        h.push(Reverse(Candidate { bytes, path: path.to_path_buf(), modified: modified_unix(meta) }));
        if h.len() > cap {
            h.pop();
        }
        if h.len() == cap {
            if let Some(Reverse(smallest)) = h.peek() {
                floor.store(smallest.bytes, Ordering::Relaxed);
                full.store(true, Ordering::Relaxed);
            }
        }
    };
    in_pool(|| par_walk_files(root, &tally, &visit));
    if ctx.cancelled() {
        return Err(Error::Cancelled);
    }
    tally.finish();

    let protector = Protector::from_env();
    let mut kept: Vec<Candidate> =
        heap.into_inner().unwrap_or_else(|e| e.into_inner()).into_iter().map(|r| r.0).collect();
    kept.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.path.cmp(&b.path)));
    let files: Vec<FoundFile> = kept
        .iter()
        .enumerate()
        .map(|(i, c)| found_file(i as u32, &c.path, c.bytes, c.modified, &protector))
        .collect();
    let total_count = count.load(Ordering::Relaxed);
    Ok(LargeScan {
        root: root.to_string_lossy().into_owned(),
        min_bytes,
        truncated: total_count > files.len() as u64,
        files,
        total_count,
        total_bytes: total.load(Ordering::Relaxed),
        elapsed_ms: started.elapsed().as_millis() as u64,
    })
}

/// Turn ids the UI sent back into removal targets, using the scan the app
/// kept. Unknown ids are an `Error::NotFound`. Protected files are an
/// `Error::Refused`.
pub fn removal_targets(scan: &LargeScan, ids: &[u32]) -> Result<Vec<RemoveTarget>> {
    let by_id: HashMap<u32, &FoundFile> = scan.files.iter().map(|f| (f.id, f)).collect();
    let protector = Protector::from_env();
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for id in ids {
        let f = by_id.get(id).ok_or_else(|| Error::not_found("That file is no longer in the results."))?;
        if f.protected || protector.is_protected(Path::new(&f.path)) {
            return Err(Error::refused(format!("{} is in a protected system location", f.name)));
        }
        if seen.insert(*id) {
            out.push(RemoveTarget { id: f.id, path: PathBuf::from(&f.path), bytes: f.bytes, modified: f.modified });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fsops::test_util::{make_junction, write};
    use crate::fsops::remove_files;

    #[test]
    fn min_size_cap_and_order() {
        let dir = tempfile::tempdir().expect("tempdir");
        for (i, size) in [10usize, 2000, 3000, 4000, 5000, 999].iter().enumerate() {
            write(&dir.path().join(format!(r"d{}\f{i}.bin", i % 2)), *size);
        }
        let s = scan_capped(dir.path(), 1000, 3, &Ctx::none()).expect("scan");
        assert_eq!(s.total_count, 4);
        assert_eq!(s.total_bytes, 14_000);
        assert!(s.truncated);
        assert_eq!(s.files.iter().map(|f| f.bytes).collect::<Vec<_>>(), vec![5000, 4000, 3000]);
        let ids: HashSet<_> = s.files.iter().map(|f| f.id).collect();
        assert_eq!(ids.len(), 3);
        assert!(s.files.iter().all(|f| !f.protected && f.name.ends_with(".bin") && !f.dir.is_empty()));

        let all = scan(dir.path(), 1000, &Ctx::none()).expect("scan");
        assert!(!all.truncated);
        assert_eq!(all.files.len(), 4);
    }

    #[test]
    fn junctions_are_not_followed() {
        let dir = tempfile::tempdir().expect("tempdir");
        let outside = tempfile::tempdir().expect("tempdir");
        write(&outside.path().join("huge.bin"), 50_000);
        write(&dir.path().join("mine.bin"), 5_000);
        if !make_junction(&dir.path().join("link"), outside.path()) {
            eprintln!("skipped: could not create a junction");
            return;
        }
        let s = scan(dir.path(), 1, &Ctx::none()).expect("scan");
        assert_eq!(s.total_count, 1);
        assert!(s.files[0].path.ends_with("mine.bin"));
    }

    #[test]
    fn removal_targets_check_ids_and_protection() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(&dir.path().join("a.bin"), 2_000);
        let mut s = scan(dir.path(), 1, &Ctx::none()).expect("scan");
        let id = s.files[0].id;
        let targets = removal_targets(&s, &[id, id]).expect("targets");
        assert_eq!(targets.len(), 1);
        assert!(matches!(removal_targets(&s, &[999]), Err(Error::NotFound(_))));

        // Changed since the scan: skipped by remove_files.
        write(&dir.path().join("a.bin"), 3_000);
        let report = remove_files(&targets, false, &Ctx::none());
        assert_eq!(report.removed, 0);
        assert_eq!(report.failures[0].reason, "Changed since the scan");
        assert!(dir.path().join("a.bin").exists());

        let win = crate::util::expand_env("%SystemRoot%").expect("SystemRoot");
        s.files.push(found_file(77, &win.join("explorer.exe"), 1, None, &Protector::from_env()));
        assert!(s.files.last().is_some_and(|f| f.protected));
        assert!(matches!(removal_targets(&s, &[77]), Err(Error::Refused(_))));
    }
}
