# WinScan

Windows system toolkit desktop app (disk scanning, drill-down, cleanup) built with CustomTkinter, with optional Google Gemini AI features. Deliberately a single-file monolith.

## Key files

- `winscan.py` - the entire app (~2,600 lines): config constants, design tokens, UI, scan logic.
- `run.bat` - installs deps if missing and runs the app directly with Python.
- `build.bat` - PyInstaller one-file build, outputs `dist\WinScan.exe` (portable).
- `WinScan.spec`, `winscan.ico`, `gen_icon.py` - build artefacts and icon generator.

## Run / build

```
run.bat            # or: python winscan.py (best run as Administrator)
build.bat          # portable EXE -> dist\WinScan.exe
```

No tests. Dependencies: customtkinter, google-generativeai, pyinstaller (build only).

## Conventions

- Keep it one file - do not split into modules.
- The DPI-awareness ctypes block at the very top of `winscan.py` must stay before any Tk/CTk import, or per-monitor scaling silently breaks.
- Design tokens live in a named section near the top; keep token names stable.
- Gemini model is the `GEMINI_MODEL` constant; the user's API key is saved to `.winscan_key` next to the script/EXE. Never commit `.winscan_key`.
