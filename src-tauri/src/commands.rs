//! The commands the web view may call. Each one is a thin wrapper: check the
//! input, hand the work to `quadrant-core` on a worker thread, pass the result
//! back as JSON.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, State, WebviewWindow};

use quadrant_core::model::{
    AiStatus, Area, CategoryFiles, CleanOptions, CleanReport, CleanScan, Connection, DirListing,
    DiskScanSummary, Drive, DupScan, HealthReport, History, LargeScan, LiveStats, Program,
    Progress, RemoveReport, SecurityCheck, StartupItem, SystemOverview, TaskItem,
};
use quadrant_core::{
    ai, cleanup, diskmap, duplicates, elevation, fsops, health, history, largefiles, network,
    programs, secrets, security, shell, startup, system, tasks, Ctx, Error,
};

use crate::state::AppState;

type Shared<'a> = State<'a, Arc<AppState>>;

// ── Errors ───────────────────────────────────────────────────────────────────

/// What the web view gets when a command fails. `kind` lets the UI react
/// (offer "restart as administrator" for "denied", stay quiet for
/// "cancelled"); `message` is shown to the user as it is.
#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CmdError {
    kind: &'static str,
    message: String,
}

impl From<Error> for CmdError {
    fn from(e: Error) -> Self {
        let kind = match &e {
            Error::Cancelled => "cancelled",
            Error::Denied(_) => "denied",
            Error::NotFound(_) => "notFound",
            Error::Refused(_) => "refused",
            Error::Message(_) | Error::Io(_) => "message",
        };
        CmdError { kind, message: e.to_string() }
    }
}

type Cmd<T> = Result<T, CmdError>;

/// Run blocking work off the UI thread. A panic in the work comes back as an
/// ordinary error instead of taking the app down.
async fn blocking<T, F>(work: F) -> Cmd<T>
where
    F: FnOnce() -> quadrant_core::Result<T> + Send + 'static,
    T: Send + 'static,
{
    match tauri::async_runtime::spawn_blocking(work).await {
        Ok(result) => result.map_err(CmdError::from),
        Err(e) => Err(CmdError {
            kind: "message",
            message: format!("That stopped unexpectedly ({e}). Nothing further was changed."),
        }),
    }
}

/// Lock a mutex even if a previous holder panicked: the data is still usable.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn existing_dir(path: &str) -> quadrant_core::Result<PathBuf> {
    let p = PathBuf::from(path.trim());
    if !p.is_absolute() {
        return Err(Error::msg(r"Choose a full folder path, for example C:\Users."));
    }
    if !p.is_dir() {
        return Err(Error::not_found(format!("{} is not a folder that exists", p.display())));
    }
    Ok(p)
}

// ── Progress events ──────────────────────────────────────────────────────────

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ProgressEvent {
    task: &'static str,
    #[serde(flatten)]
    progress: Progress,
}

/// Build the progress callback for a task. Running totals are throttled to
/// about twenty a second so a fast disk walk cannot flood the web view;
/// status changes and the final message always get through.
fn emitter(app: &AppHandle, task: &'static str) -> impl Fn(Progress) + Send + Sync + 'static {
    let app = app.clone();
    let last: Mutex<(Option<Instant>, String)> = Mutex::new((None, String::new()));
    move |p: Progress| {
        {
            let mut seen = lock(&last);
            let due = seen.0.map_or(true, |t| t.elapsed() >= Duration::from_millis(50));
            let is_status = p.items == 0 && p.bytes == 0 && p.message != seen.1;
            let done = p.fraction == Some(1.0);
            if !(due || is_status || done) {
                return;
            }
            seen.0 = Some(Instant::now());
            seen.1.clone_from(&p.message);
        }
        let _ = app.emit("progress", ProgressEvent { task, progress: p });
    }
}

// ── App ──────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    version: &'static str,
    is_elevated: bool,
    /// The signed-in user's profile folder, the default place to scan.
    home_dir: String,
}

#[tauri::command]
pub fn app_info() -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        is_elevated: elevation::is_elevated(),
        home_dir: std::env::var("USERPROFILE").unwrap_or_default(),
    }
}

/// Called by the UI once it has painted. The window starts hidden.
#[tauri::command]
pub fn app_ready(window: WebviewWindow) {
    let _ = window.show();
    let _ = window.set_focus();
}

/// Tint the native title bar to match the UI theme ("dark" or "light").
#[tauri::command]
pub fn set_theme(window: WebviewWindow, theme: String) {
    crate::chrome::apply(&window, &theme);
}

#[tauri::command]
pub async fn relaunch_as_admin(app: AppHandle) -> Cmd<()> {
    blocking(elevation::relaunch_elevated).await?;
    app.exit(0);
    Ok(())
}

#[tauri::command]
pub fn cancel(state: Shared<'_>, task: String) {
    state.cancel(&task);
}

// ── System ───────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn system_overview() -> Cmd<SystemOverview> {
    blocking(|| Ok(system::overview())).await
}

#[tauri::command]
pub async fn system_live(state: Shared<'_>) -> Cmd<LiveStats> {
    let st = state.inner().clone();
    blocking(move || Ok(lock(&st.sampler).sample())).await
}

// ── Health check ─────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn health_check(
    app: AppHandle,
    state: Shared<'_>,
    options: Option<CleanOptions>,
) -> Cmd<HealthReport> {
    let st = state.inner().clone();
    let cancel = st.begin("health");
    let progress = emitter(&app, "health");
    blocking(move || {
        let report = health::run(options.unwrap_or_default(), &Ctx::new(&progress, &cancel))?;
        *lock(&st.junk) = Some(report.junk.clone());
        Ok(report)
    })
    .await
}

// ── Cleaner (junk and privacy) ───────────────────────────────────────────────

fn area_task(area: Area) -> &'static str {
    match area {
        Area::Junk => "junk",
        Area::Privacy => "privacy",
    }
}

#[tauri::command]
pub async fn clean_scan(
    app: AppHandle,
    state: Shared<'_>,
    area: Area,
    options: Option<CleanOptions>,
) -> Cmd<CleanScan> {
    let st = state.inner().clone();
    let task = area_task(area);
    let cancel = st.begin(task);
    let progress = emitter(&app, task);
    blocking(move || {
        let scan = cleanup::scan(area, options.unwrap_or_default(), &Ctx::new(&progress, &cancel))?;
        if area == Area::Junk {
            *lock(&st.junk) = Some(scan.clone());
        }
        Ok(scan)
    })
    .await
}

#[tauri::command]
pub async fn clean_files(id: String, options: Option<CleanOptions>) -> Cmd<CategoryFiles> {
    blocking(move || cleanup::list_files(&id, options.unwrap_or_default(), 500)).await
}

#[tauri::command]
pub async fn clean_run(
    app: AppHandle,
    state: Shared<'_>,
    area: Area,
    ids: Vec<String>,
    options: Option<CleanOptions>,
) -> Cmd<CleanReport> {
    let st = state.inner().clone();
    let cancel = st.begin("clean");
    let progress = emitter(&app, "clean");
    blocking(move || {
        let report =
            cleanup::clean(area, &ids, options.unwrap_or_default(), &Ctx::new(&progress, &cancel))?;
        history::record_clean(area_task(area), &report);
        Ok(report)
    })
    .await
}

// ── Disk map ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn drives() -> Cmd<Vec<Drive>> {
    blocking(|| Ok(diskmap::drives())).await
}

#[tauri::command]
pub async fn disk_scan(app: AppHandle, state: Shared<'_>, path: String) -> Cmd<DiskScanSummary> {
    let st = state.inner().clone();
    let cancel = st.begin("disk");
    let progress = emitter(&app, "disk");
    blocking(move || {
        let root = existing_dir(&path)?;
        let tree = diskmap::scan(&root, &Ctx::new(&progress, &cancel))?;
        let summary = tree.summary();
        *lock(&st.disk_tree) = Some(Arc::new(tree));
        Ok(summary)
    })
    .await
}

#[tauri::command]
pub async fn disk_listing(state: Shared<'_>, path: String) -> Cmd<DirListing> {
    let tree = lock(&state.disk_tree).clone();
    blocking(move || {
        let tree = tree.ok_or_else(|| Error::msg("Scan a drive or folder first."))?;
        tree.listing(&path)
            .ok_or_else(|| Error::not_found(format!("{path} is not part of the last scan")))
    })
    .await
}

// ── Large files ──────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn large_scan(
    app: AppHandle,
    state: Shared<'_>,
    path: String,
    min_bytes: u64,
) -> Cmd<LargeScan> {
    let st = state.inner().clone();
    let cancel = st.begin("large");
    let progress = emitter(&app, "large");
    blocking(move || {
        let root = existing_dir(&path)?;
        let scan = largefiles::scan(&root, min_bytes, &Ctx::new(&progress, &cancel))?;
        *lock(&st.large) = Some(scan.clone());
        Ok(scan)
    })
    .await
}

#[tauri::command]
pub async fn large_remove(
    app: AppHandle,
    state: Shared<'_>,
    ids: Vec<u32>,
    to_recycle_bin: bool,
) -> Cmd<RemoveReport> {
    let st = state.inner().clone();
    let cancel = st.begin("remove");
    let progress = emitter(&app, "remove");
    blocking(move || {
        let targets = {
            let guard = lock(&st.large);
            let scan = guard.as_ref().ok_or_else(|| Error::msg("Run a scan first."))?;
            largefiles::removal_targets(scan, &ids)?
        };
        let report = fsops::remove_files(&targets, to_recycle_bin, &Ctx::new(&progress, &cancel));
        if let Some(scan) = lock(&st.large).as_mut() {
            scan.files.retain(|f| !report.removed_ids.contains(&f.id));
        }
        history::record_removal("large", "Large files", &report);
        Ok(report)
    })
    .await
}

// ── Duplicates ───────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn dup_scan(
    app: AppHandle,
    state: Shared<'_>,
    path: String,
    min_bytes: u64,
) -> Cmd<DupScan> {
    let st = state.inner().clone();
    let cancel = st.begin("duplicates");
    let progress = emitter(&app, "duplicates");
    blocking(move || {
        let root = existing_dir(&path)?;
        let scan = duplicates::scan(&root, min_bytes, &Ctx::new(&progress, &cancel))?;
        *lock(&st.dups) = Some(scan.clone());
        Ok(scan)
    })
    .await
}

#[tauri::command]
pub async fn dup_remove(
    app: AppHandle,
    state: Shared<'_>,
    ids: Vec<u32>,
    to_recycle_bin: bool,
) -> Cmd<RemoveReport> {
    let st = state.inner().clone();
    let cancel = st.begin("remove");
    let progress = emitter(&app, "remove");
    blocking(move || {
        let targets = {
            let guard = lock(&st.dups);
            let scan = guard.as_ref().ok_or_else(|| Error::msg("Run a scan first."))?;
            duplicates::removal_targets(scan, &ids)?
        };
        let report = fsops::remove_files(&targets, to_recycle_bin, &Ctx::new(&progress, &cancel));
        if let Some(scan) = lock(&st.dups).as_mut() {
            // Drop what was removed, and any group that is no longer a
            // duplicate set, so a later call cannot act on stale results.
            for group in scan.groups.iter_mut() {
                group.files.retain(|f| !report.removed_ids.contains(&f.id));
                group.wasted_bytes = group.bytes * (group.files.len().saturating_sub(1) as u64);
            }
            scan.groups.retain(|g| g.files.len() > 1);
            scan.wasted_bytes = scan.groups.iter().map(|g| g.wasted_bytes).sum();
        }
        history::record_removal("duplicates", "Duplicate files", &report);
        Ok(report)
    })
    .await
}

// ── Startup, programs, tasks, network, security ──────────────────────────────

#[tauri::command]
pub async fn startup_list() -> Cmd<Vec<StartupItem>> {
    blocking(|| Ok(startup::list())).await
}

#[tauri::command]
pub async fn startup_set(id: String, enabled: bool) -> Cmd<Vec<StartupItem>> {
    blocking(move || {
        startup::set_enabled(&id, enabled)?;
        Ok(startup::list())
    })
    .await
}

#[tauri::command]
pub async fn programs_list() -> Cmd<Vec<Program>> {
    blocking(|| Ok(programs::list())).await
}

#[tauri::command]
pub async fn program_uninstall(id: String) -> Cmd<()> {
    blocking(move || programs::uninstall(&id)).await
}

#[tauri::command]
pub async fn tasks_list() -> Cmd<Vec<TaskItem>> {
    blocking(tasks::list).await
}

#[tauri::command]
pub async fn task_set(path: String, enabled: bool) -> Cmd<Vec<TaskItem>> {
    blocking(move || {
        tasks::set_enabled(&path, enabled)?;
        tasks::list()
    })
    .await
}

#[tauri::command]
pub async fn network_list() -> Cmd<Vec<Connection>> {
    blocking(network::connections).await
}

#[tauri::command]
pub async fn security_checks() -> Cmd<Vec<SecurityCheck>> {
    blocking(|| Ok(security::checks())).await
}

// ── Windows pages and folders ────────────────────────────────────────────────

#[tauri::command]
pub async fn open_known(id: String) -> Cmd<()> {
    blocking(move || shell::open_known(&id)).await
}

#[tauri::command]
pub async fn reveal(path: String) -> Cmd<()> {
    blocking(move || shell::reveal_in_explorer(&PathBuf::from(path))).await
}

/// Show the Windows folder picker. None when the user cancels.
#[tauri::command]
pub async fn pick_folder(window: WebviewWindow) -> Cmd<Option<String>> {
    let dialog = rfd::AsyncFileDialog::new().set_title("Choose a folder").set_parent(&window);
    Ok(dialog.pick_folder().await.map(|folder| folder.path().display().to_string()))
}

// ── AI advice ────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn ai_status() -> Cmd<AiStatus> {
    blocking(|| Ok(ai::status())).await
}

#[tauri::command]
pub async fn ai_set_key(key: String) -> Cmd<AiStatus> {
    blocking(move || {
        secrets::set_api_key(key.trim())?;
        Ok(ai::status())
    })
    .await
}

#[tauri::command]
pub async fn ai_clear_key() -> Cmd<AiStatus> {
    blocking(|| {
        secrets::clear_api_key()?;
        Ok(ai::status())
    })
    .await
}

#[tauri::command]
pub async fn ai_import_legacy() -> Cmd<AiStatus> {
    blocking(|| {
        secrets::import_legacy_key()?;
        Ok(ai::status())
    })
    .await
}

#[tauri::command]
pub async fn ai_summarise(state: Shared<'_>) -> Cmd<String> {
    let scan = lock(&state.junk).clone();
    blocking(move || {
        let scan = scan.ok_or_else(|| Error::msg("Run a junk scan first, then ask for advice."))?;
        ai::summarise(&scan)
    })
    .await
}

// ── History ──────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn history() -> Cmd<History> {
    blocking(|| Ok(history::load())).await
}
