//! A parallel folder walk shared by the large file and duplicate finders.
//!
//! It never descends into a folder that is a reparse point (junction,
//! symlink, mount point), so a scan cannot wander outside the folder the
//! user picked or loop forever.

use std::fs;
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use rayon::prelude::*;

use super::{is_dir_attr, is_reparse, local_bytes};
use crate::Ctx;

/// How many files pass between progress messages.
const TALLY_EVERY: u64 = 1_000;

/// Run `f` on a rayon pool with generous stacks: the walk recurses once per
/// folder level, and work stealing can stack several walks on one thread.
pub(crate) fn in_pool<R: Send>(f: impl FnOnce() -> R + Send) -> R {
    match rayon::ThreadPoolBuilder::new().stack_size(32 << 20).build() {
        Ok(pool) => pool.install(f),
        // Falling back to the global pool still works, with smaller stacks.
        Err(_) => f(),
    }
}

/// Running totals reported to the UI while walking.
pub(crate) struct Tally<'c> {
    ctx: &'c Ctx<'c>,
    message: String,
    items: AtomicU64,
    bytes: AtomicU64,
}

impl<'c> Tally<'c> {
    pub(crate) fn new(ctx: &'c Ctx<'c>, message: String) -> Self {
        Self { ctx, message, items: AtomicU64::new(0), bytes: AtomicU64::new(0) }
    }

    pub(crate) fn add(&self, items: u64, bytes: u64) {
        if items == 0 {
            return;
        }
        let before = self.items.fetch_add(items, Ordering::Relaxed);
        let total_bytes = self.bytes.fetch_add(bytes, Ordering::Relaxed) + bytes;
        if before / TALLY_EVERY != (before + items) / TALLY_EVERY {
            self.ctx.tally(self.message.clone(), before + items, total_bytes);
        }
    }

    pub(crate) fn finish(&self) {
        self.ctx.tally(
            self.message.clone(),
            self.items.load(Ordering::Relaxed),
            self.bytes.load(Ordering::Relaxed),
        );
    }

    pub(crate) fn ctx(&self) -> &Ctx<'c> {
        self.ctx
    }
}

/// Call `visit` for every entry under `root` that is not a folder, in
/// parallel. Folder links are skipped, never followed. Folders Windows will
/// not open are skipped quietly. Call from inside `in_pool`.
pub(crate) fn par_walk_files<F>(root: &Path, tally: &Tally, visit: &F)
where
    F: Fn(&Path, &fs::Metadata) + Sync,
{
    walk_dir(root.to_path_buf(), tally, visit);
}

fn walk_dir<F>(dir: PathBuf, tally: &Tally, visit: &F)
where
    F: Fn(&Path, &fs::Metadata) + Sync,
{
    if tally.ctx().cancelled() {
        return;
    }
    let Ok(read) = fs::read_dir(&dir) else { return };
    let mut subdirs = Vec::new();
    let (mut items, mut bytes) = (0u64, 0u64);
    for entry in read.flatten() {
        // On Windows this comes from the directory listing itself and does
        // not follow links.
        let Ok(meta) = entry.metadata() else { continue };
        let attrs = meta.file_attributes();
        if is_dir_attr(attrs) {
            if !is_reparse(attrs) {
                subdirs.push(entry.path());
            }
            continue;
        }
        items += 1;
        bytes += local_bytes(attrs, meta.len());
        visit(&entry.path(), &meta);
    }
    tally.add(items, bytes);
    subdirs.into_par_iter().for_each(|d| walk_dir(d, tally, visit));
}
