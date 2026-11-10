# Quadrant

Windows PC care app: one-click Health Check, junk and privacy cleaning, duplicates, large files, disk map, startup, programs, scheduled tasks, security checks, network view. Rust core with a Tauri 2 shell; the window is plain HTML/CSS/JS in WebView2. Optional Google Gemini advice.

Version 3 is a Rust rebuild. The old Python/CustomTkinter app (2.1) is parked in `legacy/` and is not built or maintained.

## Layout

- `crates/core/` - `quadrant-core`: all scanning, cleaning and inspection logic. No UI dependencies. Unit tested.
  - `model.rs` - every type sent to the UI. This is the Rust to UI contract (camelCase JSON).
  - `cleanup.rs` - junk and privacy categories (a fixed allowlist) with scan, list and clean.
  - `fsops.rs`, `fsops/walk.rs` - safe deletion through a handle, protected paths, parallel walk.
  - `diskmap.rs`, `largefiles.rs`, `duplicates.rs` - the file tools.
  - `system.rs`, `startup.rs`, `programs.rs`, `tasks.rs`, `network.rs`, `security.rs` - the inspection tools.
  - `shell.rs` - opens Windows pages from a fixed id allowlist. `elevation.rs` - admin check and relaunch.
  - `secrets.rs` - Gemini key in Windows Credential Manager. `ai.rs` - the Gemini call and `GEMINI_MODEL`.
  - `health.rs` - Health Check orchestration and scoring. `history.rs` - clean-up receipts.
- `src-tauri/` - the app shell. `commands.rs` is the full list of what the web view may call. `state.rs` holds scan results between calls. `chrome.rs` tints the native title bar.
- `ui/` - the interface. No bundler, no npm: ES modules loaded as they are.
  - `css/tokens.css` - design tokens (see `brand.md`). `base.css`, `components.css`, `pages.css`, `tools.css`.
  - `js/api.js` - every backend call. `js/mock.js` - fake backend used only in a browser preview.
  - `js/pages/*.js` - one module per page. `js/dial.js` - the health dial.
  - `fonts/` (SIL OFL), `img/icons.svg` (Lucide, built by `tools/build-icons.mjs`).
- `tools/cdp.mjs` - drives a debug build through the WebView2 DevTools port for screenshots and checks.
- `brand.md` - design direction and tokens. `legacy/` - the Python app.

## Run / build / test

```
run.bat                        # cargo run -p quadrant (debug)
build.bat                      # release build -> dist\Quadrant.exe (portable, ~13 MB)
cargo test -p quadrant-core     # core tests
```

UI preview without the app: serve `ui/` (`.claude/launch.json` has `quadrant-ui`) and open it in a browser. It runs on mock data.

Checking the real app: set `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9223`, start `target\debug\Quadrant.exe`, then `node tools/cdp.mjs eval "quadrant.navigate('junk')" shot out.png`. Debug builds only.

## Conventions

- Destructive commands take ids, never paths. Ids are resolved in the core against the category allowlist, a fresh listing, or the scan kept in `AppState`. Keep it that way.
- Deleting never follows junctions or symlinks, never removes a category root, and counts locked files as skipped. Each rule has a test in `cleanup.rs` or `fsops.rs`. Do not weaken one without changing its test on purpose.
- Tests must not touch real system locations, the real Run keys, the real API key, or open windows. Use temp folders and scratch keys.
- The UI builds DOM with `h()` from `js/dom.js`, never HTML strings: file and program names are untrusted text. The CSP in `tauri.conf.json` has no `unsafe-inline`; set styles through the `style` prop (CSSOM).
- The web view gets no Tauri plugins. New capability means a new command in `commands.rs`.
- Release builds ignore the `WEBVIEW2_*` override environment variables (see `src-tauri/src/lib.rs`), because the app often runs elevated.
- Change a field in `model.rs` and the UI (and `mock.js`) must change with it.
- Colours, sizes and radii come from `ui/css/tokens.css`. If `--bg`, `--fg` or `--border` change, update `src-tauri/src/chrome.rs` to match.
- The Gemini key lives in Credential Manager. Never write it to a file or send it to the web view. Never commit `.winscan_key` (the old plain-text key file).
- UI copy: plain UK English, no em dashes, no exclamation marks, no scare wording.
