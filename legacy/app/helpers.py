"""WinScan — shared helpers: size formatting, file ages, RAM/uptime."""

import ctypes
import os
import time
from pathlib import Path

from app.theme import GREEN, ORANGE, RED_SOFT


# ── Helpers ───────────────────────────────────────────────────────────────────

def fmt_size(b: int) -> str:
    if b < 0:
        b = 0
    for unit in ("B", "KB", "MB", "GB"):
        if b < 1024:
            return f"{b:.1f} {unit}"
        b /= 1024
    return f"{b:.1f} TB"

def size_color(b: int) -> str:
    if b < 50 * 1024 * 1024:
        return GREEN
    if b < 500 * 1024 * 1024:
        return ORANGE
    return RED_SOFT

def dir_size(path: Path) -> tuple[int, int]:
    total, count = 0, 0
    try:
        for entry in path.rglob("*"):
            try:
                if entry.is_file(follow_symlinks=False):
                    total += entry.stat().st_size
                    count += 1
            except (PermissionError, OSError):
                pass
    except (PermissionError, OSError):
        pass
    return total, count

def days_since_access(path: Path) -> float:
    try:
        return (time.time() - path.stat().st_atime) / 86400
    except OSError:
        return 0

def expand(s: str) -> Path:
    return Path(os.path.expandvars(os.path.expanduser(s)))

class _MEMORYSTATUSEX(ctypes.Structure):
    _fields_ = [
        ("dwLength", ctypes.c_ulong),
        ("dwMemoryLoad", ctypes.c_ulong),
        ("ullTotalPhys", ctypes.c_ulonglong),
        ("ullAvailPhys", ctypes.c_ulonglong),
        ("ullTotalPageFile", ctypes.c_ulonglong),
        ("ullAvailPageFile", ctypes.c_ulonglong),
        ("ullTotalVirtual", ctypes.c_ulonglong),
        ("ullAvailVirtual", ctypes.c_ulonglong),
        ("ullAvailExtendedVirtual", ctypes.c_ulonglong),
    ]

def _get_ram_info() -> tuple[int, int]:
    """Returns (total_bytes, available_bytes)."""
    stat = _MEMORYSTATUSEX()
    stat.dwLength = ctypes.sizeof(_MEMORYSTATUSEX)
    ctypes.windll.kernel32.GlobalMemoryStatusEx(ctypes.byref(stat))
    return stat.ullTotalPhys, stat.ullAvailPhys

def _get_uptime_str() -> str:
    ms = ctypes.windll.kernel32.GetTickCount64()
    secs = ms // 1000
    days, secs = divmod(secs, 86400)
    hours, secs = divmod(secs, 3600)
    mins = secs // 60
    parts = []
    if days:
        parts.append(f"{days}d")
    if hours:
        parts.append(f"{hours}h")
    parts.append(f"{mins}m")
    return " ".join(parts)
