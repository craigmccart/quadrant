//! Disk map: what is using the space, folder by folder.
//!
//! A scan walks a whole drive or folder once and keeps the folder tree in
//! memory, so drilling in and out afterwards is instant. Only folders are
//! stored, never one entry per file, so a full system drive stays small.

use std::fs;
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use rayon::prelude::*;

use crate::fsops::walk::{in_pool, Tally};
use crate::fsops::{is_dir_attr, is_reparse, local_bytes};
use crate::model::{DirEntryView, DirListing, DiskScanSummary, Drive, TypeUsage};
use crate::util::{file_kind, now_unix};
use crate::{Ctx, Error, Result};

/// The vocabulary of `util::file_kind`, in a fixed order for the counters.
const KINDS: [&str; 9] =
    ["Video", "Images", "Audio", "Documents", "Archives", "Apps", "Code", "System", "Other"];

fn kind_index(kind: &str) -> usize {
    KINDS.iter().position(|k| *k == kind).unwrap_or(KINDS.len() - 1)
}

const LOOSE_FILES: &str = "Files in this folder";

struct Node {
    name: Box<str>,
    parent: u32,
    /// Totals for this folder and everything below it.
    bytes: u64,
    files: u64,
    /// Files directly in this folder.
    loose_bytes: u64,
    loose_files: u64,
    /// Windows refused access to this folder or something below it.
    denied: bool,
    /// Biggest first.
    children: Box<[u32]>,
}

/// The result of one scan. Holds folders only (with totals), never one entry
/// per file, so it stays small even for a full system drive.
pub struct DiskTree {
    root: String,
    /// Index 0 is the scanned root.
    nodes: Vec<Node>,
    by_type: Vec<TypeUsage>,
    scanned_at: i64,
    elapsed_ms: u64,
}

impl DiskTree {
    /// Totals and the by-type breakdown for the whole scan.
    pub fn summary(&self) -> DiskScanSummary {
        let (bytes, file_count) = self.nodes.first().map(|n| (n.bytes, n.files)).unwrap_or((0, 0));
        DiskScanSummary {
            root: self.root.clone(),
            bytes,
            file_count,
            dir_count: self.nodes.len().saturating_sub(1) as u64,
            by_type: self.by_type.clone(),
            scanned_at: self.scanned_at,
            elapsed_ms: self.elapsed_ms,
        }
    }

    /// The children of `path`, biggest first. `path` must be the scanned root
    /// or a folder inside it (compared case-insensitively). None otherwise.
    pub fn listing(&self, path: &str) -> Option<DirListing> {
        let idx = self.find(path)?;
        let node = &self.nodes[idx];
        let full = self.full_path(idx);
        let percent = |b: u64| if node.bytes == 0 { 0.0 } else { (b as f64 * 100.0 / node.bytes as f64) as f32 };

        let mut entries: Vec<DirEntryView> = node
            .children
            .iter()
            .map(|&c| {
                let child = &self.nodes[c as usize];
                DirEntryView {
                    name: child.name.to_string(),
                    path: join(&full, &child.name),
                    bytes: child.bytes,
                    file_count: child.files,
                    is_dir: true,
                    percent: percent(child.bytes),
                    has_children: !child.children.is_empty(),
                    denied: child.denied,
                }
            })
            .collect();
        if node.loose_files > 0 {
            entries.push(DirEntryView {
                name: LOOSE_FILES.to_string(),
                path: String::new(),
                bytes: node.loose_bytes,
                file_count: node.loose_files,
                is_dir: false,
                percent: percent(node.loose_bytes),
                has_children: false,
                denied: false,
            });
        }
        entries.sort_by_key(|e| std::cmp::Reverse(e.bytes));

        Some(DirListing {
            parent: (idx != 0).then(|| self.full_path(node.parent as usize)),
            path: full,
            bytes: node.bytes,
            file_count: node.files,
            entries,
        })
    }

    /// The scanned root, as given to `scan`.
    pub fn root(&self) -> &str {
        &self.root
    }

    /// Walk down from the root one name at a time, ignoring case. This avoids
    /// keeping a full path string for every folder on the drive.
    fn find(&self, path: &str) -> Option<usize> {
        let wanted = split(path);
        let root = split(&self.root);
        if wanted.len() < root.len() || !wanted.iter().zip(&root).all(|(a, b)| same_name(a, b)) {
            return None;
        }
        let mut idx = 0usize;
        for part in &wanted[root.len()..] {
            let node = self.nodes.get(idx)?;
            idx = node.children.iter().map(|&c| c as usize).find(|&c| same_name(&self.nodes[c].name, part))?;
        }
        Some(idx)
    }

    fn full_path(&self, mut idx: usize) -> String {
        let mut names = Vec::new();
        while idx != 0 {
            let node = &self.nodes[idx];
            names.push(&*node.name);
            idx = node.parent as usize;
        }
        names.iter().rev().fold(self.root.clone(), |acc, n| join(&acc, n))
    }
}

fn split(path: &str) -> Vec<&str> {
    path.split(['\\', '/']).filter(|p| !p.is_empty()).collect()
}

fn same_name(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.eq_ignore_ascii_case(b) || a.to_lowercase() == b.to_lowercase()
}

fn join(base: &str, name: &str) -> String {
    if base.ends_with('\\') || base.ends_with('/') {
        format!("{base}{name}")
    } else {
        format!("{base}\\{name}")
    }
}

/// Fixed and removable drives that are ready to read.
pub fn drives() -> Vec<Drive> {
    let Ok(disks) = sysinfo::Disks::new_with_refreshed_list() else { return Vec::new() };
    let mut out: Vec<Drive> = disks
        .iter()
        .filter(|d| d.total_space() > 0)
        .filter_map(|d| {
            let mount = d.mount_point().to_string_lossy().into_owned();
            // Only drive letters. Volumes mounted into folders show up under
            // the drive they live on.
            let b = mount.as_bytes();
            if !(b.len() == 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && b[2] == b'\\') {
                return None;
            }
            let kind = if d.is_removable() {
                "Removable"
            } else {
                match d.kind() {
                    sysinfo::DiskKind::SSD => "SSD",
                    sysinfo::DiskKind::HDD => "HDD",
                    _ => "Unknown",
                }
            };
            Some(Drive {
                mount,
                label: d.name().to_string_lossy().into_owned(),
                file_system: d.file_system().to_string_lossy().into_owned(),
                kind: kind.to_string(),
                total_bytes: d.total_space(),
                free_bytes: d.available_space(),
            })
        })
        .collect();
    out.sort_by(|a, b| a.mount.cmp(&b.mount));
    out.dedup_by(|a, b| a.mount == b.mount);
    out
}

/// A folder as the parallel walk builds it, before it is flattened.
struct Built {
    name: Box<str>,
    loose_bytes: u64,
    loose_files: u64,
    denied: bool,
    children: Vec<Built>,
}

struct Shared<'a, 'c> {
    tally: &'a Tally<'c>,
    kinds: [(AtomicU64, AtomicU64); 9],
}

fn build(dir: PathBuf, name: Box<str>, shared: &Shared) -> Built {
    let mut out = Built { name, loose_bytes: 0, loose_files: 0, denied: false, children: Vec::new() };
    if shared.tally.ctx().cancelled() {
        return out;
    }
    let read = match fs::read_dir(&dir) {
        Ok(r) => r,
        Err(_) => {
            out.denied = true;
            return out;
        }
    };
    let mut subdirs = Vec::new();
    let mut kinds = [(0u64, 0u64); 9];
    for entry in read {
        let Ok(entry) = entry else {
            out.denied = true;
            continue;
        };
        let Ok(meta) = entry.metadata() else {
            out.denied = true;
            continue;
        };
        let attrs = meta.file_attributes();
        let file_name = entry.file_name();
        if is_dir_attr(attrs) {
            // Never follow a junction, symlink or mount point.
            if !is_reparse(attrs) {
                subdirs.push((entry.path(), file_name.to_string_lossy().into()));
            }
            continue;
        }
        let bytes = local_bytes(attrs, meta.len());
        out.loose_bytes += bytes;
        out.loose_files += 1;
        let k = kind_index(file_kind(Path::new(&file_name)));
        kinds[k].0 += bytes;
        kinds[k].1 += 1;
    }
    for (i, (b, n)) in kinds.iter().enumerate() {
        if *n > 0 {
            shared.kinds[i].0.fetch_add(*b, Ordering::Relaxed);
            shared.kinds[i].1.fetch_add(*n, Ordering::Relaxed);
        }
    }
    shared.tally.add(out.loose_files, out.loose_bytes);
    out.children = subdirs.into_par_iter().map(|(p, n)| build(p, n, shared)).collect();
    out
}

/// Turn the built tree into the flat, sorted node list. Uses an explicit
/// stack so a very deep tree cannot overflow.
fn flatten(root: Built) -> Vec<Node> {
    // First pass: lay nodes out with their raw children, parent before child.
    let mut nodes: Vec<Node> = Vec::new();
    let mut kids: Vec<Vec<u32>> = Vec::new();
    let mut stack: Vec<(Built, u32)> = vec![(root, 0)];
    while let Some((b, parent)) = stack.pop() {
        let idx = nodes.len() as u32;
        if idx != 0 {
            kids[parent as usize].push(idx);
        }
        nodes.push(Node {
            name: b.name,
            parent,
            bytes: b.loose_bytes,
            files: b.loose_files,
            loose_bytes: b.loose_bytes,
            loose_files: b.loose_files,
            denied: b.denied,
            children: Box::new([]),
        });
        kids.push(Vec::new());
        for child in b.children {
            stack.push((child, idx));
        }
    }
    // Children always come after their parent, so walking backwards adds
    // every subtree into its parent after the subtree itself is complete.
    for i in (1..nodes.len()).rev() {
        let (bytes, files, denied, parent) = (nodes[i].bytes, nodes[i].files, nodes[i].denied, nodes[i].parent);
        let p = &mut nodes[parent as usize];
        p.bytes += bytes;
        p.files += files;
        p.denied |= denied;
    }
    for (i, mut list) in kids.into_iter().enumerate() {
        list.sort_by(|&a, &b| nodes[b as usize].bytes.cmp(&nodes[a as usize].bytes));
        nodes[i].children = list.into_boxed_slice();
    }
    nodes
}

/// Walk `root` and build the tree. Reports running totals through `ctx` a few
/// times a second and stops with `Error::Cancelled` if asked.
pub fn scan(root: &Path, ctx: &Ctx) -> Result<DiskTree> {
    let started = Instant::now();
    let root_str = root.to_string_lossy().into_owned();
    let tally = Tally::new(ctx, format!("Scanning {root_str}"));
    let shared = Shared { tally: &tally, kinds: Default::default() };
    let built = in_pool(|| build(root.to_path_buf(), root_str.clone().into(), &shared));
    if ctx.cancelled() {
        return Err(Error::Cancelled);
    }
    tally.finish();
    let nodes = flatten(built);

    let mut by_type: Vec<TypeUsage> = KINDS
        .iter()
        .zip(shared.kinds.iter())
        .map(|(k, (b, n))| TypeUsage {
            kind: (*k).to_string(),
            bytes: b.load(Ordering::Relaxed),
            file_count: n.load(Ordering::Relaxed),
        })
        .filter(|t| t.file_count > 0)
        .collect();
    by_type.sort_by_key(|t| std::cmp::Reverse(t.bytes));

    Ok(DiskTree {
        root: root_str,
        nodes,
        by_type,
        scanned_at: now_unix(),
        elapsed_ms: started.elapsed().as_millis() as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fsops::test_util::{make_junction, write};

    fn sample() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        let r = dir.path();
        write(&r.join("top.txt"), 100);
        write(&r.join(r"Big\film.mkv"), 5000);
        write(&r.join(r"Big\Inner\a.zip"), 2000);
        write(&r.join(r"Small\note.md"), 300);
        write(&r.join(r"Small\pic.jpg"), 600);
        fs::create_dir_all(r.join("Empty")).expect("mkdir");
        dir
    }

    #[test]
    fn totals_add_up_and_listing_is_sorted() {
        let dir = sample();
        let tree = scan(dir.path(), &Ctx::none()).expect("scan");
        let s = tree.summary();
        assert_eq!(s.bytes, 8000);
        assert_eq!(s.file_count, 5);
        assert_eq!(s.dir_count, 4);
        let video = s.by_type.iter().find(|t| t.kind == "Video").expect("video");
        assert_eq!(video.bytes, 5000);
        assert_eq!(s.by_type.iter().map(|t| t.bytes).sum::<u64>(), 8000);

        let root = tree.listing(tree.root()).expect("root listing");
        assert_eq!(root.parent, None);
        assert_eq!(root.bytes, 8000);
        let names: Vec<_> = root.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["Big", "Small", LOOSE_FILES, "Empty"]);
        assert!((root.entries[0].percent - 87.5).abs() < 0.01);
        let loose = &root.entries[2];
        assert!(!loose.is_dir && loose.path.is_empty() && loose.bytes == 100);
        let total: f32 = root.entries.iter().map(|e| e.percent).sum();
        assert!((total - 100.0).abs() < 0.01);
        assert!(root.entries[0].has_children);
        assert!(!root.entries[1].has_children);
    }

    #[test]
    fn lookup_ignores_case_and_sets_parent() {
        let dir = sample();
        let tree = scan(dir.path(), &Ctx::none()).expect("scan");
        let upper = format!("{}\\BIG\\inner\\", tree.root().to_uppercase());
        let inner = tree.listing(&upper).expect("inner");
        assert_eq!(inner.bytes, 2000);
        assert!(inner.path.ends_with(r"Big\Inner"));
        let parent = inner.parent.expect("parent");
        assert!(parent.ends_with("Big"));
        assert_eq!(tree.listing(&parent).expect("big").bytes, 7000);
        assert!(tree.listing(&format!("{}\\Nope", tree.root())).is_none());
        assert!(tree.listing(r"Z:\elsewhere").is_none());
    }

    #[test]
    fn junctions_are_not_followed() {
        let dir = sample();
        let outside = tempfile::tempdir().expect("tempdir");
        write(&outside.path().join("huge.bin"), 50_000);
        if !make_junction(&dir.path().join("Link"), outside.path()) {
            eprintln!("skipped: could not create a junction");
            return;
        }
        let tree = scan(dir.path(), &Ctx::none()).expect("scan");
        assert_eq!(tree.summary().bytes, 8000);
        assert!(tree.listing(&format!("{}\\Link", tree.root())).is_none());
    }

    #[test]
    fn tree_is_send_and_sync_and_cancel_works() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<DiskTree>();
        let dir = sample();
        let cancel = std::sync::atomic::AtomicBool::new(true);
        let ignore = |_| {};
        let ctx = Ctx::new(&ignore, &cancel);
        assert!(matches!(scan(dir.path(), &ctx), Err(Error::Cancelled)));
    }

    #[test]
    fn drives_lists_something() {
        let d = drives();
        assert!(d.iter().all(|d| d.mount.len() == 3 && d.total_bytes > 0));
    }
}
