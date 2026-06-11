# WinScan

Windows system toolkit desktop app (disk scanning, drill-down, cleanup) built with CustomTkinter, with optional Google Gemini AI features. Split into an `app/` package with `winscan.py` as a thin entry point.

## Key files

- `winscan.py` - entry point only: docstring, the DPI-awareness ctypes block, then `from app.main import main`.
- `app/config.py` - configuration constants (`GEMINI_MODEL`, `APP_VERSION`, scan thresholds, `SAFE_EXTENSIONS`).
- `app/theme.py` - design tokens: colours, radii, icons, `DPI_SCALE`, font helpers (`F_BODY` etc.).
- `app/helpers.py` - size formatting, file ages, RAM/uptime via ctypes.
- `app/widgets.py` - shared UI building blocks: treeviews, page headers, `Tile`, `ActionCard`.
- `app/scanner.py` - cleanup scan logic: `ScanResult`, `scan_all`, `delete_category`.
- `app/gemini.py` - `ask_gemini` (lazy-imports `google.generativeai`).
- `app/settings.py` - `key_file()`: location of the saved Gemini API key (`.winscan_key`).
- `app/ctk_setup.py` - CustomTkinter appearance mode + DPI window-scaling fix; import side effects only.
- `app/tools/` - one module per tool page (cleanup, system_info, large_files, startup, programs, network, duplicates, privacy, tasks, disk, home).
- `app/main.py` - app shell: `TOOL_SECTIONS` registry, `WinScanApp`, `main()`.
- `run.bat` - installs deps if missing and runs the app directly with Python.
- `build.bat` - PyInstaller one-file build, outputs `dist\WinScan.exe` (portable).
- `WinScan.spec`, `winscan.ico`, `gen_icon.py` - build artefacts and icon generator.
- `.shot.ps1` - screenshots the running WinScan window (used to verify launches).

## Run / build

```
run.bat            # or: python winscan.py (best run as Administrator)
build.bat          # portable EXE -> dist\WinScan.exe
```

No tests. Dependencies: customtkinter, google-generativeai, pyinstaller (build only).

## Conventions

- `winscan.py` must stay the entry point: both `build.bat` and `WinScan.spec` reference it, and PyInstaller discovers the `app` package by following its imports.
- The DPI-awareness ctypes block at the very top of `winscan.py` must stay before any Tk/CTk import, or per-monitor scaling silently breaks.
- `app/ctk_setup.py` must be imported before `WinScanApp` is constructed (app/main.py does this first); it sets appearance mode and the DPI window-scaling fix.
- Design tokens live in `app/theme.py`; keep token names stable.
- Gemini model is the `GEMINI_MODEL` constant in `app/config.py`; the user's API key is saved to `.winscan_key` next to the EXE when frozen, otherwise next to `winscan.py` (see `app/settings.py`). Never commit `.winscan_key`.
