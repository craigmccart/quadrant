//! The cleaner: junk files and privacy traces.
//!
//! Categories are a fixed allowlist defined in this file. The UI only ever
//! sends category ids back, never paths.
//!
//! How a clean stays inside its lines:
//! - Every category root is built from a fixed template and checked by
//!   `root_is_safe` first. A root that is too shallow, or that is (or holds)
//!   a key folder such as the user profile, makes the category disappear.
//! - Walking never enters a reparse point. A link found inside a root is
//!   removed as a link, never what it points to.
//! - The root itself is never removed, only what is inside it.
//! - Each entry is opened without following links, its real location and age
//!   are checked through that same handle, and only then is it deleted.

use std::collections::HashSet;
use std::fs;
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::Instant;

use windows::core::PCWSTR;
use windows::Win32::UI::Shell::{
    SHEmptyRecycleBinW, SHQueryRecycleBinW, SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI,
    SHERB_NOSOUND, SHQUERYRBINFO,
};
use winreg::enums::{HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE};
use winreg::RegKey;

use crate::fsops::{
    delete_checked, is_dir_attr, is_reparse, key_starts_with, key_strictly_inside,
    local_bytes, path_key, HandleInfo,
};
use crate::model::{
    Area, CategoryFiles, CleanCategory, CleanItemResult, CleanOptions, CleanReport, CleanScan,
    FileEntry, Risk,
};
use crate::util::{now_unix, to_unix};
use crate::{Ctx, Error, Result};

// ── The allowlist ────────────────────────────────────────────────────────────

/// What a root template is based on, which sets how deep it must be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Base {
    /// Under the user profile: at least three folders below the drive.
    User,
    /// `%TEMP%`: like `User`, but `D:\Temp` style folders are allowed too.
    Temp,
    /// Under the Windows folder or ProgramData: at least two folders deep.
    Machine,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shape {
    /// Everything inside the root, sub-folders included.
    Contents,
    /// Matching files directly inside the root.
    Top,
    /// Matching files anywhere under the root. Folders are left in place.
    Tree,
    /// One named file. The template is the file itself.
    File,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Match {
    Any,
    Ext(&'static [&'static str]),
    /// `thumbcache_*.db` and `iconcache_*.db`.
    ExplorerCache,
    /// Anything whose extension is not in `KEEP_EXTENSIONS`.
    NotKept,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Age {
    None,
    /// `CleanOptions::temp_min_age_hours`.
    Temp,
    Days(u32),
}

#[derive(Clone, Copy, Debug)]
struct Spec {
    template: &'static str,
    base: Base,
    shape: Shape,
    matcher: Match,
    age: Age,
    min_bytes: u64,
}

const fn contents(template: &'static str, base: Base) -> Spec {
    Spec { template, base, shape: Shape::Contents, matcher: Match::Any, age: Age::None, min_bytes: 0 }
}

const fn top(template: &'static str, base: Base, matcher: Match) -> Spec {
    Spec { template, base, shape: Shape::Top, matcher, age: Age::None, min_bytes: 0 }
}

#[derive(Clone, Copy, Debug)]
enum Source {
    Paths(&'static [Spec]),
    /// `%TEMP%`, falling back to `%LOCALAPPDATA%\Temp` when it is not set.
    UserTemp,
    /// A Chromium "User Data" folder.
    Chromium(&'static str),
    Firefox,
    RecycleBin,
    Registry(&'static str),
}

struct Def {
    id: &'static str,
    area: Area,
    group: &'static str,
    label: &'static str,
    description: &'static str,
    note: &'static str,
    risk: Risk,
    default_selected: bool,
    needs_admin: bool,
    to_recycle_bin: bool,
    source: Source,
}

impl Def {
    fn can_list(&self) -> bool {
        !matches!(self.source, Source::RecycleBin | Source::Registry(_))
    }
}

/// Files in Downloads with these extensions are never offered for removal:
/// photos, video, audio, documents and source code. Ported from the old
/// `SAFE_EXTENSIONS`.
const KEEP_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "bmp", "tiff", "tif", "heic", "heif", "raw", "cr2", "nef", "arw",
    "dng", "webp", "svg", "mp4", "mov", "avi", "mkv", "wmv", "flv", "m4v", "mpg", "mpeg", "mp3",
    "wav", "flac", "aac", "ogg", "wma", "m4a", "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx",
    "odt", "ods", "odp", "rtf", "txt", "md", "py", "js", "ts", "cs", "cpp", "c", "h", "java", "go",
    "rs", "rb", "php", "swift", "kt",
];

const REG_TYPED_PATHS: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\TypedPaths";
const REG_RUN_MRU: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\RunMRU";
const REG_WORD_WHEEL: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\WordWheelQuery";

/// The only registry keys the cleaner may touch. Values under them are
/// deleted; the keys themselves are always kept.
const REGISTRY_KEYS: [&str; 3] = [REG_TYPED_PATHS, REG_RUN_MRU, REG_WORD_WHEEL];

const MB: u64 = 1024 * 1024;

const CRASH_SPECS: &[Spec] = &[
    contents(r"%LOCALAPPDATA%\CrashDumps", Base::User),
    contents(r"%LOCALAPPDATA%\Microsoft\Windows\WER\ReportArchive", Base::User),
    contents(r"%LOCALAPPDATA%\Microsoft\Windows\WER\ReportQueue", Base::User),
    contents(r"%ProgramData%\Microsoft\Windows\WER\ReportArchive", Base::Machine),
    contents(r"%ProgramData%\Microsoft\Windows\WER\ReportQueue", Base::Machine),
    contents(r"%SystemRoot%\Minidump", Base::Machine),
    Spec {
        template: r"%SystemRoot%\MEMORY.DMP",
        base: Base::Machine,
        shape: Shape::File,
        matcher: Match::Any,
        age: Age::None,
        min_bytes: 0,
    },
];

const DEFS: &[Def] = &[
    // ── Junk: Windows ──
    Def {
        id: "user-temp",
        area: Area::Junk,
        group: "Windows",
        label: "Temporary files",
        description: "Files that apps and installers leave behind in your temporary folder.",
        note: "Recent files are kept in case a running app or installer still needs them.",
        risk: Risk::Safe,
        default_selected: true,
        needs_admin: false,
        to_recycle_bin: false,
        source: Source::UserTemp,
    },
    Def {
        id: "windows-temp",
        area: Area::Junk,
        group: "Windows",
        label: "Windows temporary files",
        description: "Temporary files that Windows and system services leave behind.",
        note: "Recent files are kept in case an update or installer still needs them.",
        risk: Risk::Safe,
        default_selected: true,
        needs_admin: true,
        to_recycle_bin: false,
        source: Source::Paths(&[Spec {
            template: r"%SystemRoot%\Temp",
            base: Base::Machine,
            shape: Shape::Contents,
            matcher: Match::Any,
            age: Age::Temp,
            min_bytes: 0,
        }]),
    },
    Def {
        id: "update-cache",
        area: Area::Junk,
        group: "Windows",
        label: "Windows Update downloads",
        description: "Update files that Windows has already downloaded and installed.",
        note: "Windows downloads these again if it needs them.",
        risk: Risk::Safe,
        default_selected: true,
        needs_admin: true,
        to_recycle_bin: false,
        source: Source::Paths(&[contents(r"%SystemRoot%\SoftwareDistribution\Download", Base::Machine)]),
    },
    Def {
        id: "thumbnails",
        area: Area::Junk,
        group: "Windows",
        label: "Thumbnail and icon cache",
        description: "Small preview pictures and icons that File Explorer saves so folders open faster.",
        note: "Windows rebuilds them as you browse, so some folders may open a little slower at first.",
        risk: Risk::Safe,
        default_selected: true,
        needs_admin: false,
        to_recycle_bin: false,
        source: Source::Paths(&[top(
            r"%LOCALAPPDATA%\Microsoft\Windows\Explorer",
            Base::User,
            Match::ExplorerCache,
        )]),
    },
    Def {
        id: "crash-dumps",
        area: Area::Junk,
        group: "Windows",
        label: "Crash dumps and error reports",
        description: "Reports and memory dumps saved when an app or Windows stopped working.",
        note: "You will not be able to send these reports to Microsoft or a developer afterwards.",
        risk: Risk::Safe,
        default_selected: true,
        needs_admin: false,
        to_recycle_bin: false,
        source: Source::Paths(CRASH_SPECS),
    },
    Def {
        id: "old-logs",
        area: Area::Junk,
        group: "Windows",
        label: "Old Windows logs",
        description: "Windows log files that have not changed for 90 days.",
        note: "Newer logs are kept in case they are needed to track down a problem.",
        risk: Risk::Safe,
        default_selected: true,
        needs_admin: true,
        to_recycle_bin: false,
        source: Source::Paths(&[Spec {
            template: r"%SystemRoot%\Logs",
            base: Base::Machine,
            shape: Shape::Tree,
            matcher: Match::Ext(&["log", "etl"]),
            age: Age::Days(90),
            min_bytes: 0,
        }]),
    },
    Def {
        id: "prefetch",
        area: Area::Junk,
        group: "Windows",
        label: "Prefetch data",
        description: "Notes Windows keeps to start the apps you use most a little faster.",
        note: "Apps may start a little slower for a while until Windows rebuilds them.",
        risk: Risk::Review,
        default_selected: false,
        needs_admin: true,
        to_recycle_bin: false,
        source: Source::Paths(&[top(r"%SystemRoot%\Prefetch", Base::Machine, Match::Ext(&["pf"]))]),
    },
    Def {
        id: "recycle-bin",
        area: Area::Junk,
        group: "Windows",
        label: "Recycle Bin",
        description: "Files you have deleted that are still waiting in the Recycle Bin.",
        note: "These cannot be restored once the Recycle Bin is emptied.",
        risk: Risk::Review,
        default_selected: false,
        needs_admin: false,
        to_recycle_bin: false,
        source: Source::RecycleBin,
    },
    // ── Junk: browsers ──
    Def {
        id: "chrome-cache",
        area: Area::Junk,
        group: "Browsers",
        label: "Google Chrome cache",
        description: "Copies of web pages and pictures that Chrome saves to load sites faster.",
        note: "Close Chrome first for a full clean.",
        risk: Risk::Safe,
        default_selected: true,
        needs_admin: false,
        to_recycle_bin: false,
        source: Source::Chromium(r"%LOCALAPPDATA%\Google\Chrome\User Data"),
    },
    Def {
        id: "edge-cache",
        area: Area::Junk,
        group: "Browsers",
        label: "Microsoft Edge cache",
        description: "Copies of web pages and pictures that Edge saves to load sites faster.",
        note: "Close Edge first for a full clean.",
        risk: Risk::Safe,
        default_selected: true,
        needs_admin: false,
        to_recycle_bin: false,
        source: Source::Chromium(r"%LOCALAPPDATA%\Microsoft\Edge\User Data"),
    },
    Def {
        id: "brave-cache",
        area: Area::Junk,
        group: "Browsers",
        label: "Brave cache",
        description: "Copies of web pages and pictures that Brave saves to load sites faster.",
        note: "Close Brave first for a full clean.",
        risk: Risk::Safe,
        default_selected: true,
        needs_admin: false,
        to_recycle_bin: false,
        source: Source::Chromium(r"%LOCALAPPDATA%\BraveSoftware\Brave-Browser\User Data"),
    },
    Def {
        id: "firefox-cache",
        area: Area::Junk,
        group: "Browsers",
        label: "Firefox cache",
        description: "Copies of web pages and pictures that Firefox saves to load sites faster.",
        note: "Close Firefox first for a full clean.",
        risk: Risk::Safe,
        default_selected: true,
        needs_admin: false,
        to_recycle_bin: false,
        source: Source::Firefox,
    },
    // ── Junk: developer caches ──
    Def {
        id: "nuget-cache",
        area: Area::Junk,
        group: "Developer",
        label: "NuGet package cache",
        description: "Packages downloaded for .NET projects and shared between them.",
        note: "Your next build downloads the packages it needs again, which can take a while.",
        risk: Risk::Review,
        default_selected: false,
        needs_admin: false,
        to_recycle_bin: false,
        source: Source::Paths(&[contents(r"%USERPROFILE%\.nuget\packages", Base::User)]),
    },
    Def {
        id: "npm-cache",
        area: Area::Junk,
        group: "Developer",
        label: "npm cache",
        description: "Packages npm has downloaded for JavaScript projects.",
        note: "npm downloads packages again the next time a project needs them.",
        risk: Risk::Review,
        default_selected: false,
        needs_admin: false,
        to_recycle_bin: false,
        source: Source::Paths(&[
            contents(r"%LOCALAPPDATA%\npm-cache", Base::User),
            contents(r"%APPDATA%\npm-cache", Base::User),
        ]),
    },
    Def {
        id: "pip-cache",
        area: Area::Junk,
        group: "Developer",
        label: "pip cache",
        description: "Packages pip has downloaded for Python projects.",
        note: "pip downloads packages again the next time you install them.",
        risk: Risk::Review,
        default_selected: false,
        needs_admin: false,
        to_recycle_bin: false,
        source: Source::Paths(&[contents(r"%LOCALAPPDATA%\pip\Cache", Base::User)]),
    },
    // ── Junk: downloads ──
    Def {
        id: "old-downloads",
        area: Area::Junk,
        group: "Downloads",
        label: "Large old downloads",
        description: "Downloads of 100 MB or more that have not changed for 90 days, such as old installers and archives.",
        note: "Photos, videos, music, documents and code are never included, and the rest go to the Recycle Bin.",
        risk: Risk::Review,
        default_selected: false,
        needs_admin: false,
        to_recycle_bin: true,
        source: Source::Paths(&[Spec {
            template: r"%USERPROFILE%\Downloads",
            base: Base::User,
            shape: Shape::Tree,
            matcher: Match::NotKept,
            age: Age::Days(90),
            min_bytes: 100 * MB,
        }]),
    },
    // ── Privacy: Explorer ──
    Def {
        id: "recent-files",
        area: Area::Privacy,
        group: "Explorer",
        label: "Recent files list",
        description: "The list of files you opened recently, shown in File Explorer and the Start menu.",
        note: "Only the shortcuts are removed, never the files themselves.",
        risk: Risk::Safe,
        default_selected: true,
        needs_admin: false,
        to_recycle_bin: false,
        source: Source::Paths(&[top(r"%APPDATA%\Microsoft\Windows\Recent", Base::User, Match::Ext(&["lnk"]))]),
    },
    Def {
        id: "jump-lists",
        area: Area::Privacy,
        group: "Explorer",
        label: "Jump lists",
        description: "The recent items shown when you right-click an app on the taskbar or Start menu.",
        note: "This also clears items you have pinned to jump lists.",
        risk: Risk::Review,
        default_selected: false,
        needs_admin: false,
        to_recycle_bin: false,
        source: Source::Paths(&[
            top(r"%APPDATA%\Microsoft\Windows\Recent\AutomaticDestinations", Base::User, Match::Any),
            top(r"%APPDATA%\Microsoft\Windows\Recent\CustomDestinations", Base::User, Match::Any),
        ]),
    },
    Def {
        id: "typed-paths",
        area: Area::Privacy,
        group: "Explorer",
        label: "File Explorer address bar history",
        description: "Folder paths you have typed into the File Explorer address bar.",
        note: "The address bar stops suggesting places you typed before.",
        risk: Risk::Safe,
        default_selected: true,
        needs_admin: false,
        to_recycle_bin: false,
        source: Source::Registry(REG_TYPED_PATHS),
    },
    Def {
        id: "run-history",
        area: Area::Privacy,
        group: "Explorer",
        label: "Run dialog history",
        description: "Commands you have typed into the Run box.",
        note: "The Run box stops suggesting commands you typed before.",
        risk: Risk::Safe,
        default_selected: true,
        needs_admin: false,
        to_recycle_bin: false,
        source: Source::Registry(REG_RUN_MRU),
    },
    Def {
        id: "search-history",
        area: Area::Privacy,
        group: "Explorer",
        label: "File Explorer search history",
        description: "Searches you have typed into the File Explorer search box.",
        note: "The search box stops suggesting searches you typed before.",
        risk: Risk::Safe,
        default_selected: true,
        needs_admin: false,
        to_recycle_bin: false,
        source: Source::Registry(REG_WORD_WHEEL),
    },
];

fn find_def(id: &str) -> Option<&'static Def> {
    DEFS.iter().find(|d| d.id == id)
}

// ── Environment and root checks ──────────────────────────────────────────────

/// Reads environment variables. Tests pass a fake one so they never depend
/// on, or change, the real environment.
type Env<'e> = &'e dyn Fn(&str) -> Option<String>;

fn real_env(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

/// Like `util::expand_env`, but through `env`. None if any variable is
/// missing or empty.
fn expand(template: &str, env: Env) -> Option<PathBuf> {
    let mut out = String::with_capacity(template.len() + 32);
    let mut rest = template;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let end = after.find('%')?;
        let value = env(&after[..end])?;
        if value.trim().is_empty() {
            return None;
        }
        out.push_str(value.trim());
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    Some(PathBuf::from(out))
}

/// Folders a category root may never be, or contain.
const KEY_FOLDERS: &[&str] = &[
    "%SystemRoot%",
    "%windir%",
    "%USERPROFILE%",
    "%ProgramFiles%",
    "%ProgramFiles(x86)%",
    "%ProgramW6432%",
    "%ProgramData%",
    "%APPDATA%",
    "%LOCALAPPDATA%",
    "%PUBLIC%",
    r"%SystemDrive%\Users",
];

/// System folders a user-profile root must never sit inside.
const SYSTEM_FOLDERS: &[&str] = &[
    "%SystemRoot%",
    "%windir%",
    "%ProgramFiles%",
    "%ProgramFiles(x86)%",
    "%ProgramW6432%",
    "%ProgramData%",
];

/// The suspicious-root rule. A root built from environment variables must be
/// an absolute, tidy path, deep enough for its kind, and must not be (or
/// hold) any key folder. A user-profile root must also not sit inside a
/// system folder. If `%TEMP%` points at `C:\` or the profile, the category
/// simply does not appear.
fn root_is_safe(root: &Path, base: Base, env: Env) -> bool {
    let Some(key) = path_key(root) else { return false };
    let depth = key.len() - 1;
    let deep_enough = match base {
        Base::User => depth >= 3,
        Base::Machine => depth >= 2,
        Base::Temp => {
            depth >= 3 || (depth >= 1 && matches!(key[depth].as_str(), "temp" | "tmp"))
        }
    };
    if !deep_enough {
        return false;
    }
    for template in KEY_FOLDERS {
        if let Some(folder) = expand(template, env).and_then(|p| path_key(&p)) {
            if key_starts_with(&folder, &key) {
                return false;
            }
        }
    }
    if base != Base::Machine {
        for template in SYSTEM_FOLDERS {
            if let Some(folder) = expand(template, env).and_then(|p| path_key(&p)) {
                if key_starts_with(&key, &folder) {
                    return false;
                }
            }
        }
    }
    true
}

/// An existing folder that is not itself a link.
fn is_real_dir(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|m| is_dir_attr(m.file_attributes()) && !is_reparse(m.file_attributes()))
        .unwrap_or(false)
}

// ── Resolved targets ─────────────────────────────────────────────────────────

/// One root on this machine and the rule for what inside it may go.
#[derive(Clone, Debug)]
struct Target {
    root: PathBuf,
    shape: Shape,
    matcher: Match,
    /// Entries newer than this many seconds are left alone.
    min_age_secs: Option<i64>,
    min_bytes: u64,
    recycle: bool,
}

fn min_age(age: Age, opts: CleanOptions) -> Option<i64> {
    match age {
        Age::None => None,
        Age::Temp if opts.temp_min_age_hours == 0 => None,
        Age::Temp => Some(i64::from(opts.temp_min_age_hours) * 3_600),
        Age::Days(d) => Some(i64::from(d) * 86_400),
    }
}

fn spec_target(spec: &Spec, root: PathBuf, opts: CleanOptions, recycle: bool, env: Env) -> Option<Target> {
    if !root_is_safe(&root, spec.base, env) {
        return None;
    }
    let present = match spec.shape {
        Shape::File => fs::symlink_metadata(&root)
            .map(|m| !is_dir_attr(m.file_attributes()))
            .unwrap_or(false),
        _ => is_real_dir(&root),
    };
    present.then(|| Target {
        root,
        shape: spec.shape,
        matcher: spec.matcher,
        min_age_secs: min_age(spec.age, opts),
        min_bytes: spec.min_bytes,
        recycle,
    })
}

/// Real subfolders of `dir` whose names pass `keep`. Links are skipped.
fn subfolders(dir: &Path, keep: impl Fn(&str) -> bool) -> Vec<PathBuf> {
    let Ok(read) = fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<PathBuf> = read
        .flatten()
        .filter(|e| {
            e.metadata()
                .map(|m| is_dir_attr(m.file_attributes()) && !is_reparse(m.file_attributes()))
                .unwrap_or(false)
        })
        .filter(|e| keep(&e.file_name().to_string_lossy()))
        .map(|e| e.path())
        .collect();
    out.sort();
    out
}

/// The roots of a file-based category that exist on this machine and pass
/// the safety checks. Empty for registry and Recycle Bin categories.
fn targets(def: &Def, opts: CleanOptions, env: Env) -> Vec<Target> {
    let recycle = def.to_recycle_bin;
    match def.source {
        Source::Paths(specs) => specs
            .iter()
            .filter_map(|s| expand(s.template, env).and_then(|r| spec_target(s, r, opts, recycle, env)))
            .collect(),
        Source::UserTemp => {
            let spec = Spec {
                template: "",
                base: Base::Temp,
                shape: Shape::Contents,
                matcher: Match::Any,
                age: Age::Temp,
                min_bytes: 0,
            };
            // Fall back only when TEMP is missing. A TEMP that fails the
            // safety check means no category, not a different folder.
            let root = match expand("%TEMP%", env) {
                Some(t) => Some(t),
                None => expand(r"%LOCALAPPDATA%\Temp", env),
            };
            root.and_then(|r| spec_target(&spec, r, opts, recycle, env)).into_iter().collect()
        }
        Source::Chromium(user_data) => {
            let Some(base) = expand(user_data, env) else { return Vec::new() };
            if !root_is_safe(&base, Base::User, env) || !is_real_dir(&base) {
                return Vec::new();
            }
            let profiles = subfolders(&base, |n| {
                n == "Default" || n == "Guest Profile" || n.starts_with("Profile ")
            });
            let spec = contents("", Base::User);
            profiles
                .iter()
                .flat_map(|p| ["Cache", "Code Cache", "GPUCache"].map(|c| p.join(c)))
                .filter_map(|root| spec_target(&spec, root, opts, recycle, env))
                .collect()
        }
        Source::Firefox => {
            let Some(base) = expand(r"%LOCALAPPDATA%\Mozilla\Firefox\Profiles", env) else {
                return Vec::new();
            };
            if !root_is_safe(&base, Base::User, env) || !is_real_dir(&base) {
                return Vec::new();
            }
            let spec = contents("", Base::User);
            subfolders(&base, |_| true)
                .into_iter()
                .filter_map(|p| spec_target(&spec, p.join("cache2"), opts, recycle, env))
                .collect()
        }
        Source::RecycleBin | Source::Registry(_) => Vec::new(),
    }
}

// ── Walking a target ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct Item {
    path: PathBuf,
    /// 0 for links and cloud placeholders, which free no space.
    bytes: u64,
    modified: Option<i64>,
}

#[derive(Debug, Default)]
struct Collected {
    items: Vec<Item>,
    /// Real sub-folders, with their depth and age when collected. Only for
    /// `Shape::Contents`, so empty ones can be tidied away afterwards.
    dirs: Vec<(PathBuf, usize, Option<i64>)>,
}

impl Collected {
    fn bytes(&self) -> u64 {
        self.items.iter().map(|i| i.bytes).sum()
    }
}

/// The newer of created and modified. A file copied or unpacked into a temp
/// folder keeps its old modified time but gets a fresh created time, and it
/// may well be in use, so the age rules go by whichever is newer.
fn newest(created: Option<i64>, modified: Option<i64>) -> Option<i64> {
    created.max(modified)
}

fn meta_newest(meta: &fs::Metadata) -> Option<i64> {
    newest(meta.created().ok().map(to_unix), meta.modified().ok().map(to_unix))
}

/// True when the age rule allows removal. An unknown time counts as new.
fn old_enough(newest: Option<i64>, min_age_secs: Option<i64>, now: i64) -> bool {
    match min_age_secs {
        None => true,
        Some(age) => newest.is_some_and(|t| now - t >= age),
    }
}

fn name_matches(matcher: Match, name: &str) -> bool {
    let lower = name.to_lowercase();
    let ext = Path::new(&lower).extension().and_then(|e| e.to_str()).unwrap_or("").to_string();
    match matcher {
        Match::Any => true,
        Match::Ext(list) => list.contains(&ext.as_str()),
        Match::ExplorerCache => {
            (lower.starts_with("thumbcache_") || lower.starts_with("iconcache_"))
                && lower.ends_with(".db")
        }
        Match::NotKept => !KEEP_EXTENSIONS.contains(&ext.as_str()),
    }
}

/// Everything in `t` that a clean would remove right now. Never enters a
/// reparse point. Stops early, with what it has, if cancelled.
fn collect(t: &Target, now: i64, ctx: &Ctx) -> Collected {
    let mut out = Collected::default();
    if t.shape == Shape::File {
        if let Ok(meta) = fs::symlink_metadata(&t.root) {
            let attrs = meta.file_attributes();
            let bytes = if is_reparse(attrs) { 0 } else { local_bytes(attrs, meta.len()) };
            if !is_dir_attr(attrs)
                && bytes >= t.min_bytes
                && old_enough(meta_newest(&meta), t.min_age_secs, now)
            {
                out.items.push(Item {
                    path: t.root.clone(),
                    bytes,
                    modified: crate::util::modified_unix(&meta),
                });
            }
        }
        return out;
    }

    let mut stack = vec![(t.root.clone(), 0usize)];
    let mut seen = 0u32;
    while let Some((dir, depth)) = stack.pop() {
        let Ok(read) = fs::read_dir(&dir) else { continue };
        for entry in read.flatten() {
            seen = seen.wrapping_add(1);
            if seen.is_multiple_of(512) && ctx.cancelled() {
                return out;
            }
            let Ok(meta) = entry.metadata() else { continue };
            let attrs = meta.file_attributes();
            let is_dir = is_dir_attr(attrs);
            let link = is_reparse(attrs);
            let path = entry.path();

            if is_dir && !link {
                match t.shape {
                    Shape::Contents => {
                        out.dirs.push((path.clone(), depth + 1, meta_newest(&meta)));
                        stack.push((path, depth + 1));
                    }
                    Shape::Tree => stack.push((path, depth + 1)),
                    _ => {}
                }
                continue;
            }
            // A folder link is only ever removed by a Contents rule, which
            // owns everything in the root. Pattern rules leave them alone.
            if is_dir && t.shape != Shape::Contents {
                continue;
            }
            if !name_matches(t.matcher, &entry.file_name().to_string_lossy()) {
                continue;
            }
            let bytes = if link { 0 } else { local_bytes(attrs, meta.len()) };
            if bytes < t.min_bytes || !old_enough(meta_newest(&meta), t.min_age_secs, now) {
                continue;
            }
            out.items.push(Item { path, bytes, modified: crate::util::modified_unix(&meta) });
        }
    }
    out
}

// ── Removing from a target ───────────────────────────────────────────────────

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct Counts {
    removed: u64,
    freed: u64,
    skipped: u64,
}

/// Is `path` allowed to be removed from target `t`, judged on the path as
/// written? Strictly inside a folder root, or exactly the file for a File
/// target. A path with `..` never passes.
fn lexically_allowed(t: &Target, root_key: &[String], path: &Path) -> bool {
    match path_key(path) {
        None => false,
        Some(key) if t.shape == Shape::File => key == root_key,
        Some(key) => key_strictly_inside(&key, root_key),
    }
}

fn remove_item(t: &Target, root_key: &[String], canon_key: &[String], item: &Item, now: i64) -> bool {
    if !lexically_allowed(t, root_key, &item.path) {
        return false;
    }
    let real_inside = |p: &Path| match path_key(p) {
        Some(k) if t.shape == Shape::File => k == canon_key,
        Some(k) => key_strictly_inside(&k, canon_key),
        None => false,
    };

    if t.recycle {
        // The Recycle Bin path cannot use our handle, so check the entry
        // again as late as possible, and the real folder it sits in.
        let Ok(meta) = fs::symlink_metadata(&item.path) else { return false };
        let attrs = meta.file_attributes();
        if is_dir_attr(attrs) || is_reparse(attrs) {
            return false;
        }
        if local_bytes(attrs, meta.len()) < t.min_bytes
            || !old_enough(meta_newest(&meta), t.min_age_secs, now)
        {
            return false;
        }
        let parent_ok = item
            .path
            .parent()
            .and_then(|p| fs::canonicalize(p).ok())
            .and_then(|p| path_key(&p))
            .is_some_and(|k| key_starts_with(&k, canon_key));
        return parent_ok && trash::delete(&item.path).is_ok();
    }

    let result = delete_checked(&item.path, |info: &HandleInfo| {
        if !real_inside(&info.final_path) {
            return Err("Outside the folder being cleaned");
        }
        let link = is_reparse(info.attrs);
        if is_dir_attr(info.attrs) && !link {
            return Err("Not a plain file");
        }
        let bytes = if link { 0 } else { local_bytes(info.attrs, info.bytes) };
        if bytes < t.min_bytes || !old_enough(newest(info.created, info.modified), t.min_age_secs, now)
        {
            return Err("Too new");
        }
        Ok(())
    });
    result.is_ok()
}

/// Remove what `collect` finds in `t`. Returns the counts and whether it
/// stopped because of a cancel.
fn clean_target(t: &Target, now: i64, ctx: &Ctx) -> (Counts, bool) {
    let mut counts = Counts::default();
    let Some(root_key) = path_key(&t.root) else { return (counts, false) };
    let Some(canon_key) = fs::canonicalize(&t.root).ok().and_then(|p| path_key(&p)) else {
        return (counts, false);
    };
    // The root must still be what we checked: a folder that is not a link.
    if t.shape != Shape::File && !is_real_dir(&t.root) {
        return (counts, false);
    }

    let found = collect(t, now, ctx);
    for item in &found.items {
        if ctx.cancelled() {
            return (counts, true);
        }
        if remove_item(t, &root_key, &canon_key, item, now) {
            counts.removed += 1;
            counts.freed += item.bytes;
        } else {
            counts.skipped += 1;
        }
    }

    if t.shape == Shape::Contents {
        let mut dirs = found.dirs;
        // Deepest first, so a parent is only tried once its children have gone.
        dirs.sort_by_key(|d| std::cmp::Reverse(d.1));
        for (dir, _, when) in &dirs {
            if ctx.cancelled() {
                return (counts, true);
            }
            // The age is the one recorded before we removed anything, since
            // removing files bumps the folder's own modified time.
            if !old_enough(*when, t.min_age_secs, now) || !lexically_allowed(t, &root_key, dir) {
                continue;
            }
            // Fails harmlessly when the folder is not empty.
            let _ = delete_checked(dir, |info| {
                match path_key(&info.final_path) {
                    Some(k) if key_strictly_inside(&k, &canon_key) => {}
                    _ => return Err("Outside the folder being cleaned"),
                }
                if !is_dir_attr(info.attrs) {
                    return Err("Not a folder");
                }
                Ok(())
            });
        }
    }
    (counts, false)
}

// ── Recycle Bin and registry ─────────────────────────────────────────────────

/// Size and item count of the Recycle Bin across all drives.
fn recycle_bin_info() -> Option<(u64, u64)> {
    let mut info = SHQUERYRBINFO {
        cbSize: std::mem::size_of::<SHQUERYRBINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: `info` is a correctly sized, initialised struct that outlives
    // the call. A null root asks about every drive.
    unsafe { SHQueryRecycleBinW(PCWSTR::null(), &mut info) }.ok()?;
    Some((info.i64Size.max(0) as u64, info.i64NumItems.max(0) as u64))
}

fn empty_recycle_bin() -> Counts {
    let Some((bytes, count)) = recycle_bin_info() else {
        return Counts::default();
    };
    if count == 0 {
        return Counts::default();
    }
    // SAFETY: no window handle and a null root (every drive) are valid
    // arguments. The flags stop any dialog, progress window or sound.
    let ok = unsafe {
        SHEmptyRecycleBinW(None, PCWSTR::null(), SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND)
    }
    .is_ok();
    if ok {
        Counts { removed: count, freed: bytes, skipped: 0 }
    } else {
        Counts { removed: 0, freed: 0, skipped: count }
    }
}

fn registry_value_count(subkey: &str) -> Option<u64> {
    let key = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(subkey, KEY_QUERY_VALUE).ok()?;
    key.query_info().ok().map(|i| u64::from(i.values))
}

/// Delete every value under one of the three allowlisted keys. The key
/// itself stays. Any other key is refused before anything is opened.
fn clear_registry_values(subkey: &str) -> Result<Counts> {
    if !REGISTRY_KEYS.contains(&subkey) {
        return Err(Error::refused("That registry key is not on the cleaner's list."));
    }
    let Ok(key) = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(subkey, KEY_QUERY_VALUE | KEY_SET_VALUE)
    else {
        return Ok(Counts::default());
    };
    let names: Vec<_> = key.enum_values_os_string().flatten().map(|(name, _)| name).collect();
    let mut counts = Counts::default();
    for name in names {
        if key.delete_value(&name).is_ok() {
            counts.removed += 1;
        } else {
            counts.skipped += 1;
        }
    }
    Ok(counts)
}

// ── Public API ───────────────────────────────────────────────────────────────

/// What one category holds right now, or None if it does not exist here.
fn measure(def: &Def, opts: CleanOptions, now: i64, ctx: &Ctx) -> Option<(u64, u64)> {
    match def.source {
        Source::RecycleBin => recycle_bin_info(),
        Source::Registry(subkey) => registry_value_count(subkey).map(|n| (0, n)),
        _ => {
            let targets = targets(def, opts, &real_env);
            if targets.is_empty() {
                return None;
            }
            let (mut bytes, mut count) = (0, 0);
            for t in &targets {
                let c = collect(t, now, ctx);
                bytes += c.bytes();
                count += c.items.len() as u64;
            }
            Some((bytes, count))
        }
    }
}

/// Measure every category in `area` that exists on this machine.
///
/// Reports a status line per category through `ctx` and stops early with
/// `Error::Cancelled` if `ctx.cancelled()`.
pub fn scan(area: Area, opts: CleanOptions, ctx: &Ctx) -> Result<CleanScan> {
    let started = Instant::now();
    let now = now_unix();
    let mut categories = Vec::new();
    for def in DEFS.iter().filter(|d| d.area == area) {
        if ctx.cancelled() {
            return Err(Error::Cancelled);
        }
        ctx.status(format!("Scanning {}", def.label));
        let Some((bytes, item_count)) = measure(def, opts, now, ctx) else { continue };
        if ctx.cancelled() {
            return Err(Error::Cancelled);
        }
        categories.push(CleanCategory {
            id: def.id.to_string(),
            area: def.area,
            group: def.group.to_string(),
            label: def.label.to_string(),
            description: def.description.to_string(),
            note: def.note.to_string(),
            risk: def.risk,
            bytes,
            item_count,
            needs_admin: def.needs_admin,
            default_selected: def.default_selected,
            to_recycle_bin: def.to_recycle_bin,
            can_list: def.can_list(),
        });
    }
    // Stable, so equal sizes keep the table's order.
    categories.sort_by_key(|c| std::cmp::Reverse(c.bytes));
    Ok(CleanScan {
        area,
        total_bytes: categories.iter().map(|c| c.bytes).sum(),
        safe_bytes: categories.iter().filter(|c| c.risk == Risk::Safe).map(|c| c.bytes).sum(),
        total_items: categories.iter().map(|c| c.item_count).sum(),
        categories,
        scanned_at: now_unix(),
        elapsed_ms: started.elapsed().as_millis() as u64,
    })
}

/// The files behind one category, biggest first, at most `limit` of them.
/// Errors with `Error::NotFound` for an unknown id.
pub fn list_files(id: &str, opts: CleanOptions, limit: usize) -> Result<CategoryFiles> {
    let def = find_def(id).ok_or_else(|| Error::not_found(format!("No cleaner category called {id}")))?;
    let mut files = Vec::new();
    if def.can_list() {
        let now = now_unix();
        for t in targets(def, opts, &real_env) {
            files.extend(collect(&t, now, &Ctx::none()).items.into_iter().map(|i| FileEntry {
                path: i.path.to_string_lossy().into_owned(),
                bytes: i.bytes,
                modified: i.modified,
            }));
        }
    }
    files.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.path.cmp(&b.path)));
    let total_items = files.len() as u64;
    let truncated = files.len() > limit;
    files.truncate(limit);
    Ok(CategoryFiles { id: def.id.to_string(), files, total_items, truncated })
}

/// Check every id before anything is touched. Duplicates are dropped. The
/// Recycle Bin goes first, so files the same clean sends to the bin (old
/// downloads) are not emptied straight out of it again.
fn validate(area: Area, ids: &[String]) -> Result<Vec<&'static Def>> {
    let mut seen = HashSet::new();
    let mut defs = Vec::new();
    for id in ids {
        let def = find_def(id).ok_or_else(|| Error::refused(format!("{id} is not a cleaner category")))?;
        if def.area != area {
            return Err(Error::refused(format!("{} does not belong to this cleaner", def.label)));
        }
        if seen.insert(def.id) {
            defs.push(def);
        }
    }
    defs.sort_by_key(|d| !matches!(d.source, Source::RecycleBin));
    Ok(defs)
}

/// Remove what the given categories hold and return the receipt.
///
/// Every id must belong to `area`, otherwise the whole call is refused with
/// `Error::Refused` before anything is touched.
pub fn clean(area: Area, ids: &[String], opts: CleanOptions, ctx: &Ctx) -> Result<CleanReport> {
    let defs = validate(area, ids)?;
    let started = Instant::now();
    let now = now_unix();
    let total = defs.len().max(1) as f32;
    let mut items = Vec::new();

    for (i, def) in defs.iter().enumerate() {
        if ctx.cancelled() {
            break;
        }
        ctx.step(format!("Cleaning {}", def.label), i as f32 / total);
        let mut counts = Counts::default();
        let mut stopped = false;
        match def.source {
            Source::RecycleBin => counts = empty_recycle_bin(),
            Source::Registry(subkey) => counts = clear_registry_values(subkey)?,
            _ => {
                for t in targets(def, opts, &real_env) {
                    let (c, cancelled) = clean_target(&t, now, ctx);
                    counts.removed += c.removed;
                    counts.freed += c.freed;
                    counts.skipped += c.skipped;
                    if cancelled {
                        stopped = true;
                        break;
                    }
                }
            }
        }
        items.push(CleanItemResult {
            id: def.id.to_string(),
            label: def.label.to_string(),
            freed_bytes: counts.freed,
            removed: counts.removed,
            skipped: counts.skipped,
        });
        if stopped {
            break;
        }
    }
    ctx.step("Done", 1.0);
    Ok(CleanReport {
        area,
        freed_bytes: items.iter().map(|i| i.freed_bytes).sum(),
        removed: items.iter().map(|i| i.removed).sum(),
        skipped: items.iter().map(|i| i.skipped).sum(),
        items,
        finished_at: now_unix(),
        elapsed_ms: started.elapsed().as_millis() as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fsops::test_util::{age, keep_open_exclusive, make_junction, write};

    fn folder_target(root: &Path, shape: Shape, matcher: Match, min_age_secs: Option<i64>) -> Target {
        Target {
            root: root.to_path_buf(),
            shape,
            matcher,
            min_age_secs,
            min_bytes: 0,
            recycle: false,
        }
    }

    #[test]
    fn children_deleted_root_kept() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("cache");
        write(&root.join("a.bin"), 100);
        write(&root.join(r"sub\b.bin"), 50);
        write(&root.join(r"sub\deeper\c.bin"), 25);
        let t = folder_target(&root, Shape::Contents, Match::Any, None);
        let (c, cancelled) = clean_target(&t, now_unix(), &Ctx::none());
        assert!(!cancelled);
        assert_eq!(c, Counts { removed: 3, freed: 175, skipped: 0 });
        assert!(root.is_dir(), "the root itself must survive");
        assert_eq!(fs::read_dir(&root).expect("read").count(), 0, "empty folders tidied away");
    }

    #[test]
    fn junction_inside_root_is_removed_as_a_link() {
        let dir = tempfile::tempdir().expect("tempdir");
        let outside = tempfile::tempdir().expect("tempdir");
        let precious = outside.path().join(r"keep\precious.txt");
        write(&precious, 1000);
        let root = dir.path().join("temp");
        write(&root.join("junk.tmp"), 10);
        let link = root.join("link");
        if !make_junction(&link, outside.path()) {
            eprintln!("skipped: could not create a junction");
            return;
        }
        let t = folder_target(&root, Shape::Contents, Match::Any, None);
        let found = collect(&t, now_unix(), &Ctx::none());
        assert_eq!(found.bytes(), 10, "nothing behind the junction is counted");
        let (c, _) = clean_target(&t, now_unix(), &Ctx::none());
        assert_eq!(c.removed, 2);
        assert!(fs::symlink_metadata(&link).is_err(), "the link is gone");
        assert!(precious.exists(), "the junction target survives");
    }

    #[test]
    fn age_rule_keeps_new_and_removes_old() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("temp");
        let old = root.join("old.tmp");
        let new = root.join("new.tmp");
        let old_dir = root.join("old_empty");
        let new_dir = root.join("new_empty");
        let mixed = root.join("mixed");
        write(&old, 10);
        write(&new, 20);
        write(&mixed.join("old_inside.tmp"), 5);
        write(&mixed.join("new_inside.tmp"), 5);
        fs::create_dir_all(&old_dir).expect("mkdir");
        fs::create_dir_all(&new_dir).expect("mkdir");
        age(&old, 3);
        age(&mixed.join("old_inside.tmp"), 3);
        age(&old_dir, 3);
        age(&mixed, 3);

        let opts = CleanOptions { temp_min_age_hours: 24 };
        let t = folder_target(&root, Shape::Contents, Match::Any, min_age(Age::Temp, opts));
        let scanned = collect(&t, now_unix(), &Ctx::none());
        let (c, _) = clean_target(&t, now_unix(), &Ctx::none());
        assert_eq!(c.removed, scanned.items.len() as u64);
        assert_eq!(c.removed, 2);
        assert!(!old.exists());
        assert!(new.exists());
        assert!(!old_dir.exists());
        assert!(new_dir.exists());
        assert!(mixed.join("new_inside.tmp").exists());
        assert!(!mixed.join("old_inside.tmp").exists());
        assert!(mixed.exists(), "a folder that still holds a new file stays");

        // 0 switches the rule off.
        assert_eq!(min_age(Age::Temp, CleanOptions { temp_min_age_hours: 0 }), None);
    }

    #[test]
    fn pattern_rules_only_take_matching_files_at_the_right_depth() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("Explorer");
        write(&root.join("thumbcache_256.db"), 10);
        write(&root.join("IconCache_32.db"), 10);
        write(&root.join("other.db"), 10);
        write(&root.join(r"sub\thumbcache_1.db"), 10);
        let t = folder_target(&root, Shape::Top, Match::ExplorerCache, None);
        let (c, _) = clean_target(&t, now_unix(), &Ctx::none());
        assert_eq!(c.removed, 2);
        assert!(root.join("other.db").exists());
        assert!(root.join(r"sub\thumbcache_1.db").exists());

        // Downloads style: size, age and the keep list all apply.
        let dl = dir.path().join("Downloads");
        write(&dl.join("setup.exe"), 2048);
        write(&dl.join("holiday.jpg"), 2048);
        write(&dl.join("small.zip"), 10);
        write(&dl.join(r"nested\fresh.iso"), 2048);
        age(&dl.join("setup.exe"), 100);
        age(&dl.join("holiday.jpg"), 100);
        age(&dl.join("small.zip"), 100);
        let mut t = folder_target(&dl, Shape::Tree, Match::NotKept, Some(90 * 86_400));
        t.min_bytes = 1024;
        let found = collect(&t, now_unix(), &Ctx::none());
        assert_eq!(found.items.len(), 1);
        assert!(found.items[0].path.ends_with("setup.exe"));
    }

    #[test]
    fn locked_files_are_skipped_not_fatal() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("temp");
        write(&root.join("locked.tmp"), 10);
        write(&root.join("free.tmp"), 10);
        let guard = keep_open_exclusive(&root.join("locked.tmp"));
        let t = folder_target(&root, Shape::Contents, Match::Any, None);
        let (c, _) = clean_target(&t, now_unix(), &Ctx::none());
        drop(guard);
        assert_eq!(c, Counts { removed: 1, freed: 10, skipped: 1 });
        assert!(root.join("locked.tmp").exists());
    }

    #[test]
    fn scan_counts_match_what_clean_removes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path().join("temp");
        for i in 0..20 {
            write(&root.join(format!(r"d{}\f{i}.tmp", i % 4)), 100 + i);
            if i % 3 == 0 {
                age(&root.join(format!(r"d{}\f{i}.tmp", i % 4)), 5);
            }
        }
        let t = folder_target(&root, Shape::Contents, Match::Any, Some(86_400));
        let now = now_unix();
        let scanned = collect(&t, now, &Ctx::none());
        let (c, _) = clean_target(&t, now, &Ctx::none());
        assert_eq!(c.removed, scanned.items.len() as u64);
        assert_eq!(c.freed, scanned.bytes());
        assert_eq!(c.skipped, 0);
        assert_eq!(c.removed, 7);
    }

    #[test]
    fn file_target_only_removes_that_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let dump = dir.path().join("MEMORY.DMP");
        write(&dump, 64);
        write(&dir.path().join("neighbour.txt"), 64);
        let t = folder_target(&dump, Shape::File, Match::Any, None);
        let (c, _) = clean_target(&t, now_unix(), &Ctx::none());
        assert_eq!(c.removed, 1);
        assert!(dir.path().join("neighbour.txt").exists());
        assert!(dir.path().exists());
    }

    #[test]
    fn paths_outside_or_with_dot_dot_are_refused() {
        let t = folder_target(Path::new(r"C:\Users\a\AppData\Local\Temp"), Shape::Contents, Match::Any, None);
        let key = path_key(&t.root).expect("key");
        assert!(lexically_allowed(&t, &key, Path::new(r"C:\Users\a\AppData\Local\Temp\x.tmp")));
        assert!(lexically_allowed(&t, &key, Path::new(r"c:\users\A\appdata\local\temp\X.TMP")));
        assert!(!lexically_allowed(&t, &key, Path::new(r"C:\Users\a\AppData\Local\Temp")));
        assert!(!lexically_allowed(&t, &key, Path::new(r"C:\Users\a\AppData\Local\Temp\..\x")));
        assert!(!lexically_allowed(&t, &key, Path::new(r"C:\Users\a\AppData\Local\Temporary\x")));
    }

    fn fake_env(name: &str) -> Option<String> {
        let v = match name {
            "SystemRoot" | "windir" => r"C:\Windows",
            "SystemDrive" => "C:",
            "USERPROFILE" => r"C:\Users\a",
            "APPDATA" => r"C:\Users\a\AppData\Roaming",
            "LOCALAPPDATA" => r"C:\Users\a\AppData\Local",
            "ProgramFiles" | "ProgramW6432" => r"C:\Program Files",
            "ProgramFiles(x86)" => r"C:\Program Files (x86)",
            "ProgramData" => r"C:\ProgramData",
            "PUBLIC" => r"C:\Users\Public",
            _ => return None,
        };
        Some(v.to_string())
    }

    #[test]
    fn suspicious_roots_are_refused() {
        let env: Env = &fake_env;
        let bad = [
            (r"C:\", Base::Temp),
            (r"C:\Users\a", Base::Temp),
            (r"C:\Users\a", Base::User),
            (r"C:\Users\a\AppData", Base::User),
            (r"C:\Users\a\AppData\Local", Base::User),
            (r"C:\Users", Base::Temp),
            (r"C:\Windows", Base::Machine),
            (r"C:\Windows\Temp", Base::Temp),
            (r"C:\Windows\System32\drivers", Base::Temp),
            (r"C:\Program Files\App\Temp", Base::User),
            (r"C:\ProgramData", Base::Machine),
            (r"C:\Users\a\AppData\Local\..\..\..\Windows", Base::User),
            (r"Temp", Base::Temp),
        ];
        for (path, base) in bad {
            assert!(!root_is_safe(Path::new(path), base, env), "{path} should be refused");
        }
        let good = [
            (r"C:\Users\a\AppData\Local\Temp", Base::Temp),
            (r"D:\Temp", Base::Temp),
            (r"C:\Windows\Temp", Base::Machine),
            (r"C:\Users\a\AppData\Local\CrashDumps", Base::User),
        ];
        for (path, base) in good {
            assert!(root_is_safe(Path::new(path), base, env), "{path} should be allowed");
        }

        // A TEMP of C:\ makes the category disappear rather than fall back.
        let user_temp = find_def("user-temp").expect("def");
        let temp_is_drive = |n: &str| if n == "TEMP" { Some(r"C:\".to_string()) } else { fake_env(n) };
        assert!(targets(user_temp, CleanOptions::default(), &temp_is_drive).is_empty());
        let temp_is_profile =
            |n: &str| if n == "TEMP" { Some(r"C:\Users\a".to_string()) } else { fake_env(n) };
        assert!(targets(user_temp, CleanOptions::default(), &temp_is_profile).is_empty());
    }

    #[test]
    fn unknown_and_wrong_area_ids_refuse_the_whole_call() {
        assert!(matches!(
            clean(Area::Junk, &["no-such-thing".to_string()], CleanOptions::default(), &Ctx::none()),
            Err(Error::Refused(_))
        ));
        assert!(matches!(
            clean(Area::Privacy, &["no-such-thing".to_string()], CleanOptions::default(), &Ctx::none()),
            Err(Error::Refused(_))
        ));
        // Wrong-area checks go through `validate` only, so a bug here could
        // never clean a real category on this machine.
        assert!(matches!(
            validate(Area::Junk, &["user-temp".into(), "recent-files".into()]),
            Err(Error::Refused(_))
        ));
        assert!(matches!(validate(Area::Privacy, &["typed-paths".into(), "nope".into()]), Err(Error::Refused(_))));
        let ok = validate(Area::Junk, &["old-downloads".into(), "recycle-bin".into(), "old-downloads".into()])
            .expect("valid");
        assert_eq!(ok.iter().map(|d| d.id).collect::<Vec<_>>(), vec!["recycle-bin", "old-downloads"]);
        assert!(matches!(list_files("nope", CleanOptions::default(), 10), Err(Error::NotFound(_))));
    }

    #[test]
    fn registry_cleaning_is_limited_to_three_keys() {
        assert!(matches!(clear_registry_values(r"Software\Microsoft\Windows\CurrentVersion\Run"), Err(Error::Refused(_))));
        assert!(matches!(clear_registry_values(r"Software"), Err(Error::Refused(_))));
        let registry_defs: Vec<_> = DEFS
            .iter()
            .filter_map(|d| match d.source {
                Source::Registry(k) => Some(k),
                _ => None,
            })
            .collect();
        assert_eq!(registry_defs.len(), 3);
        assert!(registry_defs.iter().all(|k| REGISTRY_KEYS.contains(k)));
    }

    #[test]
    fn the_table_is_well_formed() {
        let mut ids = HashSet::new();
        for d in DEFS {
            assert!(ids.insert(d.id), "duplicate id {}", d.id);
            assert!(!(d.risk == Risk::Review && d.default_selected), "{} is ticked by default", d.id);
            for text in [d.label, d.description, d.note] {
                assert!(!text.contains('\u{2014}') && !text.contains('!'), "{text}");
            }
        }
        assert_eq!(DEFS.len(), 21);
        assert!(find_def("old-downloads").is_some_and(|d| d.to_recycle_bin));
        assert!(DEFS.iter().filter(|d| d.to_recycle_bin).count() == 1);
    }

    #[test]
    fn real_scans_run_read_only() {
        let junk = scan(Area::Junk, CleanOptions::default(), &Ctx::none()).expect("junk scan");
        assert!(junk.categories.iter().all(|c| c.area == Area::Junk));
        assert!(junk.categories.iter().any(|c| c.id == "recycle-bin"));
        assert!(junk.categories.windows(2).all(|w| w[0].bytes >= w[1].bytes));
        let privacy = scan(Area::Privacy, CleanOptions::default(), &Ctx::none()).expect("privacy scan");
        assert!(privacy.categories.iter().all(|c| c.area == Area::Privacy));
        let listed = list_files("user-temp", CleanOptions::default(), 5).expect("list");
        assert!(listed.files.len() <= 5);
        assert!(list_files("recycle-bin", CleanOptions::default(), 5).expect("list").files.is_empty());
    }
}
