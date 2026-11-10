//! Progress reporting and cancellation for long-running work.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::model::Progress;

/// Passed into every long-running function. Cheap to copy around by reference
/// and safe to share across rayon threads.
pub struct Ctx<'a> {
    progress: &'a (dyn Fn(Progress) + Sync),
    cancel: &'a AtomicBool,
}

static NEVER: AtomicBool = AtomicBool::new(false);

fn ignore(_: Progress) {}

impl<'a> Ctx<'a> {
    pub fn new(progress: &'a (dyn Fn(Progress) + Sync), cancel: &'a AtomicBool) -> Self {
        Self { progress, cancel }
    }

    /// No progress reporting and never cancelled. For tests and quick calls.
    pub fn none() -> Ctx<'static> {
        Ctx { progress: &ignore, cancel: &NEVER }
    }

    /// The same cancel flag with a different progress sink. Used to wrap a
    /// sub-task so its messages can be folded into a bigger job's progress.
    pub fn with_progress<'b>(&self, progress: &'b (dyn Fn(Progress) + Sync)) -> Ctx<'b>
    where
        'a: 'b,
    {
        Ctx { progress, cancel: self.cancel }
    }

    /// True once the user has asked to stop. Check this inside loops.
    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    /// Report a full progress record.
    pub fn report(&self, p: Progress) {
        (self.progress)(p)
    }

    /// Report a plain status line, e.g. "Scanning Chrome cache".
    pub fn status(&self, message: impl Into<String>) {
        self.report(Progress { message: message.into(), ..Progress::default() })
    }

    /// Report a status line with a known fraction complete (0.0 to 1.0).
    pub fn step(&self, message: impl Into<String>, fraction: f32) {
        self.report(Progress {
            message: message.into(),
            fraction: Some(fraction.clamp(0.0, 1.0)),
            ..Progress::default()
        })
    }

    /// Report running totals while walking the disk.
    pub fn tally(&self, message: impl Into<String>, items: u64, bytes: u64) {
        self.report(Progress { message: message.into(), fraction: None, items, bytes })
    }
}
