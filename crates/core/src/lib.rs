//! Quadrant core: scanning, cleaning and inspection logic for Windows.
//!
//! This crate has no UI dependencies. The Tauri shell in `src-tauri` calls
//! into it and forwards results to the web view as JSON.
//!
//! Safety rules that every module here follows:
//! - Destructive functions never take a free-form path from the caller. They
//!   take ids that are resolved against a fixed allowlist or a fresh listing.
//! - Deleting never follows symlinks or junctions, and never removes a
//!   category's root folder, only what is inside it.
//! - A locked or protected file is counted as skipped. It is never fatal.

pub mod ctx;
pub mod error;
pub mod model;
pub mod util;

pub mod cleanup;
pub mod diskmap;
pub mod duplicates;
pub mod fsops;
pub mod largefiles;

pub mod ai;
pub mod elevation;
pub mod network;
pub mod programs;
pub mod security;
pub mod shell;
pub mod startup;
pub mod system;
pub mod tasks;

pub mod health;
pub mod history;

pub use ctx::Ctx;
pub use error::{Error, Result};
