//! Quadrant app shell. Owns the window and turns web view calls into calls to
//! `quadrant-core`. No scanning or cleaning logic lives here.
//!
//! Security notes:
//! - The web view can only call the commands listed in `run()`. No Tauri
//!   plugin (file system, shell, HTTP) is exposed to it.
//! - Commands that remove or change things take ids, which the core resolves
//!   against its own allowlists or the scan results kept in `AppState`. A
//!   path sent by the web view is never deleted.
//! - The Gemini API key never reaches the web view.

mod chrome;
mod commands;
mod state;

use std::sync::Arc;

use tauri::Manager;

/// Environment variables that let whoever sets them steer WebView2: open a
/// remote debugging port, swap the browser binary, or move its data folder.
/// Quadrant often runs elevated, so a release build ignores all of them.
#[cfg(not(debug_assertions))]
const WEBVIEW2_OVERRIDES: &[&str] = &[
    "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS",
    "WEBVIEW2_BROWSER_EXECUTABLE_FOLDER",
    "WEBVIEW2_USER_DATA_FOLDER",
    "WEBVIEW2_RELEASE_CHANNEL_PREFERENCE",
    "WEBVIEW2_CHANNEL_SEARCH_KIND",
    "WEBVIEW2_PIPE_FOR_SCRIPT_DEBUGGER",
    "WEBVIEW2_WAIT_FOR_SCRIPT_DEBUGGER",
];

pub fn run() {
    #[cfg(not(debug_assertions))]
    for name in WEBVIEW2_OVERRIDES {
        std::env::remove_var(name);
    }

    tauri::Builder::default()
        .manage(Arc::new(state::AppState::new()))
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::app_ready,
            commands::set_theme,
            commands::relaunch_as_admin,
            commands::cancel,
            commands::system_overview,
            commands::system_live,
            commands::health_check,
            commands::clean_scan,
            commands::clean_files,
            commands::clean_run,
            commands::drives,
            commands::disk_scan,
            commands::disk_listing,
            commands::large_scan,
            commands::large_remove,
            commands::dup_scan,
            commands::dup_remove,
            commands::startup_list,
            commands::startup_set,
            commands::programs_list,
            commands::program_uninstall,
            commands::tasks_list,
            commands::task_set,
            commands::network_list,
            commands::security_checks,
            commands::open_known,
            commands::reveal,
            commands::pick_folder,
            commands::ai_status,
            commands::ai_set_key,
            commands::ai_clear_key,
            commands::ai_import_legacy,
            commands::ai_summarise,
            commands::history,
        ])
        .setup(|app| {
            // The window starts hidden and the UI shows it once it has
            // painted, which avoids a white flash. If the UI never reports
            // in, show the window anyway so the user is not left with nothing.
            if let Some(window) = app.get_webview_window("main") {
                chrome::apply(&window, "dark");
                let fallback = window.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(2500));
                    let _ = fallback.show();
                });
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Quadrant could not start");
}
