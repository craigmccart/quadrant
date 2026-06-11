"""WinScan — cleanup scan logic: categories, scanning and deletion."""

import shutil
from pathlib import Path
from typing import Optional, Callable

from app.config import LARGE_FILE_MIN_BYTES, OLD_FILE_DAYS, SAFE_EXTENSIONS
from app.helpers import days_since_access, dir_size, expand, fmt_size


# ── ScanResult ────────────────────────────────────────────────────────────────

class ScanResult:
    def __init__(self, label: str, paths: list[Path], total_bytes: int, note: str = ""):
        self.label = label
        self.paths = paths
        self.total_bytes = total_bytes
        self.note = note

# ── Scan logic ────────────────────────────────────────────────────────────────

def _browser_cache(path: Path, label: str, out: list):
    if path.exists():
        size, count = dir_size(path)
        if count > 0:
            out.append(ScanResult(label, [path], size,
                f"{count:,} cached files — browser rebuilds automatically"))

def scan_all(log_cb: Optional[Callable] = None) -> list[ScanResult]:
    results: list[ScanResult] = []

    def log(msg):
        if log_cb:
            log_cb(msg)

    log("Scanning User Temp (%TEMP%) …")
    tmp = expand("%TEMP%")
    if tmp.exists():
        s, n = dir_size(tmp)
        if n:
            results.append(ScanResult("User Temp  (%TEMP%)", [tmp], s,
                f"{n:,} files — safe to clear entirely"))

    log("Scanning Windows System Temp …")
    wtmp = Path(r"C:\Windows\Temp")
    if wtmp.exists():
        s, n = dir_size(wtmp)
        if n:
            results.append(ScanResult("Windows System Temp", [wtmp], s,
                f"{n:,} files — may skip locked files"))

    log("Scanning Windows Update cache …")
    wuc = Path(r"C:\Windows\SoftwareDistribution\Download")
    if wuc.exists():
        s, n = dir_size(wuc)
        if n:
            results.append(ScanResult("Windows Update Download Cache", [wuc], s,
                f"{n:,} downloaded update packages"))

    log("Scanning Prefetch …")
    pf = Path(r"C:\Windows\Prefetch")
    if pf.exists():
        files = list(pf.glob("*.pf"))
        s = sum(f.stat().st_size for f in files if f.is_file())
        if files:
            results.append(ScanResult("Windows Prefetch", files, s,
                f"{len(files):,} .pf files — Windows rebuilds automatically"))

    log("Scanning Chrome cache …")
    for sub in (r"Cache", r"Cache2"):
        cc = expand(rf"%LOCALAPPDATA%\Google\Chrome\User Data\Default\{sub}")
        if cc.exists():
            _browser_cache(cc, "Google Chrome Cache", results)
            break

    log("Scanning Edge cache …")
    for sub in (r"Cache", r"Cache2"):
        ec = expand(rf"%LOCALAPPDATA%\Microsoft\Edge\User Data\Default\{sub}")
        if ec.exists():
            _browser_cache(ec, "Microsoft Edge Cache", results)
            break

    log("Scanning Firefox cache …")
    ffr = expand(r"%LOCALAPPDATA%\Mozilla\Firefox\Profiles")
    if ffr.exists():
        for fc in ffr.glob("*/cache2"):
            _browser_cache(fc, "Firefox Cache", results)

    log("Scanning thumbnail cache …")
    thumbs = list(expand(r"%LOCALAPPDATA%\Microsoft\Windows\Explorer").glob("thumbcache_*.db"))
    if thumbs:
        s = sum(f.stat().st_size for f in thumbs if f.is_file())
        results.append(ScanResult("Thumbnail Cache", thumbs, s,
            f"{len(thumbs):,} files — Windows regenerates as needed"))

    log("Scanning Recycle Bin …")
    rb_size, rb_paths = 0, []
    for d in "CDEFGH":
        rb = Path(f"{d}:\\$RECYCLE.BIN")
        if rb.exists():
            s, _ = dir_size(rb)
            if s:
                rb_size += s
                rb_paths.append(rb)
    if rb_paths:
        results.append(ScanResult("Recycle Bin", rb_paths, rb_size,
            "Already-deleted files waiting to be emptied"))

    log("Scanning crash dumps & error reports …")
    crash_dirs = [
        expand(r"%LOCALAPPDATA%\CrashDumps"),
        expand(r"%APPDATA%\Microsoft\Windows\WER\ReportArchive"),
        expand(r"%APPDATA%\Microsoft\Windows\WER\ReportQueue"),
        Path(r"C:\ProgramData\Microsoft\Windows\WER\ReportArchive"),
        Path(r"C:\ProgramData\Microsoft\Windows\WER\ReportQueue"),
    ]
    cs, cp = 0, []
    for d in crash_dirs:
        if d.exists():
            s, n = dir_size(d)
            if n:
                cs += s; cp.append(d)
    if cp:
        results.append(ScanResult("Crash Dumps & Error Reports", cp, cs,
            "Diagnostic files — safe once issues resolved"))

    log("Scanning old log files …")
    log_files, log_size = [], 0
    for ld in [Path(r"C:\Windows\Logs"),
               expand(r"%APPDATA%\..\..\ProgramData\Microsoft\Windows\Logs")]:
        if ld.exists():
            for ext in ("*.log", "*.etl"):
                for f in ld.rglob(ext):
                    try:
                        if f.is_file() and days_since_access(f) > OLD_FILE_DAYS:
                            log_files.append(f)
                            log_size += f.stat().st_size
                    except OSError:
                        pass
    if log_files:
        results.append(ScanResult(
            f"Old Log Files  (>{OLD_FILE_DAYS} days)",
            log_files[:500], log_size,
            f"{len(log_files):,} diagnostic logs not accessed in {OLD_FILE_DAYS}+ days"))

    log(f"Scanning Downloads for large old files …")
    dl = expand(r"%USERPROFILE%\Downloads")
    dl_files, dl_size = [], 0
    if dl.exists():
        for f in dl.rglob("*"):
            try:
                if (f.is_file(follow_symlinks=False)
                        and f.suffix.lower() not in SAFE_EXTENSIONS
                        and f.stat().st_size >= LARGE_FILE_MIN_BYTES
                        and days_since_access(f) > OLD_FILE_DAYS):
                    dl_files.append(f)
                    dl_size += f.stat().st_size
            except (PermissionError, OSError):
                pass
    if dl_files:
        results.append(ScanResult(
            f"Large Old Files in Downloads  (>{fmt_size(LARGE_FILE_MIN_BYTES)}, >{OLD_FILE_DAYS}d)",
            dl_files, dl_size,
            f"{len(dl_files):,} files — review before deleting"))

    log("Scanning package manager caches …")
    pkg = {
        "NuGet Package Cache":  expand(r"%USERPROFILE%\.nuget\packages"),
        "npm Cache":            expand(r"%APPDATA%\npm-cache"),
        "pip Cache":            expand(r"%LOCALAPPDATA%\pip\Cache"),
    }
    for lbl, d in pkg.items():
        if d.exists():
            s, n = dir_size(d)
            if s > 5 * 1024 * 1024:
                results.append(ScanResult(lbl, [d], s,
                    f"{n:,} cached packages — regenerated on next build"))

    log("Scan complete.")
    return results

# ── Delete helpers ────────────────────────────────────────────────────────────

def delete_category(result: ScanResult) -> tuple[int, int, int]:
    """Returns (freed_bytes, ok_count, fail_count)."""
    freed, ok, fail = 0, 0, 0
    for p in result.paths:
        try:
            size = p.stat().st_size if p.is_file() else dir_size(p)[0]
        except OSError:
            size = 0
        try:
            if p.is_dir():
                shutil.rmtree(p, ignore_errors=False)
            else:
                p.unlink(missing_ok=True)
            freed += size
            ok += 1
        except (PermissionError, OSError):
            fail += 1
    return freed, ok, fail
