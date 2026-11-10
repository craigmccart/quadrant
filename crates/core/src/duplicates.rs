//! Duplicate file finder.
//!
//! Files are grouped by size, then by a hash of their first 64 KB, then by a
//! hash of the whole file. Only whole-file matches count as duplicates.

use std::collections::{HashMap, HashSet};
use std::fs::{File, OpenOptions};
use std::io::Read;
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Instant;

use rayon::prelude::*;

use crate::fsops::walk::{in_pool, par_walk_files, Tally};
use crate::fsops::{is_cloud_placeholder, is_dir_attr, is_reparse, Protector, RemoveTarget};
use crate::largefiles::found_file;
use crate::model::{DupGroup, DupScan, Progress};
use crate::util::modified_unix;
use crate::{Ctx, Error, Result};

/// The most groups a scan returns. `DupScan::total_groups` still counts all.
pub const MAX_GROUPS: usize = 2000;

const PARTIAL: u64 = 64 * 1024;
const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
const FILE_FLAG_SEQUENTIAL_SCAN: u32 = 0x0800_0000;

struct Cand {
    path: PathBuf,
    bytes: u64,
    modified: Option<i64>,
}

/// Progress for the hashing stages: bytes hashed out of bytes to hash.
struct HashProgress<'a, 'c> {
    ctx: &'a Ctx<'c>,
    done: AtomicU64,
    total: AtomicU64,
    last: AtomicU64,
}

impl HashProgress<'_, '_> {
    fn add(&self, n: u64) {
        let done = self.done.fetch_add(n, Ordering::Relaxed) + n;
        // Report about every 32 MB so the shell is not flooded.
        let bucket = done >> 25;
        if self.last.swap(bucket, Ordering::Relaxed) != bucket {
            let total = self.total.load(Ordering::Relaxed).max(1);
            self.ctx.report(Progress {
                message: "Comparing file contents".to_string(),
                fraction: Some((done as f64 / total as f64).min(1.0) as f32),
                items: 0,
                bytes: done,
            });
        }
    }
}

/// Open for reading without following a link, and confirm through the
/// handle that this is still a plain, local file of the expected size.
fn open_plain(path: &Path, bytes: u64) -> Option<File> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_SEQUENTIAL_SCAN)
        .open(path)
        .ok()?;
    let meta = file.metadata().ok()?;
    let attrs = meta.file_attributes();
    if is_dir_attr(attrs) || is_reparse(attrs) || is_cloud_placeholder(attrs) || meta.len() != bytes {
        return None;
    }
    Some(file)
}

/// BLAKE3 of the first `limit` bytes (or all of it). None if the file
/// cannot be read, has changed size, or the scan is cancelled.
fn hash_file(c: &Cand, limit: u64, progress: &HashProgress) -> Option<blake3::Hash> {
    let mut file = open_plain(&c.path, c.bytes)?;
    let want = c.bytes.min(limit);
    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0u8; 256 * 1024];
    let mut read_total = 0u64;
    while read_total < want {
        if progress.ctx.cancelled() {
            return None;
        }
        let chunk = (want - read_total).min(buf.len() as u64) as usize;
        let n = file.read(&mut buf[..chunk]).ok()?;
        if n == 0 {
            return None;
        }
        hasher.update(&buf[..n]);
        read_total += n as u64;
        progress.add(n as u64);
    }
    Some(hasher.finalize())
}

/// Find files under `root` of at least `min_bytes` that have identical
/// content. Two files only count as duplicates when their full contents hash
/// the same.
pub fn scan(root: &Path, min_bytes: u64, ctx: &Ctx) -> Result<DupScan> {
    scan_capped(root, min_bytes, MAX_GROUPS, ctx)
}

fn scan_capped(root: &Path, min_bytes: u64, cap: usize, ctx: &Ctx) -> Result<DupScan> {
    let started = Instant::now();
    // Empty files are all "identical" and never worth grouping.
    let min = min_bytes.max(1);

    let found: Mutex<Vec<Cand>> = Mutex::new(Vec::new());
    let tally = Tally::new(ctx, format!("Scanning {}", root.display()));
    let visit = |path: &Path, meta: &std::fs::Metadata| {
        let attrs = meta.file_attributes();
        // Never read a cloud placeholder: it would start a download.
        if is_reparse(attrs) || is_cloud_placeholder(attrs) || meta.len() < min {
            return;
        }
        let c = Cand { path: path.to_path_buf(), bytes: meta.len(), modified: modified_unix(meta) };
        found.lock().unwrap_or_else(|e| e.into_inner()).push(c);
    };
    in_pool(|| par_walk_files(root, &tally, &visit));
    if ctx.cancelled() {
        return Err(Error::Cancelled);
    }
    tally.finish();
    let cands = found.into_inner().unwrap_or_else(|e| e.into_inner());
    let files_checked = cands.len() as u64;

    // Stage 1: same size.
    let mut by_size: HashMap<u64, Vec<usize>> = HashMap::new();
    for (i, c) in cands.iter().enumerate() {
        by_size.entry(c.bytes).or_default().push(i);
    }
    let sized: Vec<usize> = by_size.into_values().filter(|v| v.len() > 1).flatten().collect();

    // The full-hash total is an upper bound until stage 2 narrows it, so the
    // bar can only jump forwards.
    let partial_total: u64 = sized.iter().map(|&i| cands[i].bytes.min(PARTIAL)).sum();
    let full_upper: u64 = sized.iter().map(|&i| cands[i].bytes).filter(|&b| b > PARTIAL).sum();
    let progress = HashProgress {
        ctx,
        done: AtomicU64::new(0),
        total: AtomicU64::new(partial_total + full_upper),
        last: AtomicU64::new(u64::MAX),
    };

    // Stage 2: same size and same first 64 KB. For small files this already
    // covers the whole file.
    let partial: Vec<(usize, blake3::Hash)> = in_pool(|| {
        sized
            .par_iter()
            .filter_map(|&i| hash_file(&cands[i], PARTIAL, &progress).map(|h| (i, h)))
            .collect()
    });
    if ctx.cancelled() {
        return Err(Error::Cancelled);
    }
    let mut by_partial: HashMap<(u64, blake3::Hash), Vec<usize>> = HashMap::new();
    for (i, h) in partial {
        by_partial.entry((cands[i].bytes, h)).or_default().push(i);
    }
    let mut groups_in: Vec<(u64, blake3::Hash, Vec<usize>)> = Vec::new();
    let mut needs_full: Vec<usize> = Vec::new();
    for ((bytes, h), members) in by_partial {
        if members.len() < 2 {
            continue;
        }
        if bytes <= PARTIAL {
            groups_in.push((bytes, h, members));
        } else {
            needs_full.extend(members);
        }
    }
    let full_total: u64 = needs_full.iter().map(|&i| cands[i].bytes).sum();
    progress.total.store(partial_total + full_total, Ordering::Relaxed);

    // Stage 3: the whole file.
    let full: Vec<(usize, blake3::Hash)> = in_pool(|| {
        needs_full
            .par_iter()
            .filter_map(|&i| hash_file(&cands[i], u64::MAX, &progress).map(|h| (i, h)))
            .collect()
    });
    if ctx.cancelled() {
        return Err(Error::Cancelled);
    }
    let mut by_full: HashMap<(u64, blake3::Hash), Vec<usize>> = HashMap::new();
    for (i, h) in full {
        by_full.entry((cands[i].bytes, h)).or_default().push(i);
    }
    groups_in.extend(by_full.into_iter().filter(|(_, m)| m.len() > 1).map(|((b, h), m)| (b, h, m)));

    let wasted = |g: &(u64, blake3::Hash, Vec<usize>)| g.0 * (g.2.len() as u64 - 1);
    let total_groups = groups_in.len() as u64;
    let wasted_bytes: u64 = groups_in.iter().map(wasted).sum();
    groups_in.sort_by(|a, b| wasted(b).cmp(&wasted(a)).then_with(|| cands[a.2[0]].path.cmp(&cands[b.2[0]].path)));
    groups_in.truncate(cap);

    let protector = Protector::from_env();
    let mut next_id = 0u32;
    let groups: Vec<DupGroup> = groups_in
        .into_iter()
        .enumerate()
        .map(|(gi, (bytes, hash, mut members))| {
            // Oldest first; files with no known time go last.
            members.sort_by_key(|&i| (cands[i].modified.is_none(), cands[i].modified, cands[i].path.clone()));
            let files: Vec<_> = members
                .iter()
                .map(|&i| {
                    let c = &cands[i];
                    let f = found_file(next_id, &c.path, c.bytes, c.modified, &protector);
                    next_id += 1;
                    f
                })
                .collect();
            DupGroup {
                id: gi as u32,
                bytes,
                hash: hash.to_hex().as_str()[..16].to_string(),
                wasted_bytes: bytes * (files.len() as u64 - 1),
                files,
            }
        })
        .collect();
    ctx.step("Done", 1.0);

    Ok(DupScan {
        root: root.to_string_lossy().into_owned(),
        min_bytes,
        truncated: total_groups > groups.len() as u64,
        groups,
        total_groups,
        wasted_bytes,
        files_checked,
        elapsed_ms: started.elapsed().as_millis() as u64,
    })
}

/// Turn file ids the UI sent back into removal targets, using the scan the
/// app kept.
///
/// Refuses with `Error::Refused` if the ids would remove every copy in any
/// group: at least one copy of each file always survives. Unknown ids are an
/// `Error::NotFound`.
pub fn removal_targets(scan: &DupScan, ids: &[u32]) -> Result<Vec<RemoveTarget>> {
    let wanted: HashSet<u32> = ids.iter().copied().collect();
    let mut by_id = HashMap::new();
    for g in &scan.groups {
        for f in &g.files {
            by_id.insert(f.id, f);
        }
    }
    if let Some(missing) = wanted.iter().find(|id| !by_id.contains_key(id)) {
        return Err(Error::not_found(format!("File {missing} is no longer in the results.")));
    }
    for g in &scan.groups {
        if !g.files.is_empty() && g.files.iter().all(|f| wanted.contains(&f.id)) {
            return Err(Error::refused(format!(
                "That would remove every copy of {}. At least one copy is always kept.",
                g.files[0].name
            )));
        }
    }
    let protector = Protector::from_env();
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for id in ids {
        if !seen.insert(*id) {
            continue;
        }
        let Some(f) = by_id.get(id) else { continue };
        if f.protected || protector.is_protected(Path::new(&f.path)) {
            return Err(Error::refused(format!("{} is in a protected system location", f.name)));
        }
        out.push(RemoveTarget { id: f.id, path: PathBuf::from(&f.path), bytes: f.bytes, modified: f.modified });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fsops::test_util::make_junction;

    fn put(path: &Path, data: &[u8]) {
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p).expect("mkdir");
        }
        std::fs::write(path, data).expect("write");
    }

    #[test]
    fn finds_true_duplicates_only() {
        let dir = tempfile::tempdir().expect("tempdir");
        let r = dir.path();
        put(&r.join("a.txt"), b"hello world");
        put(&r.join(r"sub\a copy.txt"), b"hello world");
        put(&r.join("b.txt"), b"hello wurld"); // same size, different content
        put(&r.join("empty1"), b"");
        put(&r.join("empty2"), b"");
        // Big files sharing the first 64 KB but differing at the end.
        let mut big = vec![1u8; 200_000];
        put(&r.join("big1.bin"), &big);
        put(&r.join("big2.bin"), &big);
        big[199_999] = 2;
        put(&r.join("big3.bin"), &big);

        let s = scan(r, 0, &Ctx::none()).expect("scan");
        assert_eq!(s.total_groups, 2);
        assert_eq!(s.groups[0].bytes, 200_000, "most wasted first");
        assert_eq!(s.groups[0].files.len(), 2);
        assert!(s.groups[0].files.iter().all(|f| !f.name.contains("big3")));
        assert_eq!(s.groups[1].files.len(), 2);
        assert!(s.groups[1].files.iter().all(|f| f.name.contains('a')));
        assert_eq!(s.wasted_bytes, 200_011);
        assert_eq!(s.groups[0].hash.len(), 16);
        let ids: HashSet<_> = s.groups.iter().flat_map(|g| g.files.iter().map(|f| f.id)).collect();
        assert_eq!(ids.len(), 4);

        let capped = scan_capped(r, 0, 1, &Ctx::none()).expect("scan");
        assert_eq!(capped.groups.len(), 1);
        assert!(capped.truncated);
        assert_eq!(capped.total_groups, 2);

        let above = scan(r, 100, &Ctx::none()).expect("scan");
        assert_eq!(above.total_groups, 1);
    }

    #[test]
    fn oldest_first_and_keep_one_rule() {
        let dir = tempfile::tempdir().expect("tempdir");
        put(&dir.path().join("new.txt"), b"same bytes");
        put(&dir.path().join("old.txt"), b"same bytes");
        put(&dir.path().join("mid.txt"), b"same bytes");
        crate::fsops::test_util::age(&dir.path().join("old.txt"), 30);
        crate::fsops::test_util::age(&dir.path().join("mid.txt"), 10);
        let s = scan(dir.path(), 1, &Ctx::none()).expect("scan");
        let g = &s.groups[0];
        let names: Vec<_> = g.files.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, vec!["old.txt", "mid.txt", "new.txt"]);
        assert_eq!(g.wasted_bytes, 20);

        let all: Vec<u32> = g.files.iter().map(|f| f.id).collect();
        assert!(matches!(removal_targets(&s, &all), Err(Error::Refused(_))));
        assert_eq!(removal_targets(&s, &all[1..]).expect("ok").len(), 2);
        assert!(matches!(removal_targets(&s, &[12345]), Err(Error::NotFound(_))));

        let mut protected = s.clone();
        protected.groups[0].files[1].protected = true;
        assert!(matches!(removal_targets(&protected, &[all[1]]), Err(Error::Refused(_))));
    }

    #[test]
    fn junctions_are_not_followed() {
        let dir = tempfile::tempdir().expect("tempdir");
        let outside = tempfile::tempdir().expect("tempdir");
        put(&dir.path().join("mine.txt"), b"duplicate me");
        put(&outside.path().join("theirs.txt"), b"duplicate me");
        if !make_junction(&dir.path().join("link"), outside.path()) {
            eprintln!("skipped: could not create a junction");
            return;
        }
        let s = scan(dir.path(), 1, &Ctx::none()).expect("scan");
        assert_eq!(s.total_groups, 0);
        assert_eq!(s.files_checked, 1);
    }
}
