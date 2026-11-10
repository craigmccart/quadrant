// A stand-in backend with made-up data, used only when the UI is opened in an
// ordinary browser for design work. It never runs inside the real app (see
// api.js). Shapes mirror crates/core/src/model.rs.

const GB = 1024 ** 3;
const MB = 1024 ** 2;
const now = () => Math.floor(Date.now() / 1000);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const listeners = new Map();
const cancelled = new Set();

export function listen(event, handler) {
  if (!listeners.has(event)) listeners.set(event, new Set());
  listeners.get(event).add(handler);
  return Promise.resolve(() => listeners.get(event).delete(handler));
}

function progress(task, message, fraction = null, items = 0, bytes = 0) {
  for (const handler of listeners.get("progress") || []) {
    handler({ payload: { task, message, fraction, items, bytes } });
  }
}

/** Play a list of status lines with a fraction, stopping if cancelled. */
async function play(task, steps, stepMs = 320) {
  cancelled.delete(task);
  for (let i = 0; i < steps.length; i++) {
    if (cancelled.has(task)) throw { kind: "cancelled", message: "Cancelled" };
    progress(task, steps[i], (i + 0.5) / steps.length, (i + 1) * 4231, (i + 1) * 310 * MB);
    await sleep(stepMs);
  }
  progress(task, "Done", 1);
}

// ── Data ────────────────────────────────────────────────────────────────────

const cat = (id, area, group, label, risk, bytes, itemCount, extra = {}) => ({
  id, area, group, label, risk, bytes, itemCount,
  description: extra.description || "Files this app or Windows can rebuild when it needs them.",
  note: extra.note || "Nothing of yours is lost.",
  needsAdmin: Boolean(extra.needsAdmin),
  defaultSelected: risk === "safe",
  toRecycleBin: Boolean(extra.toRecycleBin),
  canList: extra.canList !== false,
});

const JUNK = [
  cat("update-cache", "junk", "Windows", "Windows Update downloads", "safe", 2.31 * GB, 412, {
    description: "Update files that Windows has already downloaded and installed.",
    note: "Windows downloads these again if it needs them.", needsAdmin: true }),
  cat("chrome-cache", "junk", "Browsers", "Google Chrome cache", "safe", 1.12 * GB, 18233, {
    description: "Copies of web pages and images that Chrome keeps to load sites faster.",
    note: "Close Chrome first for a full clean. Sites may load a little slower once." }),
  cat("user-temp", "junk", "Windows", "Temporary files", "safe", 846 * MB, 3907, {
    description: "Files that apps and installers leave behind in your temporary folder.",
    note: "Recent files are kept in case a running app or installer still needs them." }),
  cat("edge-cache", "junk", "Browsers", "Microsoft Edge cache", "safe", 512 * MB, 9120, {
    description: "Copies of web pages and images that Edge keeps to load sites faster.",
    note: "Close Edge first for a full clean." }),
  cat("crash-dumps", "junk", "Windows", "Crash dumps and error reports", "safe", 188 * MB, 46, {
    description: "Diagnostic files written when an app or Windows crashed.",
    note: "Only useful if you are still investigating a crash." }),
  cat("thumbnails", "junk", "Windows", "Thumbnail and icon cache", "safe", 96 * MB, 14, {
    description: "Small previews of your pictures, videos and icons.",
    note: "Windows rebuilds these as you browse your folders." }),
  cat("windows-temp", "junk", "Windows", "Windows temporary files", "safe", 61 * MB, 388, {
    description: "Temporary files that Windows and system services leave behind.",
    note: "Recent files are kept in case an update still needs them.", needsAdmin: true }),
  cat("old-logs", "junk", "Windows", "Old Windows logs", "safe", 22 * MB, 131, {
    description: "Windows log files that have not changed for 90 days.",
    note: "Only of use when diagnosing an old problem.", needsAdmin: true }),
  cat("firefox-cache", "junk", "Browsers", "Firefox cache", "safe", 0, 0),
  cat("old-downloads", "junk", "Downloads", "Large old downloads", "review", 6.4 * GB, 7, {
    description: "Files over 100 MB in Downloads that have not changed for 90 days.",
    note: "These go to the Recycle Bin, so you can get them back.", toRecycleBin: true }),
  cat("recycle-bin", "junk", "Windows", "Recycle Bin", "review", 3.9 * GB, 218, {
    description: "Files you have already deleted, waiting to be removed for good.",
    note: "Once emptied, these cannot be restored.", canList: false }),
  cat("npm-cache", "junk", "Developer", "npm cache", "review", 1.8 * GB, 40112, {
    description: "Packages npm has downloaded before.",
    note: "npm downloads them again when a project needs them, which takes time." }),
  cat("nuget-cache", "junk", "Developer", "NuGet package cache", "review", 920 * MB, 11876, {
    description: "Packages NuGet has downloaded before.",
    note: "Restored on the next build, which takes time." }),
  cat("prefetch", "junk", "Windows", "Prefetch data", "review", 14 * MB, 251, {
    description: "Notes Windows keeps to start your apps faster.",
    note: "Apps may start a little slower until Windows rebuilds them.", needsAdmin: true }),
];

const PRIVACY = [
  cat("recent-files", "privacy", "Explorer", "Recent files list", "safe", 412 * 1024, 148, {
    description: "Shortcuts to files you opened recently, shown in File Explorer.",
    note: "The files themselves are not touched." }),
  cat("jump-lists", "privacy", "Explorer", "Jump lists", "review", 2.2 * MB, 86, {
    description: "Recent items shown when you right-click an app on the taskbar.",
    note: "This also clears items you pinned to jump lists." }),
  cat("typed-paths", "privacy", "Explorer", "File Explorer address bar history", "safe", 0, 12, {
    description: "Folder paths you typed into the File Explorer address bar.",
    note: "Nothing else changes.", canList: false }),
  cat("run-history", "privacy", "Explorer", "Run dialog history", "safe", 0, 9, {
    description: "Commands you typed into the Run box.", note: "Nothing else changes.", canList: false }),
  cat("search-history", "privacy", "Explorer", "File Explorer search history", "safe", 0, 23, {
    description: "Searches you typed into File Explorer.", note: "Nothing else changes.", canList: false }),
];

function scanOf(area, list) {
  const categories = [...list].sort((a, b) => b.bytes - a.bytes);
  return {
    area, categories,
    totalBytes: categories.reduce((n, c) => n + c.bytes, 0),
    safeBytes: categories.filter((c) => c.risk === "safe").reduce((n, c) => n + c.bytes, 0),
    totalItems: categories.reduce((n, c) => n + c.itemCount, 0),
    scannedAt: now(), elapsedMs: 4200,
  };
}

const DRIVES = [
  { mount: "C:\\", label: "Windows", fileSystem: "NTFS", kind: "SSD", totalBytes: 1863 * GB, freeBytes: 251 * GB },
  { mount: "D:\\", label: "Games", fileSystem: "NTFS", kind: "SSD", totalBytes: 931 * GB, freeBytes: 402 * GB },
  { mount: "E:\\", label: "Backup", fileSystem: "exFAT", kind: "Removable", totalBytes: 238 * GB, freeBytes: 17 * GB },
];

const OVERVIEW = {
  hostName: "FORGE", userName: "craig", osName: "Windows 11 Pro", osVersion: "25H2", osBuild: "26300.4120",
  manufacturer: "Gigabyte Technology Co., Ltd.", model: "B650 AORUS ELITE AX",
  cpuName: "AMD Ryzen 7 7700X 8-Core Processor", cpuCoresPhysical: 8, cpuCoresLogical: 16,
  gpus: ["NVIDIA GeForce RTX 4070 SUPER"], ramTotalBytes: 31.2 * GB, ramUsedBytes: 18.4 * GB,
  uptimeSecs: 3 * 86400 + 4 * 3600, bootTime: now() - 3 * 86400, isElevated: false, drives: DRIVES,
};

const startupItem = (name, command, scope, enabled, source = "registryRun") => ({
  id: `${source}|${scope}|64|${name}`, name, command,
  target: command.replace(/^"([^"]+)".*$/, "$1"), source, scope,
  location: source === "startupFolder" ? "Startup folder, this user" : scope === "user" ? "Registry, this user" : "Registry, all users",
  enabled, canToggle: source !== "registryRunOnce", needsAdmin: scope === "machine",
});

let STARTUP = [
  startupItem("Discord", '"C:\\Users\\craig\\AppData\\Local\\Discord\\Update.exe" --processStart Discord.exe', "user", true),
  startupItem("Docker Desktop", '"C:\\Program Files\\Docker\\Docker\\Docker Desktop.exe" -Autostart', "user", true),
  startupItem("Microsoft Edge Update", '"C:\\Program Files (x86)\\Microsoft\\EdgeUpdate\\MicrosoftEdgeUpdate.exe" /c', "machine", true),
  startupItem("NVIDIA Broadcast", '"C:\\Program Files\\NVIDIA Corporation\\NVIDIA Broadcast\\NVIDIA Broadcast UI.exe"', "machine", true),
  startupItem("OneDrive", '"C:\\Program Files\\Microsoft OneDrive\\OneDrive.exe" /background', "user", true),
  startupItem("Obsidian", "C:\\Users\\craig\\AppData\\Roaming\\Microsoft\\Windows\\Start Menu\\Programs\\Startup\\Obsidian.lnk", "user", true, "startupFolder"),
  startupItem("SecurityHealth", "%windir%\\system32\\SecurityHealthSystray.exe", "machine", true),
  startupItem("Spotify", '"C:\\Users\\craig\\AppData\\Roaming\\Spotify\\Spotify.exe" --autostart --minimized', "user", false),
  startupItem("Steam", '"C:\\Program Files (x86)\\Steam\\steam.exe" -silent', "user", true),
  startupItem("Tailscale", '"C:\\Program Files\\Tailscale\\tailscale-ipn.exe"', "machine", true),
  startupItem("Teams", '"C:\\Users\\craig\\AppData\\Local\\Microsoft\\Teams\\Update.exe" --processStart Teams.exe', "user", false),
];

const check = (id, label, status, summary, detail, fixPage = null) => ({ id, label, status, summary, detail, fixPage });
const SECURITY = [
  check("antivirus", "Antivirus", "good", "Microsoft Defender Antivirus is on and up to date", "Real-time protection stops known malware before it can run.", "windows-security"),
  check("firewall", "Firewall", "good", "On for all networks", "The firewall blocks unexpected connections from other devices.", "firewall"),
  check("uac", "User Account Control", "good", "Asks before apps make changes", "Stops apps changing system settings without you knowing.", "uac"),
  check("secure-boot", "Secure Boot", "good", "On", "Stops tampered startup software loading before Windows. This is a firmware setting."),
  check("drive-encryption", "Drive encryption", "attention", "Off for the system drive", "Without encryption, anyone with the PC or its drive can read your files.", "device-encryption"),
  check("remote-desktop", "Remote Desktop", "good", "Off", "Nobody can sign in to this PC over the network.", "remote-desktop"),
  check("smb1", "Old file sharing (SMB 1)", "good", "Off", "SMB 1 is an outdated protocol with known serious flaws."),
  check("restart-pending", "Pending restart", "attention", "Updates are waiting for a restart", "Security updates only take full effect after a restart.", "windows-update"),
];

function healthReport() {
  const junk = scanOf("junk", JUNK);
  const privacy = scanOf("privacy", PRIVACY);
  const quick = junk.categories.filter((c) => c.risk === "safe" && c.bytes > 0);
  return {
    score: 78, status: "attention", summary: "Two areas could do with attention.",
    pillars: [
      { id: "space", label: "Space", score: 71, status: "attention", headline: "5.1 GB", caption: "of junk that is safe to clear",
        facts: [{ label: "Safe to clear now", value: "5.1 GB" }, { label: "Worth a look first", value: "13.0 GB" }, { label: "Free on C:", value: "251.0 GB (13%)" }],
        page: "junk", actionLabel: "Review junk" },
      { id: "privacy", label: "Privacy", score: 92, status: "good", headline: "278", caption: "traces of recent activity",
        facts: [{ label: "Recent files list", value: "148" }, { label: "Jump lists", value: "86" }, { label: "File Explorer search history", value: "23" }],
        page: "privacy", actionLabel: "Review traces" },
      { id: "speed", label: "Speed", score: 88, status: "good", headline: "9", caption: "apps start with Windows",
        facts: [{ label: "Switched off", value: "2" }, { label: "Memory in use", value: "59%" }, { label: "Since last restart", value: "3 days 4 hours" }],
        page: "startup", actionLabel: "Review startup" },
      { id: "security", label: "Security", score: 80, status: "good", headline: "6 of 8", caption: "checks passed",
        facts: [{ label: "Drive encryption", value: "Off for the system drive" }, { label: "Pending restart", value: "Updates are waiting for a restart" }],
        page: "security", actionLabel: "Review security" },
    ],
    scannedAt: now(), elapsedMs: 6100, junk, privacy, startup: STARTUP, security: SECURITY,
    quickCleanIds: quick.map((c) => c.id), quickCleanBytes: quick.reduce((n, c) => n + c.bytes, 0),
  };
}

const KINDS = ["Video", "Images", "Archives", "Apps", "Documents", "Code", "Audio", "System", "Other"];
const NAMES = ["ubuntu-24.04-desktop-amd64.iso", "Holiday 2025 raw footage.mkv", "win11_25h2_english_x64.iso", "node_modules.zip",
  "backup-2026-03-01.7z", "Baldurs Gate 3 saves.rar", "training-recording.mp4", "sdxl_base_1.0.safetensors", "DockerDesktop.vhdx",
  "family-photos-2019-2024.zip", "obs-capture-0412.mkv", "pagefile.sys", "kali-linux-2026.1-installer.iso", "project-archive.tar.gz"];

function foundFile(id, i) {
  const name = NAMES[i % NAMES.length];
  const dir = i % 3 === 0 ? "C:\\Users\\craig\\Downloads" : i % 3 === 1 ? "D:\\Media\\Captures" : "C:\\Users\\craig\\Documents\\Archive";
  const ext = name.split(".").pop();
  const kind = { iso: "Archives", mkv: "Video", mp4: "Video", zip: "Archives", "7z": "Archives", rar: "Archives", gz: "Archives", vhdx: "Archives", sys: "System" }[ext] || "Other";
  const isProtected = name === "pagefile.sys";
  return { id, path: isProtected ? "C:\\pagefile.sys" : `${dir}\\${name}`, name, dir: isProtected ? "C:\\" : dir,
    bytes: Math.round((14 - (i % 14)) * 0.83 * GB + (i * 37 % 900) * MB), modified: now() - (i + 2) * 86400 * 11, kind, protected: isProtected };
}

let LARGE = Array.from({ length: 46 }, (_, i) => foundFile(i + 1, i)).sort((a, b) => b.bytes - a.bytes);

function dupGroups() {
  let id = 1;
  return Array.from({ length: 28 }, (_, g) => {
    const copies = 2 + (g % 3);
    const bytes = Math.round((30 - g) * 61 * MB + 3 * MB);
    const base = NAMES[(g * 3) % NAMES.length];
    const files = Array.from({ length: copies }, (_, c) => ({
      id: id++, name: c === 0 ? base : base.replace(/(\.[^.]+)$/, ` (${c})$1`),
      dir: c === 0 ? "C:\\Users\\craig\\Documents\\Archive" : c === 1 ? "C:\\Users\\craig\\Downloads" : "D:\\Old laptop\\Desktop",
      path: "", bytes, modified: now() - (400 - c * 120 + g) * 86400, kind: "Archives", protected: false,
    })).map((f) => ({ ...f, path: `${f.dir}\\${f.name}` }));
    return { id: g + 1, bytes, hash: (0x9e3779b9 * (g + 7) >>> 0).toString(16).padStart(8, "0") + "c41d77ae", files, wastedBytes: bytes * (copies - 1) };
  });
}
let DUPS = dupGroups();

const PROGRAMS = [
  ["7-Zip 24.09 (x64)", "Igor Pavlov", "24.09", 5.6 * MB, "2026-01-12"], ["Adobe Acrobat (64-bit)", "Adobe", "25.001.20813", 1.4 * GB, "2026-09-30"],
  ["Brave", "Brave Software Inc", "1.84.132", 820 * MB, "2026-11-02"], ["Discord", "Discord Inc.", "1.0.9214", 392 * MB, "2025-06-18"],
  ["Docker Desktop", "Docker Inc.", "4.52.0", 2.9 * GB, "2026-10-21"], ["Git", "The Git Development Community", "2.51.2", 368 * MB, "2026-10-04"],
  ["Google Chrome", "Google LLC", "142.0.7444.60", 1.1 * GB, "2026-11-06"], ["Microsoft Visual Studio Code (User)", "Microsoft Corporation", "1.105.1", 430 * MB, "2026-11-01"],
  ["Mozilla Firefox (x64 en-GB)", "Mozilla", "144.0.2", 260 * MB, "2026-10-28"], ["Node.js", "Node.js Foundation", "22.20.0", 96 * MB, "2026-09-25"],
  ["NVIDIA Graphics Driver 581.42", "NVIDIA Corporation", "581.42", null, "2026-10-14"], ["Obsidian", "Obsidian", "1.10.3", 310 * MB, "2026-10-30"],
  ["Python 3.13.7 (64-bit)", "Python Software Foundation", "3.13.7150.0", 148 * MB, "2026-08-22"], ["Rustup: the Rust toolchain installer", "The Rust Project Developers", "1.29.0", null, "2026-03-09"],
  ["Spotify", "Spotify AB", "1.2.78.418", 340 * MB, "2026-11-05"], ["Steam", "Valve Corporation", "2.10.91.91", 1.2 * GB, "2024-12-01"],
  ["Tailscale", "Tailscale Inc.", "1.90.6", 78 * MB, "2026-10-19"], ["VLC media player", "VideoLAN", "3.0.21", 172 * MB, null],
  ["Windows Subsystem for Linux", "Microsoft Corporation", "2.6.1.0", 620 * MB, "2026-09-11"], ["WinRAR 7.13 (64-bit)", "win.rar GmbH", "7.13.0", 9 * MB, "2026-07-02"],
].map(([name, publisher, version, sizeBytes, installedOn], i) => ({ id: `hklm64|app${i}`, name, publisher, version, sizeBytes: sizeBytes && Math.round(sizeBytes), installedOn,
  installLocation: `C:\\Program Files\\${name.split(" ")[0]}`, scope: i % 5 === 0 ? "user" : "machine", canUninstall: i !== 10 }));

let TASKS = [
  ["\\Adobe Acrobat Update Task", "Ready", true, "11/11/2026 18:00:00", "10/11/2026 18:00:02", "0", "Adobe Systems Incorporated", "C:\\Program Files (x86)\\Common Files\\Adobe\\ARM\\1.0\\AdobeARM.exe"],
  ["\\BraveSoftwareUpdateTaskMachineCore", "Ready", true, "11/11/2026 09:14:00", "10/11/2026 09:14:01", "0", "Brave Software Inc", "C:\\Program Files (x86)\\BraveSoftware\\Update\\BraveUpdate.exe /c"],
  ["\\GoogleUpdater\\GoogleUpdaterTaskSystem142", "Ready", true, "10/11/2026 16:02:00", "10/11/2026 15:02:00", "0", "Google LLC", "C:\\Program Files (x86)\\Google\\GoogleUpdater\\142.0\\updater.exe --wake"],
  ["\\MicrosoftEdgeUpdateTaskMachineUA", "Ready", true, "10/11/2026 15:44:00", "10/11/2026 14:44:00", "0", "Microsoft Corporation", "C:\\Program Files (x86)\\Microsoft\\EdgeUpdate\\MicrosoftEdgeUpdate.exe /ua"],
  ["\\NVIDIA App SelfUpdate", "Disabled", false, "N/A", "02/10/2026 11:20:11", "0", "NVIDIA Corporation", "C:\\Program Files\\NVIDIA Corporation\\NVIDIA App\\CEF\\NVIDIA App.exe"],
  ["\\OneDrive Reporting Task", "Ready", true, "11/11/2026 10:31:00", "10/11/2026 10:31:04", "0", "Microsoft Corporation", "%localappdata%\\Microsoft\\OneDrive\\OneDriveStandaloneUpdater.exe /reporting"],
  ["\\Opera scheduled Autoupdate", "Ready", true, "11/11/2026 12:00:00", "N/A", "267011", "", "C:\\Users\\craig\\AppData\\Local\\Programs\\Opera\\launcher.exe --scheduledautoupdate"],
].map(([path, status, enabled, nextRun, lastRun, lastResult, author, action]) => {
  const i = path.lastIndexOf("\\");
  return { path, name: path.slice(i + 1), folder: i === 0 ? "\\" : path.slice(0, i), status, enabled, nextRun, lastRun, lastResult, author, action };
});

const conn = (protocol, localAddress, localPort, remoteAddress, remotePort, state, pid, process) =>
  ({ protocol, localAddress, localPort, remoteAddress, remotePort, state, pid, process, loopback: remoteAddress.startsWith("127.") || remoteAddress === "::1" });
const NETWORK = [
  conn("TCP", "192.168.1.24", 51322, "140.82.113.26", 443, "Established", 14820, "chrome.exe"),
  conn("TCP", "192.168.1.24", 51340, "162.159.136.234", 443, "Established", 9912, "Discord.exe"),
  conn("TCP", "192.168.1.24", 51377, "35.186.224.24", 443, "Established", 17044, "Spotify.exe"),
  conn("TCP", "192.168.1.24", 51402, "20.199.120.151", 443, "Established", 5120, "OneDrive.exe"),
  conn("TCP", "127.0.0.1", 51290, "127.0.0.1", 51291, "Established", 14820, "chrome.exe"),
  conn("TCP", "192.168.1.24", 51411, "104.18.32.47", 443, "Established", 21108, "Code.exe"),
  conn("TCP", "100.84.12.7", 51233, "100.101.44.9", 22, "Established", 7732, "ssh.exe"),
  conn("TCP", "0.0.0.0", 135, "", 0, "Listening", 1284, "svchost.exe"),
  conn("TCP", "0.0.0.0", 445, "", 0, "Listening", 4, "System"),
  conn("TCP", "0.0.0.0", 5040, "", 0, "Listening", 8104, "svchost.exe"),
  conn("TCP", "127.0.0.1", 2375, "", 0, "Listening", 6620, "com.docker.backend.exe"),
  conn("TCP", "::", 7680, "", 0, "Listening", 3352, "svchost.exe"),
  conn("TCP", "192.168.1.24", 51199, "52.96.165.18", 443, "Time wait", 0, ""),
  conn("UDP", "0.0.0.0", 5353, "", 0, "", 14820, "chrome.exe"),
  conn("UDP", "0.0.0.0", 41641, "", 0, "", 4388, "tailscaled.exe"),
  conn("UDP", "192.168.1.24", 1900, "", 0, "", 3020, "svchost.exe"),
];

let HISTORY = [
  { at: now() - 6 * 86400, kind: "junk", freedBytes: 3.2 * GB, removed: 21840, skipped: 14,
    items: [{ id: "chrome-cache", label: "Google Chrome cache", freedBytes: 1.9 * GB, removed: 17002, skipped: 3 },
      { id: "user-temp", label: "Temporary files", freedBytes: 1.3 * GB, removed: 4838, skipped: 11 }] },
  { at: now() - 19 * 86400, kind: "duplicates", freedBytes: 8.7 * GB, removed: 31, skipped: 0,
    items: [{ id: "duplicates", label: "Duplicate files", freedBytes: 8.7 * GB, removed: 31, skipped: 0 }] },
  { at: now() - 41 * 86400, kind: "privacy", freedBytes: 2.1 * MB, removed: 204, skipped: 0,
    items: [{ id: "recent-files", label: "Recent files list", freedBytes: 2.1 * MB, removed: 204, skipped: 0 }] },
];

let aiKey = "";
let elevated = false;

// A small made-up folder tree for the disk map.
const TREE = {
  "C:\\": [["Users", 612], ["Program Files", 188], ["Windows", 41], ["ProgramData", 23], ["Program Files (x86)", 19], ["$Recycle.Bin", 3.9], ["Recovery", 0.9]],
  "C:\\Users": [["craig", 598], ["Public", 13], ["Default", 0.3]],
  "C:\\Users\\craig": [["Videos", 211], ["AppData", 142], ["Documents", 96], ["Downloads", 71], ["Pictures", 44], ["source", 28], ["OneDrive", 5.2], ["Desktop", 0.8]],
  "C:\\Users\\craig\\Videos": [["Captures", 148], ["Projects", 51], ["Exports", 12]],
};

function listing(path) {
  const key = path.replace(/\\$/, "") === "C:" ? "C:\\" : path.replace(/\\$/, "");
  const children = TREE[key] || [["Cache", 4.2], ["Data", 2.9], ["Logs", 0.4]];
  const total = children.reduce((n, [, gb]) => n + gb, 0) + 0.6;
  const entries = children.map(([name, gb]) => ({
    name, path: key.endsWith("\\") ? key + name : `${key}\\${name}`, bytes: Math.round(gb * GB),
    fileCount: Math.round(gb * 913), isDir: true, percent: (gb / total) * 100, hasChildren: true, denied: name === "Recovery",
  }));
  entries.push({ name: "Files in this folder", path: "", bytes: Math.round(0.6 * GB), fileCount: 37, isDir: false, percent: (0.6 / total) * 100, hasChildren: false, denied: false });
  const parent = key === "C:\\" ? null : key.slice(0, key.lastIndexOf("\\")) || "C:\\";
  return { path: key, parent: parent === "C:" ? "C:\\" : parent, bytes: Math.round(total * GB), fileCount: Math.round(total * 913), entries };
}

// ── Commands ────────────────────────────────────────────────────────────────

const commands = {
  app_info: () => ({ version: "3.0.0", isElevated: elevated, homeDir: "C:\\Users\\craig" }),
  app_ready: () => null,
  set_theme: () => null,
  relaunch_as_admin: async () => { await sleep(400); elevated = true; location.reload(); },
  cancel: ({ task }) => { cancelled.add(task); },

  system_overview: async () => { await sleep(200); return { ...OVERVIEW, isElevated: elevated }; },
  system_live: () => ({ cpuPercent: 8 + Math.random() * 30, ramUsedBytes: (17.6 + Math.random() * 1.6) * GB, ramTotalBytes: 31.2 * GB }),

  health_check: async () => {
    await play("health", ["Measuring junk files", "Scanning Chrome cache", "Scanning Windows Update downloads", "Scanning Downloads",
      "Looking for traces of recent activity", "Checking what starts with Windows", "Checking security settings", "Reading system details"], 380);
    return healthReport();
  },

  clean_scan: async ({ area }) => {
    const list = area === "junk" ? JUNK : PRIVACY;
    await play(area, list.map((c) => `Measuring ${c.label}`), 150);
    return scanOf(area, list);
  },
  clean_files: async ({ id }) => {
    await sleep(350);
    const c = [...JUNK, ...PRIVACY].find((x) => x.id === id);
    const n = Math.min(c.itemCount, 500);
    const files = Array.from({ length: Math.min(n, 60) }, (_, i) => ({
      path: `C:\\Users\\craig\\AppData\\Local\\${c.label.replace(/ /g, "")}\\${(i * 7919 + 13).toString(16)}\\data_${i}.bin`,
      bytes: Math.round(c.bytes / (i + 2) / 3), modified: now() - (i + 2) * 86400,
    }));
    return { id, files, totalItems: c.itemCount, truncated: c.itemCount > files.length };
  },
  clean_run: async ({ area, ids }) => {
    const list = area === "junk" ? JUNK : PRIVACY;
    const chosen = list.filter((c) => ids.includes(c.id));
    await play("clean", chosen.map((c) => `Clearing ${c.label}`), 420);
    const items = chosen.map((c) => {
      const skipped = Math.round(c.itemCount * 0.004);
      const freed = Math.round(c.bytes * 0.97);
      const result = { id: c.id, label: c.label, freedBytes: freed, removed: c.itemCount - skipped, skipped };
      c.bytes -= freed; c.itemCount = skipped;
      return result;
    });
    const report = { area, items, freedBytes: items.reduce((n, i) => n + i.freedBytes, 0), removed: items.reduce((n, i) => n + i.removed, 0),
      skipped: items.reduce((n, i) => n + i.skipped, 0), finishedAt: now(), elapsedMs: 2600 };
    HISTORY = [{ at: now(), kind: area, freedBytes: report.freedBytes, removed: report.removed, skipped: report.skipped, items }, ...HISTORY];
    return report;
  },

  drives: () => DRIVES,
  disk_scan: async ({ path }) => {
    cancelled.delete("disk");
    for (let i = 1; i <= 14; i++) {
      if (cancelled.has("disk")) throw { kind: "cancelled", message: "Cancelled" };
      progress("disk", `Scanning ${path}`, null, i * 96412, i * 63 * GB);
      await sleep(170);
    }
    const sizes = [212, 186, 151, 98, 71, 64, 52, 31, 22];
    return { root: path, bytes: 887 * GB, fileCount: 1349768, dirCount: 214530,
      byType: KINDS.map((kind, i) => ({ kind, bytes: sizes[i] * GB, fileCount: 40000 + i * 9000 })), scannedAt: now(), elapsedMs: 41200 };
  },
  disk_listing: async ({ path }) => { await sleep(60); return listing(path); },

  large_scan: async ({ path, minBytes }) => {
    await play("large", Array.from({ length: 8 }, () => `Scanning ${path}`), 200);
    LARGE = Array.from({ length: 46 }, (_, i) => foundFile(i + 1, i)).sort((a, b) => b.bytes - a.bytes);
    const files = LARGE.filter((f) => f.bytes >= minBytes);
    return { root: path, minBytes, files, totalCount: files.length, totalBytes: files.reduce((n, f) => n + f.bytes, 0), truncated: false, elapsedMs: 3300 };
  },
  large_remove: async ({ ids, toRecycleBin }) => {
    await play("remove", ids.slice(0, 6).map(() => "Removing files"), 200);
    const gone = LARGE.filter((f) => ids.includes(f.id));
    LARGE = LARGE.filter((f) => !ids.includes(f.id));
    return { removed: gone.length, failed: 0, freedBytes: gone.reduce((n, f) => n + f.bytes, 0), failures: [], toRecycleBin, removedIds: gone.map((f) => f.id) };
  },

  dup_scan: async ({ path, minBytes }) => {
    await play("duplicates", ["Listing files", "Listing files", "Comparing sizes", "Checking contents", "Checking contents", "Checking contents"], 330);
    DUPS = dupGroups();
    return { root: path, minBytes, groups: DUPS, totalGroups: DUPS.length, wastedBytes: DUPS.reduce((n, g) => n + g.wastedBytes, 0), filesChecked: 48211, truncated: false, elapsedMs: 9800 };
  },
  dup_remove: async ({ ids, toRecycleBin }) => {
    await play("remove", ids.slice(0, 6).map(() => "Removing files"), 200);
    let freed = 0;
    for (const g of DUPS) for (const f of g.files) if (ids.includes(f.id)) freed += f.bytes;
    // One file is reported as locked, so the failure view can be checked.
    const failures = ids.length > 2 ? [{ path: "D:\\Old laptop\\Desktop\\backup-2026-03-01 (2).7z", reason: "In use by another program" }] : [];
    const removedIds = failures.length ? ids.slice(0, -1) : ids;
    return { removed: removedIds.length, failed: failures.length, freedBytes: freed, failures, toRecycleBin, removedIds };
  },

  startup_list: async () => { await sleep(150); return STARTUP; },
  startup_set: async ({ id, enabled }) => {
    await sleep(200);
    const item = STARTUP.find((s) => s.id === id);
    if (item.needsAdmin && !elevated) throw { kind: "denied", message: "Access denied: changing this item needs administrator rights." };
    STARTUP = STARTUP.map((s) => (s.id === id ? { ...s, enabled } : s));
    return STARTUP;
  },

  programs_list: async () => { await sleep(250); return PROGRAMS; },
  program_uninstall: async () => { await sleep(500); return null; },

  tasks_list: async () => { await sleep(600); return TASKS; },
  task_set: async ({ path, enabled }) => {
    await sleep(300);
    TASKS = TASKS.map((t) => (t.path === path ? { ...t, enabled, status: enabled ? "Ready" : "Disabled" } : t));
    return TASKS;
  },

  network_list: async () => { await sleep(200); return NETWORK; },
  security_checks: async () => { await sleep(900); return SECURITY; },

  open_known: () => null,
  reveal: () => null,
  pick_folder: async () => { await sleep(200); return "D:\\Media"; },

  ai_status: () => ({ hasKey: Boolean(aiKey), keyHint: aiKey.slice(-4), model: "gemini-3.1-flash-lite-preview", legacyKeyFile: !aiKey }),
  ai_set_key: ({ key }) => { aiKey = key; return commands.ai_status(); },
  ai_clear_key: () => { aiKey = ""; return commands.ai_status(); },
  ai_import_legacy: () => { aiKey = "AIzaSyExampleKey9f3Q"; return commands.ai_status(); },
  ai_summarise: async () => {
    await sleep(1400);
    if (!aiKey) throw { kind: "message", message: "Add a Gemini API key in Settings first." };
    return "You can free about 5.1 GB straight away, and up to 18 GB if you go through the review items.\n\nSafe to clear now: Windows Update downloads (2.3 GB), the Chrome and Edge caches (1.6 GB together), temporary files and crash dumps. Windows and your browsers rebuild all of these on their own.\n\nWorth a look first: Large old downloads (6.4 GB) are your own files, so check the list before sending them to the Recycle Bin. The Recycle Bin itself holds 3.9 GB, which is gone for good once emptied. The npm and NuGet caches are safe but your next build will be slower while they refill.\n\nSuggested order: clear the safe items, restart your browsers, then review Downloads and empty the Recycle Bin once you are sure.";
  },

  history: () => ({ entries: HISTORY, totalFreedBytes: HISTORY.reduce((n, e) => n + e.freedBytes, 0), totalRemoved: HISTORY.reduce((n, e) => n + e.removed, 0) }),
};

export async function invoke(command, args = {}) {
  const handler = commands[command];
  if (!handler) throw { kind: "message", message: `Unknown command ${command}` };
  return structuredClone(await handler(args));
}
