//! What the app remembers between calls from the web view.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use quadrant_core::diskmap::DiskTree;
use quadrant_core::model::{CleanScan, DupScan, LargeScan};
use quadrant_core::system::LiveSampler;

/// Long-running jobs the user can cancel. Each has its own flag.
pub const TASKS: &[&str] = &["health", "junk", "privacy", "clean", "disk", "large", "duplicates", "remove"];

pub struct AppState {
    cancels: HashMap<&'static str, Arc<AtomicBool>>,
    /// The last disk map scan, kept so drilling in is instant.
    pub disk_tree: Mutex<Option<Arc<DiskTree>>>,
    /// The last scans whose results the user can act on. Removal commands
    /// resolve ids against these, never against anything the web view sends.
    pub large: Mutex<Option<LargeScan>>,
    pub dups: Mutex<Option<DupScan>>,
    /// The last junk scan, for the optional AI advice.
    pub junk: Mutex<Option<CleanScan>>,
    pub sampler: Mutex<LiveSampler>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            cancels: TASKS.iter().map(|t| (*t, Arc::new(AtomicBool::new(false)))).collect(),
            disk_tree: Mutex::new(None),
            large: Mutex::new(None),
            dups: Mutex::new(None),
            junk: Mutex::new(None),
            sampler: Mutex::new(LiveSampler::new()),
        }
    }

    /// The cancel flag for a task, reset ready for a new run.
    pub fn begin(&self, task: &str) -> Arc<AtomicBool> {
        let flag = self.cancels.get(task).cloned().unwrap_or_default();
        flag.store(false, Ordering::Relaxed);
        flag
    }

    /// Ask a running task to stop. Unknown task names are ignored.
    pub fn cancel(&self, task: &str) {
        if let Some(flag) = self.cancels.get(task) {
            flag.store(true, Ordering::Relaxed);
        }
    }
}

// The disk tree is shared with worker threads.
const _: fn() = || {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<DiskTree>();
};
