// The bridge to the Rust side. Every call the UI makes to the backend goes
// through `api` here, so this file is the full list of what the web view can
// ask for. The shapes of the results are defined in crates/core/src/model.rs.
//
// When the page is opened in an ordinary browser (no Tauri), a mock backend
// with made-up data is used instead, which is how the UI is designed and
// checked without touching a real machine.

const tauri = window.__TAURI__;

/** True inside the real app, false in a browser preview. */
export const isDesktop = Boolean(tauri);

const backend = isDesktop
  ? { invoke: tauri.core.invoke, listen: tauri.event.listen }
  : await import("./mock.js");

/** Errors always arrive as { kind, message }. See CmdError in commands.rs. */
function normaliseError(e) {
  if (e && typeof e === "object" && typeof e.message === "string") {
    return { kind: e.kind || "message", message: e.message };
  }
  return { kind: "message", message: String(e) };
}

async function call(command, args) {
  try {
    return await backend.invoke(command, args);
  } catch (e) {
    throw normaliseError(e);
  }
}

// ── Progress ────────────────────────────────────────────────────────────────

const progressHandlers = new Map();

backend.listen("progress", (event) => {
  const { task, ...progress } = event.payload;
  const handlers = progressHandlers.get(task);
  if (handlers) for (const handler of handlers) handler(progress);
});

/**
 * Listen for progress on a task ("health", "junk", "privacy", "clean", "disk",
 * "large", "duplicates", "remove"). Returns a function that stops listening.
 */
export function onProgress(task, handler) {
  if (!progressHandlers.has(task)) progressHandlers.set(task, new Set());
  progressHandlers.get(task).add(handler);
  return () => progressHandlers.get(task).delete(handler);
}

// ── Commands ────────────────────────────────────────────────────────────────

export const api = {
  appInfo: () => call("app_info"),
  appReady: () => call("app_ready"),
  setTheme: (theme) => call("set_theme", { theme }),
  relaunchAsAdmin: () => call("relaunch_as_admin"),
  cancel: (task) => call("cancel", { task }),

  systemOverview: () => call("system_overview"),
  systemLive: () => call("system_live"),

  healthCheck: (options) => call("health_check", { options }),

  cleanScan: (area, options) => call("clean_scan", { area, options }),
  cleanFiles: (id, options) => call("clean_files", { id, options }),
  cleanRun: (area, ids, options) => call("clean_run", { area, ids, options }),

  drives: () => call("drives"),
  diskScan: (path) => call("disk_scan", { path }),
  diskListing: (path) => call("disk_listing", { path }),

  largeScan: (path, minBytes) => call("large_scan", { path, minBytes }),
  largeRemove: (ids, toRecycleBin) => call("large_remove", { ids, toRecycleBin }),

  dupScan: (path, minBytes) => call("dup_scan", { path, minBytes }),
  dupRemove: (ids, toRecycleBin) => call("dup_remove", { ids, toRecycleBin }),

  startupList: () => call("startup_list"),
  startupSet: (id, enabled) => call("startup_set", { id, enabled }),

  programsList: () => call("programs_list"),
  programUninstall: (id) => call("program_uninstall", { id }),

  tasksList: () => call("tasks_list"),
  taskSet: (path, enabled) => call("task_set", { path, enabled }),

  networkList: () => call("network_list"),
  securityChecks: () => call("security_checks"),

  openKnown: (id) => call("open_known", { id }),
  reveal: (path) => call("reveal", { path }),
  pickFolder: () => call("pick_folder"),

  aiStatus: () => call("ai_status"),
  aiSetKey: (key) => call("ai_set_key", { key }),
  aiClearKey: () => call("ai_clear_key"),
  aiImportLegacy: () => call("ai_import_legacy"),
  aiSummarise: () => call("ai_summarise"),

  history: () => call("history"),
};
