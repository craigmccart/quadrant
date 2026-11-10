//! Every type that crosses from Rust to the web view. This file is the
//! contract between the two halves of the app: the UI in `ui/js` reads these
//! shapes as camelCase JSON. Change a field here and the UI must change too.
//!
//! Times are Unix seconds (UTC). Sizes are bytes. The UI does the formatting.

use serde::{Deserialize, Serialize};

// ── Shared ───────────────────────────────────────────────────────────────────

/// Progress for a long-running job, sent to the UI as it happens.
#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    /// Short status line, e.g. "Scanning Chrome cache".
    pub message: String,
    /// 0.0 to 1.0 when the amount of work is known.
    pub fraction: Option<f32>,
    /// Running count of files seen or handled.
    pub items: u64,
    /// Running total of bytes seen or handled.
    pub bytes: u64,
}

/// How confident we are that removing something is harmless.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Risk {
    /// Windows or the app rebuilds it. Nothing of the user's is lost.
    Safe,
    /// Might matter to the user. Never removed without them looking first.
    Review,
}

/// A traffic-light result used by security checks and health pillars.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Good,
    Attention,
    Problem,
    Unknown,
}

/// Whether something belongs to the signed-in user or the whole machine.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    User,
    Machine,
}

/// One file in a listing.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub path: String,
    pub bytes: u64,
    /// Last modified, Unix seconds.
    pub modified: Option<i64>,
}

// ── Cleaning (junk and privacy) ──────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Area {
    /// Disk space: temp files, caches, dumps, logs.
    Junk,
    /// Traces of what the user has opened or typed.
    Privacy,
}

/// Options that change what a clean will touch.
#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CleanOptions {
    /// Temp files younger than this are left alone, because a running
    /// installer or app may still need them. 0 turns the rule off.
    pub temp_min_age_hours: u32,
}

impl Default for CleanOptions {
    fn default() -> Self {
        Self { temp_min_age_hours: 24 }
    }
}

/// One line in the cleaner, e.g. "Chrome cache".
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CleanCategory {
    /// Stable id from the fixed allowlist, e.g. "user-temp".
    pub id: String,
    pub area: Area,
    /// Heading it sits under: "Windows", "Browsers", "Developer", "Downloads",
    /// "Explorer".
    pub group: String,
    pub label: String,
    /// One plain sentence: what this is.
    pub description: String,
    /// One plain sentence: what happens if it is removed, or a caveat.
    pub note: String,
    pub risk: Risk,
    pub bytes: u64,
    /// Files, or registry values for registry-backed privacy items.
    pub item_count: u64,
    /// True when a full clean needs administrator rights.
    pub needs_admin: bool,
    /// Ticked by default in the UI. Review items are never ticked by default.
    pub default_selected: bool,
    /// True when removed items go to the Recycle Bin instead of being deleted.
    pub to_recycle_bin: bool,
    /// True when the UI can show the individual files (false for registry
    /// items and the Recycle Bin).
    pub can_list: bool,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CleanScan {
    pub area: Area,
    /// Every category that exists on this machine, biggest first.
    pub categories: Vec<CleanCategory>,
    pub total_bytes: u64,
    /// Bytes in categories with `risk == Safe`.
    pub safe_bytes: u64,
    pub total_items: u64,
    pub scanned_at: i64,
    pub elapsed_ms: u64,
}

/// The files behind one category, for the drill-down view.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CategoryFiles {
    pub id: String,
    /// Biggest first, capped at the limit asked for.
    pub files: Vec<FileEntry>,
    pub total_items: u64,
    pub truncated: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CleanItemResult {
    pub id: String,
    pub label: String,
    pub freed_bytes: u64,
    pub removed: u64,
    /// Locked, in use, too new, or access denied.
    pub skipped: u64,
}

/// The receipt for one clean.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CleanReport {
    pub area: Area,
    pub items: Vec<CleanItemResult>,
    pub freed_bytes: u64,
    pub removed: u64,
    pub skipped: u64,
    pub finished_at: i64,
    pub elapsed_ms: u64,
}

// ── Disk map ─────────────────────────────────────────────────────────────────

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Drive {
    /// For example `C:\`
    pub mount: String,
    /// Volume label, may be empty.
    pub label: String,
    pub file_system: String,
    /// "SSD", "HDD", "Removable", "Network" or "Unknown".
    pub kind: String,
    pub total_bytes: u64,
    pub free_bytes: u64,
}

/// One row in a folder listing.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DirEntryView {
    pub name: String,
    /// Full path. Empty for the synthetic "files in this folder" row.
    pub path: String,
    pub bytes: u64,
    pub file_count: u64,
    pub is_dir: bool,
    /// Share of the parent folder, 0 to 100.
    pub percent: f32,
    /// True when the UI can drill into it.
    pub has_children: bool,
    /// True when Windows refused access to some or all of it.
    pub denied: bool,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DirListing {
    pub path: String,
    /// Parent folder inside the scanned tree. None at the scanned root.
    pub parent: Option<String>,
    pub bytes: u64,
    pub file_count: u64,
    /// Biggest first. Includes one row with `is_dir == false` and an empty
    /// path that sums the loose files directly in this folder, if any.
    pub entries: Vec<DirEntryView>,
}

/// Space used by one kind of file across the scanned tree.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TypeUsage {
    /// "Video", "Images", "Audio", "Documents", "Archives", "Apps", "Code",
    /// "System" or "Other".
    pub kind: String,
    pub bytes: u64,
    pub file_count: u64,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DiskScanSummary {
    pub root: String,
    pub bytes: u64,
    pub file_count: u64,
    pub dir_count: u64,
    /// Biggest first.
    pub by_type: Vec<TypeUsage>,
    pub scanned_at: i64,
    pub elapsed_ms: u64,
}

// ── Large files and duplicates ───────────────────────────────────────────────

/// A file found by a scan. `id` is only meaningful within that scan.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FoundFile {
    pub id: u32,
    pub path: String,
    /// File name only.
    pub name: String,
    /// Containing folder.
    pub dir: String,
    pub bytes: u64,
    pub modified: Option<i64>,
    /// Same vocabulary as `TypeUsage::kind`.
    pub kind: String,
    /// True when Quadrant will refuse to remove it (Windows system locations,
    /// page file and the like). The UI shows these without a tick box.
    pub protected: bool,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LargeScan {
    pub root: String,
    pub min_bytes: u64,
    /// Biggest first, capped (see `truncated`).
    pub files: Vec<FoundFile>,
    pub total_count: u64,
    pub total_bytes: u64,
    pub truncated: bool,
    pub elapsed_ms: u64,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DupGroup {
    pub id: u32,
    /// Size of each copy.
    pub bytes: u64,
    /// First 16 hex characters of the content hash.
    pub hash: String,
    /// Two or more identical files, oldest first.
    pub files: Vec<FoundFile>,
    /// bytes * (copies - 1)
    pub wasted_bytes: u64,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DupScan {
    pub root: String,
    pub min_bytes: u64,
    /// Most wasted space first, capped (see `truncated`).
    pub groups: Vec<DupGroup>,
    pub total_groups: u64,
    pub wasted_bytes: u64,
    pub files_checked: u64,
    pub truncated: bool,
    pub elapsed_ms: u64,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RemoveFailure {
    pub path: String,
    pub reason: String,
}

/// Result of removing user files (large files, duplicates).
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RemoveReport {
    pub removed: u64,
    pub failed: u64,
    pub freed_bytes: u64,
    /// First 50 failures.
    pub failures: Vec<RemoveFailure>,
    pub to_recycle_bin: bool,
    /// Ids that were removed, so the UI can drop them from its list.
    pub removed_ids: Vec<u32>,
}

// ── System ───────────────────────────────────────────────────────────────────

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SystemOverview {
    pub host_name: String,
    pub user_name: String,
    /// For example "Windows 11 Pro".
    pub os_name: String,
    /// Display version, for example "24H2". May be empty.
    pub os_version: String,
    /// For example "26100.2314".
    pub os_build: String,
    pub manufacturer: String,
    pub model: String,
    pub cpu_name: String,
    pub cpu_cores_physical: u32,
    pub cpu_cores_logical: u32,
    pub gpus: Vec<String>,
    pub ram_total_bytes: u64,
    pub ram_used_bytes: u64,
    pub uptime_secs: u64,
    pub boot_time: i64,
    pub is_elevated: bool,
    pub drives: Vec<Drive>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LiveStats {
    /// Whole-machine CPU use, 0 to 100.
    pub cpu_percent: f32,
    pub ram_used_bytes: u64,
    pub ram_total_bytes: u64,
}

// ── Startup ──────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum StartupSource {
    RegistryRun,
    RegistryRunOnce,
    StartupFolder,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct StartupItem {
    /// Opaque id. Only valid against a fresh `startup::list()`.
    pub id: String,
    pub name: String,
    /// The full command line or shortcut path as registered.
    pub command: String,
    /// Best guess at the program being launched. May be empty.
    pub target: String,
    pub source: StartupSource,
    pub scope: Scope,
    /// Plain label, for example "Registry, this user".
    pub location: String,
    pub enabled: bool,
    /// False for RunOnce entries, which Windows removes by itself.
    pub can_toggle: bool,
    /// True when changing it needs administrator rights.
    pub needs_admin: bool,
}

// ── Programs ─────────────────────────────────────────────────────────────────

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Program {
    /// Opaque id. Only valid against a fresh `programs::list()`.
    pub id: String,
    pub name: String,
    pub publisher: String,
    pub version: String,
    pub size_bytes: Option<u64>,
    /// "YYYY-MM-DD" when Windows recorded it.
    pub installed_on: Option<String>,
    pub install_location: String,
    pub scope: Scope,
    pub can_uninstall: bool,
}

// ── Scheduled tasks ──────────────────────────────────────────────────────────

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TaskItem {
    /// Full task path, for example `\Adobe\Updater`. This is the id.
    pub path: String,
    pub name: String,
    /// Folder part of the path, `\` for the root.
    pub folder: String,
    /// "Ready", "Running", "Disabled" and so on, as Windows reports it.
    pub status: String,
    pub enabled: bool,
    pub next_run: String,
    pub last_run: String,
    pub last_result: String,
    pub author: String,
    /// What the task runs.
    pub action: String,
}

// ── Network ──────────────────────────────────────────────────────────────────

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    /// "TCP" or "UDP"
    pub protocol: String,
    pub local_address: String,
    pub local_port: u16,
    /// Empty for listeners and UDP.
    pub remote_address: String,
    pub remote_port: u16,
    /// "Established", "Listening", "Time wait" and so on. Empty for UDP.
    pub state: String,
    pub pid: u32,
    /// Process name, empty when unknown.
    pub process: String,
    /// True when the remote end is this machine (127.0.0.0/8 or ::1).
    pub loopback: bool,
}

// ── Security ─────────────────────────────────────────────────────────────────

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SecurityCheck {
    /// For example "antivirus", "firewall", "uac".
    pub id: String,
    pub label: String,
    pub status: Status,
    /// What we found, for example "On for all networks".
    pub summary: String,
    /// Why it matters, one plain sentence.
    pub detail: String,
    /// Id of a Windows settings page that fixes it (see `security::open_page`).
    pub fix_page: Option<String>,
}

// ── Health check ─────────────────────────────────────────────────────────────

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Fact {
    pub label: String,
    pub value: String,
}

/// One of the four dial segments.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Pillar {
    /// "space", "privacy", "speed" or "security".
    pub id: String,
    pub label: String,
    /// 0 to 100.
    pub score: u8,
    pub status: Status,
    /// The one number that matters, for example "4.2 GB".
    pub headline: String,
    /// What the headline number is, for example "of junk to clear".
    pub caption: String,
    pub facts: Vec<Fact>,
    /// UI page to open for the detail, for example "junk".
    pub page: String,
    pub action_label: String,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct HealthReport {
    /// 0 to 100.
    pub score: u8,
    pub status: Status,
    /// One sentence verdict.
    pub summary: String,
    pub pillars: Vec<Pillar>,
    pub scanned_at: i64,
    pub elapsed_ms: u64,
    pub junk: CleanScan,
    pub privacy: CleanScan,
    pub startup: Vec<StartupItem>,
    pub security: Vec<SecurityCheck>,
    /// Junk category ids that the one-click clean would remove (Safe only).
    pub quick_clean_ids: Vec<String>,
    pub quick_clean_bytes: u64,
}

// ── History ──────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub at: i64,
    /// "junk", "privacy", "large" or "duplicates".
    pub kind: String,
    pub freed_bytes: u64,
    pub removed: u64,
    pub skipped: u64,
    pub items: Vec<CleanItemResult>,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct History {
    /// Newest first.
    pub entries: Vec<HistoryEntry>,
    pub total_freed_bytes: u64,
    pub total_removed: u64,
}

// ── AI ───────────────────────────────────────────────────────────────────────

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AiStatus {
    /// The checked address of the local model server, `http://host:port`.
    pub endpoint: String,
    /// True when a model server answered there.
    pub reachable: bool,
    /// Names of the models it offers, sorted.
    pub models: Vec<String>,
}
