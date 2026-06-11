"""
WinScan — Windows System Toolkit
Powered by Google Gemini AI
"""

import ctypes

# Make the process DPI-aware as the VERY FIRST thing, before importing Tk/CTk.
# This must win the race before Tk creates its first window — once an HWND
# exists, Windows refuses to change DPI awareness, so a later call is silently
# ignored (which is why earlier attempts had no effect). PerMonitorV2 = (-4).
try:
    ctypes.windll.user32.SetProcessDpiAwarenessContext(ctypes.c_void_p(-4))
except (AttributeError, OSError):
    try:
        ctypes.windll.shcore.SetProcessDpiAwareness(2)
    except (AttributeError, OSError):
        pass

import os
import sys
import csv
import hashlib
import io
import shutil
import subprocess
import time
import threading
import winreg
from pathlib import Path
from typing import Optional, Callable
from datetime import datetime

import customtkinter as ctk
import tkinter as tk
from tkinter import ttk, messagebox, filedialog

# ── Configuration ─────────────────────────────────────────────────────────────

GEMINI_MODEL = "gemini-3.1-flash-lite-preview"
OLD_FILE_DAYS = 90
LARGE_FILE_MIN_BYTES = 100 * 1024 * 1024  # 100 MB
DRILL_DOWN_CAP = 200  # max files shown in drill-down per category

SAFE_EXTENSIONS = {
    ".jpg", ".jpeg", ".png", ".gif", ".bmp", ".tiff", ".tif", ".heic",
    ".heif", ".raw", ".cr2", ".nef", ".arw", ".dng", ".webp", ".svg",
    ".mp4", ".mov", ".avi", ".mkv", ".wmv", ".flv", ".m4v", ".mpg", ".mpeg",
    ".mp3", ".wav", ".flac", ".aac", ".ogg", ".wma", ".m4a",
    ".pdf", ".doc", ".docx", ".xls", ".xlsx", ".ppt", ".pptx",
    ".odt", ".ods", ".odp", ".rtf", ".txt", ".md",
    ".py", ".js", ".ts", ".cs", ".cpp", ".c", ".h", ".java", ".go",
    ".rs", ".rb", ".php", ".swift", ".kt",
}

# ── Design tokens ─────────────────────────────────────────────────────────────
#
# Identity: "instrument panel" — deep slate base, multiple saturated semantic
# colours used purposefully (green for safe, amber for caution, red for danger,
# cobalt for brand/action). High-contrast ink. Names kept stable so existing
# widgets keep rendering without churn — only the values changed.

# Surfaces — deep slate with subtle hue, layered depth (3+ levels)
BG_DEEP    = "#0d1117"   # app canvas
BG_CARD    = "#161b22"   # panels
BG_ROW     = "#1c232e"   # raised cards / rows
BG_ROW_A   = "#262d3a"   # hover / second alt
SIDEBAR_BG = "#0a0d12"   # sidebar (deepest)
BORDER     = "#2a3140"   # hairline
BORDER_HI  = "#3d4555"   # focus / strong divider

# Ink — high contrast, slightly cool
TEXT_MAIN  = "#f0f3f8"   # primary
TEXT_DIM   = "#9da7b3"   # secondary
TEXT_MUTE  = "#6b7484"   # tertiary

# Brand — cobalt blue. Reads as Windows-native + professional tool.
ACCENT     = "#4d9eff"   # primary brand / action
ACCENT_DIM = "#1f4d80"   # pressed / border
ACCENT_SOFT = "#0e2640"  # active item bg tint

# Semantic — saturated, visible at a glance
GREEN      = "#5ed590"   # safe / success / OK
ORANGE     = "#f0b340"   # warning (now distinct from brand)
ORANGE_SOFT = "#3a2c10"  # warning bg tint
RED_SOFT   = "#ff7070"   # danger inline (popping)
RED_BTN    = "#c94545"   # destructive button
RED_BTN_H  = "#e5605b"   # destructive hover
INFO       = "#7eb6ff"   # info accent
PURPLE     = "#b687f5"   # extra category accent (used sparingly)

# Aliases (semantic re-export)
BG_BASE     = BG_DEEP
BG_PANEL    = BG_CARD
BG_RAISED   = BG_ROW
BG_HOVER    = BG_ROW_A
INK         = TEXT_MAIN
INK_DIM     = TEXT_DIM
INK_MUTE    = TEXT_MUTE
SIGNAL      = ACCENT
OK          = GREEN
WARN        = ORANGE
DANGER      = RED_SOFT

# Geometry — small rounding (utility tool feel, not consumer app)
R_SHARP     = 3
R_BASE      = 5
R_LARGE     = 8

# Icon glyphs (Segoe UI Symbol / Segoe MDL2 Assets compatible — Windows native)
# Single-character symbols that render reliably on Windows. Keep this small.
ICON_OVERVIEW    = "◉"   # focus / info
ICON_DISK        = "◓"   # disk usage
ICON_CLEANUP     = "✦"   # sparkle
ICON_DUPLICATES  = "⊜"   # equal
ICON_LARGE       = "⬡"   # hex / large
ICON_PRIVACY     = "◐"   # half
ICON_PROGRAMS    = "▦"   # grid
ICON_STARTUP     = "▶"   # play
ICON_TASKS       = "◷"   # clock
ICON_NETWORK     = "⌬"   # network

ICON_HOME        = "⌂"   # house

TOOL_ICONS = {
    "Home":        ICON_HOME,
    "Overview":    ICON_OVERVIEW,
    "Disk Map":    ICON_DISK,
    "Cleanup":     ICON_CLEANUP,
    "Duplicates":  ICON_DUPLICATES,
    "Large Files": ICON_LARGE,
    "Privacy":     ICON_PRIVACY,
    "Programs":    ICON_PROGRAMS,
    "Startup":     ICON_STARTUP,
    "Tasks":       ICON_TASKS,
    "Network":     ICON_NETWORK,
}

# Colour-code each tool for the sidebar accent dot (multi-colour, not all amber)
TOOL_COLOURS = {
    "Home":        ACCENT,
    "Overview":    INFO,
    "Disk Map":    ACCENT,
    "Cleanup":     GREEN,
    "Duplicates":  PURPLE,
    "Large Files": ACCENT,
    "Privacy":     ORANGE,
    "Programs":    INFO,
    "Startup":     GREEN,
    "Tasks":       PURPLE,
    "Network":     INFO,
}

# Type scale — DPI-aware.
#
# We force CTk's own scaling to 1.0 (see GUI section) so geometry maps 1:1 to
# physical pixels and content can't overflow the window. The downside is that
# fonts then render at their raw pixel size — tiny on a high-DPI display. So we
# detect the real DPI here and scale fonts (and key widget dimensions) up by it.
# Base sizes are also generous for legibility (Craig has nystagmus + dyspraxia).
#
# Use the F_* helpers everywhere; never hand-roll `ctk.CTkFont(...)`.

# CTk auto-scales fonts/widgets for DPI, so these are RAW logical sizes and
# _px is a no-op. Base sizes are generous for legibility (nystagmus + dyspraxia).
DPI_SCALE = 1.0

def _px(n: int) -> int:
    return int(n)

def _font(size_pt: int, weight: str = "normal", family: str = "Segoe UI"):
    """Build a font at a logical point size — CTk applies DPI scaling."""
    return ctk.CTkFont(family, size_pt, weight)

def F_DISPLAY(): return _font(22, "bold")
def F_H1():      return _font(17, "bold")
def F_H2():      return _font(14, "bold")
def F_BODY():    return _font(13)
def F_BODY_B():  return _font(13, "bold")
def F_SMALL():   return _font(12)
def F_MICRO():   return _font(11)
def F_MONO(s=13):return _font(s, "normal", "Cascadia Mono")
def F_MONO_B(s=13):return _font(s, "bold", "Cascadia Mono")

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

def treeview_sort(tree: ttk.Treeview, col: str, reverse: bool = False):
    data = [(tree.set(k, col), k) for k in tree.get_children("")]
    try:
        data.sort(key=lambda x: float(x[0].replace(",", "").replace(" ", "")), reverse=reverse)
    except ValueError:
        data.sort(key=lambda x: x[0].lower(), reverse=reverse)
    for i, (_, k) in enumerate(data):
        tree.move(k, "", i)
    tree.heading(col, command=lambda: treeview_sort(tree, col, not reverse))

def _setup_treeview_style():
    # Fonts/row height scaled to physical pixels (CTk scaling is 1.0).
    style = ttk.Style()
    style.theme_use("clam")
    style.configure("Dark.Treeview",
                    background=BG_BASE, foreground=INK,
                    fieldbackground=BG_BASE, borderwidth=0,
                    font=("Segoe UI", -_px(11)), rowheight=_px(30))
    style.configure("Dark.Treeview.Heading",
                    background=BG_PANEL, foreground=INK_DIM,
                    borderwidth=0, relief="flat",
                    font=("Segoe UI", -_px(10), "bold"),
                    padding=(_px(8), _px(8)))
    style.map("Dark.Treeview",
              background=[("selected", ACCENT_SOFT)],
              foreground=[("selected", INK)])
    style.map("Dark.Treeview.Heading",
              background=[("active", BG_HOVER)],
              foreground=[("active", INK)])

def make_tree(parent, columns: list[tuple[str, int]], height: int = 20) -> tuple[ctk.CTkFrame, ttk.Treeview]:
    """Create a dark-themed Treeview with scrollbar. columns = [(name, width), ...]"""
    frame = ctk.CTkFrame(parent, fg_color="transparent")
    col_ids = [c[0] for c in columns]
    tree = ttk.Treeview(frame, columns=col_ids, show="headings",
                        style="Dark.Treeview", selectmode="extended", height=height)
    vsb = tk.Scrollbar(frame, orient="vertical", command=tree.yview,
                       bg=BG_CARD, troughcolor=BG_DEEP, bd=0, width=10)
    hsb = tk.Scrollbar(frame, orient="horizontal", command=tree.xview,
                       bg=BG_CARD, troughcolor=BG_DEEP, bd=0, width=10)
    tree.configure(yscrollcommand=vsb.set, xscrollcommand=hsb.set)
    for name, width in columns:
        tree.heading(name, text=name, command=lambda c=name: treeview_sort(tree, c))
        tree.column(name, width=width, minwidth=50)
    tree.grid(row=0, column=0, sticky="nsew")
    vsb.grid(row=0, column=1, sticky="ns")
    hsb.grid(row=1, column=0, sticky="ew")
    frame.grid_columnconfigure(0, weight=1)
    frame.grid_rowconfigure(0, weight=1)
    return frame, tree

def make_page_header(parent, title: str, subtitle: str = "",
                     action_text: str = None, action_cmd=None,
                     secondary_text: str = None, secondary_cmd=None) -> tuple:
    """Standard tool page header — title, subtitle, optional inline actions on the right.

    Uses pack() inside the header frame because grid + grid_propagate(False) was
    causing CTkLabel + CTkButton children to fail to render in some configurations.
    """
    hdr = ctk.CTkFrame(parent, fg_color="transparent")

    # Right side: actions packed from the right. Widths scale with DPI so
    # the buttons stay proportional to the larger DPI-scaled text.
    if action_text:
        ctk.CTkButton(hdr, text=action_text,
                      width=_px(100), height=_px(34),
                      font=_font(11, "bold", "Segoe UI"),
                      fg_color=ACCENT, hover_color="#3a8aef",
                      text_color="#0a1322", corner_radius=R_BASE,
                      command=action_cmd
                      ).pack(side="right", padx=(8, 0), pady=8)
    if secondary_text:
        ctk.CTkButton(hdr, text=secondary_text,
                      width=_px(100), height=_px(34),
                      font=_font(11, "normal", "Segoe UI"),
                      fg_color="transparent", hover_color=BG_HOVER,
                      text_color=INK_DIM,
                      border_width=1, border_color=BORDER,
                      corner_radius=R_BASE, command=secondary_cmd
                      ).pack(side="right", padx=(8, 0), pady=8)

    # Left side: title + subtitle packed left
    text_col = ctk.CTkFrame(hdr, fg_color="transparent")
    text_col.pack(side="left", fill="x", expand=True, pady=4)

    ctk.CTkLabel(text_col, text=title,
                 font=_font(20, "bold", "Segoe UI"),
                 text_color=INK, anchor="w"
                 ).pack(anchor="w")
    if subtitle:
        ctk.CTkLabel(text_col, text=subtitle,
                     font=_font(11, "normal", "Segoe UI"),
                     text_color=INK_DIM, anchor="w"
                     ).pack(anchor="w", pady=(2, 0))

    # Hairline under header
    rule = ctk.CTkFrame(parent, fg_color=BORDER, height=1)
    return hdr, rule


class _StringVarShim:
    """Adapter that lets the rest of the code call .get()/.delete()/.insert()
    on what used to be a CTkEntry but is now a hidden StringVar (the key entry
    moved into a Settings dialog)."""
    def __init__(self, var: ctk.StringVar):
        self._var = var
    def get(self) -> str:
        return self._var.get()
    def delete(self, *_args, **_kwargs):
        self._var.set("")
    def insert(self, _idx, value: str):
        self._var.set(value)


class Tile:
    """Reusable dashboard tile. The standard pattern across all tools:
    a panel with a small-caps title (with optional accent dot) and a body
    you can populate with KPIs, gauges, lists, or arbitrary widgets.

    Use one of the convenience methods (.kpi, .gauge, .row, .text) for
    common content, or pack/grid into `.body` directly for anything custom.
    """
    def __init__(self, parent, title: str, accent: str = None,
                 height: int = None, width: int = None,
                 padding_y: tuple = (12, 14)):
        self.frame = ctk.CTkFrame(parent,
                                   fg_color=BG_PANEL,
                                   corner_radius=R_BASE,
                                   border_width=1, border_color=BORDER,
                                   width=width if width else 0,
                                   height=height if height else 0)
        if height or width:
            self.frame.grid_propagate(False)
            self.frame.pack_propagate(False)

        # Header: optional accent dot + small-caps mono title
        header = ctk.CTkFrame(self.frame, fg_color="transparent")
        header.pack(fill="x", padx=14, pady=(padding_y[0], 6))
        if accent:
            ctk.CTkFrame(header, fg_color=accent,
                         width=8, height=8, corner_radius=4
                         ).pack(side="left", padx=(0, 8))
        ctk.CTkLabel(header, text=title.upper(),
                     font=_font(9, "bold", "Cascadia Mono"),
                     text_color=INK_MUTE
                     ).pack(side="left")

        # Body — caller adds content here
        self.body = ctk.CTkFrame(self.frame, fg_color="transparent")
        self.body.pack(fill="both", expand=True, padx=14, pady=(0, padding_y[1]))

    # ── Convenience content methods ──────────────────────────────────

    def kpi(self, value: str, sub: str = "", value_colour: str = None,
            value_size: int = 20):
        """Big number + optional subline below. Returns (value_lbl, sub_lbl)
        so callers can update the labels later via .configure(text=...)."""
        self.value_lbl = ctk.CTkLabel(self.body, text=value,
                                       font=_font(value_size, "bold"),
                                       text_color=value_colour or INK, anchor="w")
        self.value_lbl.pack(anchor="w")
        self.sub_lbl = ctk.CTkLabel(self.body, text=sub,
                                     font=_font(11), text_color=INK_DIM,
                                     anchor="w")
        self.sub_lbl.pack(anchor="w", pady=(4, 0))
        return self.value_lbl, self.sub_lbl

    def gauge(self, value: str, pct: float, colour: str, sub: str = ""):
        """Big value + thin progress bar + optional subline."""
        ctk.CTkLabel(self.body, text=value,
                     font=_font(16, "bold"),
                     text_color=INK, anchor="w"
                     ).pack(anchor="w")
        bar = ctk.CTkProgressBar(self.body, height=6,
                                  fg_color=BG_BASE,
                                  progress_color=colour,
                                  corner_radius=2)
        bar.set(max(0.0, min(1.0, pct)))
        bar.pack(fill="x", pady=(8, 0))
        if sub:
            ctk.CTkLabel(self.body, text=sub,
                         font=_font(11), text_color=INK_DIM, anchor="w"
                         ).pack(anchor="w", pady=(6, 0))

    def text(self, text: str, colour: str = None):
        """Plain paragraph text body."""
        ctk.CTkLabel(self.body, text=text,
                     font=_font(11), text_color=colour or INK_DIM,
                     anchor="w", justify="left"
                     ).pack(anchor="w", fill="x")

    def grid(self, **kwargs):  return self.frame.grid(**kwargs)
    def pack(self, **kwargs):  return self.frame.pack(**kwargs)


class ActionCard:
    """Fortect-style action row: a wide panel with a coloured icon chip, a
    title + description on the left, and one big prominent button on the right.

    Horizontal layout (rather than a 2-column grid of tall cards) because it
    packs more actions per screen and sidesteps CTk's high-DPI 2-column quirk.
    """
    def __init__(self, parent, icon: str, icon_colour: str,
                 title: str, description: str,
                 button_text: str, command,
                 button_colour: str = None):
        button_colour = button_colour or ACCENT
        self.frame = ctk.CTkFrame(parent, fg_color=BG_RAISED,
                                   corner_radius=R_LARGE,
                                   border_width=1, border_color=BORDER)

        # Icon chip on its own line (gives the title the full card width)
        chip = ctk.CTkFrame(self.frame, fg_color=self._tint(icon_colour),
                            width=48, height=48, corner_radius=R_BASE)
        chip.pack(anchor="w", padx=20, pady=(20, 0))
        chip.pack_propagate(False)
        ctk.CTkLabel(chip, text=icon,
                     font=_font(22, "normal", "Segoe UI"),
                     text_color=icon_colour).pack(expand=True)

        # Title + description. wraplength is set dynamically from the card's
        # real width via <Configure> so text always fits regardless of DPI
        # scaling or column width.
        self._title_lbl = ctk.CTkLabel(self.frame, text=title,
                     font=_font(17, "bold", "Segoe UI"),
                     text_color=INK, anchor="w", justify="left", wraplength=200)
        self._title_lbl.pack(anchor="w", fill="x", padx=20, pady=(12, 0))

        self._desc_lbl = ctk.CTkLabel(self.frame, text=description,
                     font=_font(12, "normal", "Segoe UI"),
                     text_color=INK_DIM, anchor="w", justify="left", wraplength=200)
        self._desc_lbl.pack(anchor="w", fill="x", padx=20, pady=(6, 0))

        self.frame.bind("<Configure>", self._on_resize)

        # Spacer pushes the button to the bottom for even card heights
        ctk.CTkFrame(self.frame, fg_color="transparent", height=4
                     ).pack(fill="both", expand=True)

        # Big prominent button, full card width
        ctk.CTkButton(self.frame, text=button_text,
                      font=_font(13, "bold", "Segoe UI"),
                      height=46, corner_radius=R_BASE,
                      fg_color=button_colour, hover_color=self._darken(button_colour),
                      text_color="#0a1322",
                      command=command
                      ).pack(fill="x", padx=20, pady=(16, 20))

    def _on_resize(self, event):
        # event.width is in scaled tk pixels; CTk multiplies wraplength by the
        # widget-scaling factor again, so divide it out to avoid double-scaling.
        try:
            scale = ctk.ScalingTracker.get_widget_scaling(self.frame)
        except Exception:
            scale = 1.0
        wrap = max(120, int(event.width / scale) - 40)
        self._title_lbl.configure(wraplength=wrap)
        self._desc_lbl.configure(wraplength=wrap)

    @staticmethod
    def _tint(hex_colour: str) -> str:
        """A very dark tint of the accent for the icon chip background."""
        tints = {
            ACCENT: "#13243a", GREEN: "#13301f", ORANGE: "#322611",
            PURPLE: "#241733", INFO: "#16273d", RED_SOFT: "#331717",
        }
        return tints.get(hex_colour, BG_PANEL)

    @staticmethod
    def _darken(hex_colour: str) -> str:
        darks = {
            ACCENT: "#3a8aef", GREEN: "#4cbb78", ORANGE: "#d99c2c",
            PURPLE: "#9f6fe0", INFO: "#5a9be0", RED_SOFT: "#e5605b",
        }
        return darks.get(hex_colour, hex_colour)

    def grid(self, **kwargs):  return self.frame.grid(**kwargs)
    def pack(self, **kwargs):  return self.frame.pack(**kwargs)


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

# ── Gemini ────────────────────────────────────────────────────────────────────

def ask_gemini(api_key: str, scan_results: list[ScanResult]) -> str:
    try:
        import google.generativeai as genai
    except ImportError:
        return "google-generativeai not installed.\nRun:  pip install google-generativeai"

    genai.configure(api_key=api_key)
    model = genai.GenerativeModel(GEMINI_MODEL)

    total = sum(r.total_bytes for r in scan_results)
    lines = "\n".join(f"- {r.label}: {fmt_size(r.total_bytes)} ({r.note})"
                      for r in scan_results)

    prompt = f"""You are a Windows system cleanup assistant. A user scanned their Windows 11 PC.
Give a concise, friendly analysis (max 280 words) covering:
1. Total potential space savings
2. Which categories are 100% safe to delete immediately
3. Which need a little care (e.g. large downloads worth reviewing)
4. Any gotchas or warnings
5. Recommended clean-up order

Be direct and practical — the user is technically capable.

Scan results (total: {fmt_size(total)}):
{lines}"""

    try:
        response = model.generate_content(prompt)
        return response.text
    except Exception as e:
        return f"Gemini error: {e}"

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

# ══════════════════════════════════════════════════════════════════════════════
# GUI
# ══════════════════════════════════════════════════════════════════════════════

ctk.set_appearance_mode("dark")
ctk.set_default_color_theme("blue")

# DPI FIX — two independent CTk scaling axes:
#   • window_scaling controls the WINDOW geometry (geometry × detected_dpi × this)
#   • widget_scaling controls FONT/WIDGET size (size × detected_dpi × this)
# Every earlier attempt only touched widget_scaling / deactivate_automatic_dpi.
# The real culprit was window geometry: CTk multiplied 1180 × 1.75 = 2065 logical
# px, wider than the ~1463-logical desktop at 175%, so ~40% spilled off-screen.
# Lowering window_scaling to ~1/DPI cancels that multiply (geometry lands back at
# the intended physical size and content fits), while widget_scaling stays high
# so fonts/buttons remain large and crisp (good for nystagmus).
_DPI = 1.0
try:
    _DPI = ctypes.windll.user32.GetDpiForSystem() / 96.0
except (AttributeError, OSError):
    pass
if _DPI > 1.05:
    ctk.set_window_scaling(1.0 / _DPI)   # cancel the geometry over-multiply
    ctk.set_widget_scaling(1.0)          # keep fonts/widgets at full DPI size

# ── Tool 1: Cleanup ──────────────────────────────────────────────────────────

class CleanupTool(ctk.CTkFrame):

    def __init__(self, master):
        super().__init__(master, fg_color="transparent")
        self.grid_columnconfigure(0, weight=1)
        # Row weight is set inside _build_ui — don't double-set here.

        self._results: list[ScanResult] = []
        self._row_vars: list[ctk.BooleanVar] = []
        self._row_frames: list[ctk.CTkFrame] = []
        self._scanning = False
        self._expand_states: list[bool] = []
        self._detail_frames: list[Optional[ctk.CTkFrame]] = []
        self._file_cache: list = []
        self._toggle_btns: list[ctk.CTkButton] = []

        self._build_ui()
        self._load_api_key()

    def _key_file(self) -> Path:
        base = Path(sys.executable).parent if getattr(sys, "frozen", False) \
               else Path(__file__).parent
        return base / ".winscan_key"

    def _load_api_key(self):
        key = os.environ.get("GEMINI_API_KEY", "").strip()
        if not key:
            try:
                key = self._key_file().read_text().strip()
            except (FileNotFoundError, OSError):
                pass
        if key:
            self._key_entry.delete(0, "end")
            self._key_entry.insert(0, key)

    def _save_api_key(self):
        key = self._key_entry.get().strip()
        if not key:
            self._log("No API key to save.")
            return
        try:
            self._key_file().write_text(key)
            self._log("API key saved to disk.")
        except OSError as e:
            self._log(f"Could not save key: {e}")

    def _build_ui(self):
        # SINGLE-COLUMN, MINIMAL LAYOUT. No stat cards or side panels — those
        # had unsolvable overflow issues with CTk's DPI scaling.
        # Rows:
        #   0 page header (title + Settings/Scan)
        #   1 hairline
        #   2 inline status row + progress bar
        #   3 AI summary block (hidden until scan completes)
        #   4 findings panel ← grows
        #   5 action bar
        #   6 log strip (collapsible)
        self.grid_rowconfigure(4, weight=1)

        # ── Page header (title only — buttons moved to action row) ──────
        hdr, rule = make_page_header(
            self,
            title="Cleanup",
            subtitle="Temp files, caches, old downloads.",
        )
        hdr.grid(row=0, column=0, sticky="ew")
        rule.grid(row=1, column=0, sticky="ew", pady=(2, 10))

        self._key_var = ctk.StringVar()
        self._key_entry = _StringVarShim(self._key_var)

        # ── Action / status row ──────────────────────────────────────────
        # Layout (all left-aligned so buttons are guaranteed visible):
        #   [Scan] [Settings]  ●  Idle — click Scan        [stats]
        # The action buttons go FIRST so they're always at x=0 — Tk's
        # right-side packing was being rendered off-window on high-DPI displays.
        status_row = ctk.CTkFrame(self, fg_color="transparent")
        status_row.grid(row=2, column=0, sticky="ew", pady=(0, 14))

        self._scan_btn = ctk.CTkButton(
            status_row, text="Scan",
            width=100, height=34,
            font=_font(12, "bold", "Segoe UI"),
            fg_color=ACCENT, hover_color="#3a8aef",
            text_color="#0a1322", corner_radius=R_BASE,
            command=self._start_scan)
        self._scan_btn.pack(side="left")

        ctk.CTkButton(
            status_row, text="Settings",
            width=100, height=34,
            font=_font(11, "normal", "Segoe UI"),
            fg_color="transparent", hover_color=BG_HOVER,
            text_color=INK_DIM,
            border_width=1, border_color=BORDER,
            corner_radius=R_BASE, command=self._open_settings,
        ).pack(side="left", padx=(8, 18))

        # Status dot
        self._status_dot = ctk.CTkLabel(status_row, text="●",
                                         font=_font(14, "bold", "Segoe UI"),
                                         text_color=INK_MUTE)
        self._status_dot.pack(side="left", padx=(0, 8))

        self._status_lbl = ctk.CTkLabel(status_row,
                                         text="Idle — click Scan",
                                         font=_font(12, "normal", "Segoe UI"),
                                         text_color=INK_DIM, anchor="w")
        self._status_lbl.pack(side="left")

        # Compact stat pills appear after a successful scan
        self._stat_total = ctk.CTkLabel(status_row, text="",
                                          font=_font(12, "bold", "Cascadia Mono"),
                                          text_color=GREEN, anchor="w")
        self._stat_total.pack(side="left", padx=(16, 0))
        self._stat_count = ctk.CTkLabel(status_row, text="",
                                          font=_font(12, "normal", "Cascadia Mono"),
                                          text_color=INK_DIM, anchor="w")
        self._stat_count.pack(side="left", padx=(8, 0))

        # Progress bar (hairline) — hidden by default
        self._progress = ctk.CTkProgressBar(self, height=3,
                                             fg_color=BG_PANEL,
                                             progress_color=ACCENT,
                                             corner_radius=2)
        self._progress.set(0)
        # Not gridded until a scan starts

        # ── AI summary block — initially collapsed ───────────────────────
        self._ai_frame = ctk.CTkFrame(self, fg_color=BG_PANEL,
                                       corner_radius=R_BASE,
                                       border_width=1, border_color=BORDER)
        # Don't grid yet — added by _show_ai when content arrives

        ai_hdr = ctk.CTkFrame(self._ai_frame, fg_color="transparent")
        ai_hdr.pack(fill="x", padx=14, pady=(10, 6))

        ctk.CTkLabel(ai_hdr, text=GEMINI_MODEL,
                     font=_font(8, "normal", "Cascadia Mono"),
                     text_color=INK_MUTE
                     ).pack(side="right")

        ai_title_row = ctk.CTkFrame(ai_hdr, fg_color="transparent")
        ai_title_row.pack(side="left")
        ctk.CTkFrame(ai_title_row, fg_color=PURPLE, width=6, height=6,
                     corner_radius=3).pack(side="left", padx=(0, 6))
        ctk.CTkLabel(ai_title_row, text="GEMINI'S READ",
                     font=_font(8, "bold", "Cascadia Mono"),
                     text_color=INK_DIM
                     ).pack(side="left")

        self._ai_box = ctk.CTkTextbox(self._ai_frame, height=110,
                                       font=_font(11, "normal", "Segoe UI"),
                                       fg_color=BG_PANEL, text_color=INK,
                                       wrap="word", state="disabled",
                                       border_width=0,
                                       scrollbar_button_color=BG_RAISED,
                                       scrollbar_button_hover_color=BG_HOVER)
        self._ai_box.pack(fill="x", padx=14, pady=(0, 12))
        self._ai_write("Gemini analysis will appear here after scanning.")

        # ── Findings panel — full width, grows ──────────────────────────
        res_panel = ctk.CTkFrame(self, fg_color=BG_PANEL,
                                  corner_radius=R_BASE,
                                  border_width=1, border_color=BORDER)
        res_panel.grid(row=4, column=0, sticky="nsew")
        res_panel.grid_columnconfigure(0, weight=1)
        res_panel.grid_rowconfigure(2, weight=1)

        # Panel header
        res_hdr = ctk.CTkFrame(res_panel, fg_color="transparent")
        res_hdr.grid(row=0, column=0, sticky="ew", padx=16, pady=(12, 10))

        ctk.CTkButton(res_hdr, text="none", width=44, height=22,
                      font=_font(9, "normal", "Segoe UI"),
                      fg_color="transparent",
                      hover_color=BG_HOVER, text_color=INK_DIM,
                      border_width=1, border_color=BORDER,
                      corner_radius=R_SHARP,
                      command=lambda: self._select_all(False)
                      ).pack(side="right")
        ctk.CTkButton(res_hdr, text="all", width=36, height=22,
                      font=_font(9, "normal", "Segoe UI"),
                      fg_color="transparent",
                      hover_color=BG_HOVER, text_color=INK_DIM,
                      border_width=1, border_color=BORDER,
                      corner_radius=R_SHARP,
                      command=lambda: self._select_all(True)
                      ).pack(side="right", padx=(0, 4))
        self._res_title = ctk.CTkLabel(res_hdr, text="FINDINGS",
                                        font=_font(9, "bold", "Cascadia Mono"),
                                        text_color=INK_DIM, anchor="w")
        self._res_title.pack(side="left")

        # Hairline under header
        ctk.CTkFrame(res_panel, fg_color=BORDER, height=1
                     ).grid(row=1, column=0, sticky="ew")

        self._scroll = ctk.CTkScrollableFrame(res_panel, fg_color="transparent",
                                               scrollbar_button_color=BG_RAISED,
                                               scrollbar_button_hover_color=BG_HOVER)
        self._scroll.grid(row=2, column=0, sticky="nsew", padx=8, pady=8)
        self._scroll.grid_columnconfigure(0, weight=1)

        self._empty_lbl = ctk.CTkLabel(
            self._scroll,
            text="No scan yet.\nClick Scan in the top right to begin.",
            font=_font(11, "normal", "Segoe UI"), text_color=INK_MUTE,
            justify="center", wraplength=400)
        self._empty_lbl.grid(row=0, column=0, pady=72)


        # ── Action bar (pack-based) ──────────────────────────────────────
        act = ctk.CTkFrame(self, fg_color="transparent")
        act.grid(row=5, column=0, sticky="ew", pady=(12, 0))

        # Right side: destructive primary, packed last (rightmost)
        self._del_btn = ctk.CTkButton(
            act, text="Delete selected",
            font=_font(11, "bold", "Segoe UI"),
            height=32, width=140,
            fg_color=RED_BTN, hover_color=RED_BTN_H,
            text_color="#ffffff", corner_radius=R_BASE,
            state="disabled", command=self._confirm_delete)
        self._del_btn.pack(side="right", pady=4)

        # Log toggle
        self._log_visible = False
        self._log_toggle = ctk.CTkButton(
            act, text="show log", width=80, height=28,
            font=_font(9, "normal", "Segoe UI"),
            fg_color="transparent",
            hover_color=BG_HOVER, text_color=INK_MUTE,
            border_width=1, border_color=BORDER,
            corner_radius=R_SHARP, command=self._toggle_log)
        self._log_toggle.pack(side="right", padx=(0, 10), pady=4)

        # Left: selection summary
        self._total_lbl = ctk.CTkLabel(act, text="No selection",
                                        font=_font(11, "bold", "Cascadia Mono"),
                                        text_color=INK_DIM, anchor="w")
        self._total_lbl.pack(side="left", pady=4)

        # ── Log strip (hidden by default) ─────────────────────────────────
        self._log_box = ctk.CTkTextbox(self, height=80,
                                        font=F_MONO(9),
                                        fg_color=BG_PANEL, text_color=INK_DIM,
                                        state="disabled", corner_radius=R_BASE,
                                        border_width=1, border_color=BORDER,
                                        scrollbar_button_color=BG_RAISED)

    def _find_header_button(self, hdr, text: str):
        """Walk the header frame to find the button matching `text`."""
        for child in hdr.winfo_children():
            if isinstance(child, ctk.CTkButton) and child.cget("text") == text:
                return child
            for grand in child.winfo_children():
                if isinstance(grand, ctk.CTkButton) and grand.cget("text") == text:
                    return grand
        return None

    def _toggle_log(self):
        self._log_visible = not self._log_visible
        if self._log_visible:
            self._log_box.grid(row=6, column=0, sticky="ew", pady=(8, 0))
            self._log_toggle.configure(text="hide log")
        else:
            self._log_box.grid_remove()
            self._log_toggle.configure(text="show log")

    def _open_settings(self):
        """Modal-ish settings popover for the Gemini API key."""
        if hasattr(self, "_settings_win") and self._settings_win is not None \
                and self._settings_win.winfo_exists():
            self._settings_win.lift()
            return

        win = ctk.CTkToplevel(self)
        self._settings_win = win
        win.title("WinScan — Settings")
        win.geometry("460x220")
        win.configure(fg_color=BG_BASE)
        win.resizable(False, False)
        win.transient(self.winfo_toplevel())

        # Header
        ctk.CTkLabel(win, text="Gemini API key",
                     font=F_H2(), text_color=INK
                     ).pack(anchor="w", padx=20, pady=(20, 4))
        ctk.CTkLabel(win,
                     text="Used only on the Cleanup tab. Stored locally in .winscan_key.",
                     font=F_SMALL(), text_color=INK_DIM, justify="left", anchor="w"
                     ).pack(anchor="w", padx=20, pady=(0, 12))

        row = ctk.CTkFrame(win, fg_color="transparent")
        row.pack(fill="x", padx=20)
        row.grid_columnconfigure(0, weight=1)

        entry = ctk.CTkEntry(row, show="*", placeholder_text="Paste key…",
                              font=F_MONO(10), height=32,
                              fg_color=BG_PANEL, border_color=BORDER,
                              text_color=INK, corner_radius=R_BASE)
        entry.grid(row=0, column=0, sticky="ew")
        entry.insert(0, self._key_var.get())

        def toggle_show():
            entry.configure(show="" if entry.cget("show") == "*" else "*")
        ctk.CTkButton(row, text="show", width=50, height=32, font=F_MICRO(),
                      fg_color=BG_PANEL, hover_color=BG_HOVER,
                      text_color=INK_DIM, border_width=1, border_color=BORDER,
                      corner_radius=R_BASE, command=toggle_show
                      ).grid(row=0, column=1, padx=(6, 0))

        # Actions
        actions = ctk.CTkFrame(win, fg_color="transparent")
        actions.pack(fill="x", padx=20, pady=(16, 16), side="bottom")

        def save_and_close():
            self._key_var.set(entry.get().strip())
            self._save_api_key()
            win.destroy()

        ctk.CTkButton(actions, text="Cancel", width=80, height=30, font=F_BODY(),
                      fg_color="transparent", hover_color=BG_HOVER,
                      text_color=INK_DIM, border_width=1, border_color=BORDER,
                      corner_radius=R_BASE, command=win.destroy
                      ).pack(side="right", padx=(8, 0))
        ctk.CTkButton(actions, text="Save", width=80, height=30, font=F_BODY_B(),
                      fg_color=ACCENT, hover_color="#d99c2c",
                      text_color="#1a1408", corner_radius=R_BASE,
                      command=save_and_close
                      ).pack(side="right")

    def _start_scan(self):
        if self._scanning:
            return
        self._scanning = True
        if self._scan_btn:
            self._scan_btn.configure(state="disabled", text="Scanning…")
        self._del_btn.configure(state="disabled")
        self._clear_results()
        self._ai_write("Scan in progress — Gemini's read appears once it finishes.")
        self._status_dot.configure(text="●", text_color=ACCENT)
        self._status_lbl.configure(text="Scanning your drive…", text_color=INK)
        self._stat_total.configure(text="")
        self._stat_count.configure(text="")
        # Show progress bar at the bottom of the status row
        self._progress.grid(row=3, column=0, sticky="ew", pady=(0, 10))
        self._progress.configure(mode="indeterminate")
        self._progress.start()
        threading.Thread(target=self._scan_worker, daemon=True).start()

    def _scan_worker(self):
        def cb(msg):
            self.after(0, lambda m=msg: self._status_lbl.configure(text=m))
        results = scan_all(log_cb=cb)
        self.after(0, lambda: self._on_scan_done(results))

    def _on_scan_done(self, results: list[ScanResult]):
        self._progress.stop()
        self._progress.grid_remove()
        self._scanning = False
        if self._scan_btn:
            self._scan_btn.configure(state="normal", text="Scan")
        self._results = results
        if not results:
            self._status_dot.configure(text="✓", text_color=GREEN)
            self._status_lbl.configure(text="System looks tidy — nothing to clean.",
                                        text_color=INK_DIM)
            self._log("Scan complete. Nothing to clean up.")
            return
        total = sum(r.total_bytes for r in results)

        self._status_dot.configure(text="✓", text_color=GREEN)
        self._status_lbl.configure(text=f"Scan complete",
                                    text_color=INK_DIM)
        self._stat_total.configure(text=fmt_size(total), text_color=GREEN)
        self._stat_count.configure(text=f"{len(results)} categories  ·  ", text_color=INK_DIM)

        self._log(f"Scan complete — {len(results)} categories, {fmt_size(total)} recoverable.")
        self._show_results(results)

        key = self._key_entry.get().strip()
        self._ai_frame.grid(row=3, column=0, sticky="ew", pady=(0, 12))
        if key:
            self._ai_write("Asking Gemini for analysis…")
            threading.Thread(target=self._gemini_worker, args=(key, results), daemon=True).start()
        else:
            self._ai_write("Add your Gemini API key in Settings to get an AI summary of these findings.")

    def _clear_results(self):
        for f in self._row_frames:
            f.destroy()
        for f in self._detail_frames:
            if f is not None:
                f.destroy()
        self._row_frames.clear()
        self._row_vars.clear()
        self._expand_states.clear()
        self._detail_frames.clear()
        self._file_cache.clear()
        self._toggle_btns.clear()
        self._empty_lbl.grid(row=0, column=0, pady=72)
        self._res_title.configure(text="FINDINGS")
        self._total_lbl.configure(text="No selection")
        self._del_btn.configure(state="disabled")
        # Reset inline stats
        self._stat_total.configure(text="")
        self._stat_count.configure(text="")
        # Hide AI block until next scan
        try:
            self._ai_frame.grid_remove()
        except Exception:
            pass

    @staticmethod
    def _risk_for(label: str) -> tuple[str, str]:
        """Classify a scan result's risk for colour-coding. Returns (colour, tag)."""
        low = label.lower()
        # Safe (green): caches, temp, prefetch — system regenerates these
        safe_keywords = ("temp", "cache", "prefetch", "thumbnail", "update",
                         "crash dump", "error report", "package cache",
                         "nuget", "npm", "pip")
        # Caution (amber): user-touchable content — recycle bin, logs
        caution_keywords = ("recycle bin", "log files")
        # Review (red): large downloads — user data, manual review needed
        danger_keywords = ("large old", "downloads")

        if any(k in low for k in danger_keywords):
            return RED_SOFT, "REVIEW"
        if any(k in low for k in caution_keywords):
            return ORANGE, "CAUTION"
        if any(k in low for k in safe_keywords):
            return GREEN, "SAFE"
        return INK_MUTE, "—"

    def _show_results(self, results: list[ScanResult]):
        self._empty_lbl.grid_remove()
        for i, r in enumerate(results):
            var = ctk.BooleanVar(value=True)
            self._row_vars.append(var)
            risk_colour, risk_tag = self._risk_for(r.label)

            row = ctk.CTkFrame(self._scroll, fg_color=BG_RAISED,
                                corner_radius=R_BASE,
                                border_width=1, border_color=BORDER)
            row.grid(row=i * 2, column=0, sticky="ew", pady=3, padx=0)
            row.grid_columnconfigure(2, weight=1)
            self._row_frames.append(row)

            # Left colour stripe — risk indicator at a glance
            stripe = ctk.CTkFrame(row, fg_color=risk_colour,
                                   width=_px(4), corner_radius=2)
            stripe.grid(row=0, column=0, rowspan=2, sticky="ns",
                         padx=(8, 0), pady=10)

            # Checkbox
            cb = ctk.CTkCheckBox(row, text="", variable=var,
                                  width=_px(22),
                                  checkbox_width=_px(18),
                                  checkbox_height=_px(18),
                                  border_width=1,
                                  fg_color=ACCENT, hover_color=ACCENT_DIM,
                                  border_color=BORDER, corner_radius=R_SHARP,
                                  command=self._update_total)
            cb.grid(row=0, column=1, rowspan=2, padx=(10, 12), pady=10)

            # Title row: name + risk tag
            title_row = ctk.CTkFrame(row, fg_color="transparent")
            title_row.grid(row=0, column=2, sticky="w", padx=0, pady=(10, 0))
            ctk.CTkLabel(title_row, text=r.label, font=F_BODY_B(),
                         text_color=INK, anchor="w"
                         ).pack(side="left")
            # Risk tag chip (small caps mono label after the name)
            ctk.CTkLabel(title_row, text=risk_tag,
                         font=_font(8, "bold", "Cascadia Mono"),
                         text_color=risk_colour, anchor="w"
                         ).pack(side="left", padx=(10, 0))

            # Subtitle
            ctk.CTkLabel(row, text=r.note, font=F_SMALL(),
                         text_color=INK_DIM, anchor="w"
                         ).grid(row=1, column=2, sticky="w", padx=0, pady=(0, 10))

            # Size on the right (monospace + colour by size)
            ctk.CTkLabel(row, text=fmt_size(r.total_bytes),
                         font=F_MONO_B(13),
                         text_color=size_color(r.total_bytes)
                         ).grid(row=0, column=3, rowspan=2, padx=(8, 4), pady=10)

            # Expand toggle
            toggle_btn = ctk.CTkButton(
                row, text="›", width=_px(28), height=_px(28),
                font=_font(16, "bold"), fg_color="transparent",
                hover_color=BG_HOVER, text_color=INK_MUTE,
                corner_radius=R_SHARP,
                command=lambda idx=i: self._toggle_detail(idx))
            toggle_btn.grid(row=0, column=4, rowspan=2, padx=(4, 8), pady=10)
            self._toggle_btns.append(toggle_btn)
            self._expand_states.append(False)
            self._detail_frames.append(None)
            self._file_cache.append(None)
        self._res_title.configure(text=f"FINDINGS · {len(results)}")
        self._update_total()

    def _select_all(self, state: bool):
        for v in self._row_vars:
            v.set(state)
        self._update_total()

    def _update_total(self):
        total = sum(r.total_bytes for r, v in zip(self._results, self._row_vars) if v.get())
        count = sum(1 for v in self._row_vars if v.get())
        if count:
            self._total_lbl.configure(
                text=f"  {count:>2} selected   →   {fmt_size(total)}",
                text_color=INK)
            self._del_btn.configure(state="normal")
        else:
            self._total_lbl.configure(text="No selection", text_color=INK_MUTE)
            self._del_btn.configure(state="disabled")

    def _enumerate_files(self, result: ScanResult) -> tuple:
        files, total, errors = [], 0, 0
        is_dir_based = bool(result.paths) and result.paths[0].is_dir()
        if is_dir_based:
            for base_dir in result.paths:
                try:
                    for entry in base_dir.rglob("*"):
                        try:
                            if entry.is_file(follow_symlinks=False):
                                total += 1
                                if len(files) < DRILL_DOWN_CAP:
                                    try:
                                        size = entry.stat().st_size
                                    except OSError:
                                        size, errors = 0, errors + 1
                                    files.append((entry, size))
                        except (PermissionError, OSError):
                            errors += 1
                except (PermissionError, OSError):
                    errors += 1
        else:
            total = len(result.paths)
            for p in result.paths[:DRILL_DOWN_CAP]:
                try:
                    size = p.stat().st_size
                except OSError:
                    size, errors = 0, errors + 1
                files.append((p, size))
        return files, total, errors

    def _toggle_detail(self, idx: int):
        if self._expand_states[idx]:
            self._detail_frames[idx].grid_remove()
            self._expand_states[idx] = False
            self._toggle_btns[idx].configure(text="▶")
        else:
            if self._detail_frames[idx] is None:
                files, total, errors = self._enumerate_files(self._results[idx])
                self._file_cache[idx] = (files, total, errors)
                self._detail_frames[idx] = self._build_detail_frame(idx, files, total, errors)
            else:
                self._detail_frames[idx].grid()
            self._expand_states[idx] = True
            self._toggle_btns[idx].configure(text="▼")

    def _open_file_location(self, path_str: str):
        try:
            p = Path(path_str)
            if p.exists():
                subprocess.Popen(["explorer", "/select,", str(p)])
            elif p.parent.exists():
                subprocess.Popen(["explorer", str(p.parent)])
        except OSError:
            pass

    def _build_detail_frame(self, idx, files, total, errors):
        frame = ctk.CTkFrame(self._scroll, fg_color=BG_CARD, corner_radius=4)
        frame.grid(row=idx * 2 + 1, column=0, sticky="ew", padx=(28, 2), pady=(0, 4))
        frame.grid_columnconfigure(0, weight=1)

        notice = f"Showing {DRILL_DOWN_CAP:,} of {total:,} files" if total > DRILL_DOWN_CAP \
                 else f"{total:,} file{'s' if total != 1 else ''}"
        if errors:
            notice += f"  ·  {errors:,} skipped (access denied)"
        notice += "     click a file to open its location"

        ctk.CTkLabel(frame, text=notice, font=F_SMALL(),
                     text_color=INK_DIM, anchor="w"
                     ).grid(row=0, column=0, columnspan=2, sticky="w", padx=8, pady=(4, 2))

        txt = tk.Text(frame, height=10, bg=BG_BASE, fg=INK_DIM,
                      font=("Consolas", -int(round(12 * DPI_SCALE))), relief="flat", bd=0,
                      highlightthickness=0, selectbackground=ACCENT_DIM,
                      wrap="none", state="normal", cursor="arrow")
        vsb = tk.Scrollbar(frame, orient="vertical", command=txt.yview,
                           bg=BG_CARD, troughcolor=BG_DEEP, bd=0, width=10)
        txt.configure(yscrollcommand=vsb.set)
        txt.grid(row=1, column=0, sticky="nsew", padx=(8, 0), pady=(0, 4))
        vsb.grid(row=1, column=1, sticky="ns", padx=(0, 4), pady=(0, 4))

        txt.tag_configure("path", foreground=TEXT_DIM)
        txt.tag_configure("size", foreground=TEXT_DIM)
        for i, (p, size) in enumerate(files):
            tag_name = f"f{i}"
            txt.tag_configure(tag_name)
            path_str = str(p)
            txt.insert("end", f"  {fmt_size(size):>10}   ", "size")
            txt.insert("end", f"{path_str}\n", ("path", tag_name))
            txt.tag_bind(tag_name, "<Enter>",
                         lambda e, t=tag_name: txt.tag_configure(t, foreground=ACCENT, underline=True))
            txt.tag_bind(tag_name, "<Leave>",
                         lambda e, t=tag_name: txt.tag_configure(t, foreground=TEXT_DIM, underline=False))
            txt.tag_bind(tag_name, "<Button-1>",
                         lambda e, ps=path_str: self._open_file_location(ps))
        txt.configure(state="disabled", cursor="hand2")
        return frame

    def _gemini_worker(self, key, results):
        self._log("Asking Gemini for analysis…")
        text = ask_gemini(key, results)
        def on_done():
            self._ai_write(text)
            ok = not text.lower().startswith(("gemini error", "google-generativeai"))
            self._log("Gemini analysis received." if ok else "Gemini call failed.")
        self.after(0, on_done)

    def _ai_write(self, text):
        self._ai_box.configure(state="normal")
        self._ai_box.delete("1.0", "end")
        self._ai_box.insert("end", text)
        self._ai_box.configure(state="disabled")

    def _confirm_delete(self):
        selected = [(r, v) for r, v in zip(self._results, self._row_vars) if v.get()]
        if not selected:
            return
        total = sum(r.total_bytes for r, _ in selected)
        names = "\n".join(f"  - {r.label}  ({fmt_size(r.total_bytes)})" for r, _ in selected)
        if messagebox.askyesno("Confirm Deletion",
            f"Permanently delete {len(selected)} categories ({fmt_size(total)})?\n\n{names}\n\n"
            "This cannot be undone.", icon="warning"):
            self._del_btn.configure(state="disabled", text="Deleting…")
            self._scan_btn.configure(state="disabled")
            threading.Thread(target=self._delete_worker,
                             args=([r for r, _ in selected],), daemon=True).start()

    def _delete_worker(self, selected):
        total_freed = 0
        for r in selected:
            self.after(0, lambda lbl=r.label: self._log(f"Deleting: {lbl} …"))
            freed, ok, fail = delete_category(r)
            total_freed += freed
            if fail:
                self.after(0, lambda lbl=r.label, f=freed, fn=fail:
                           self._log(f"{lbl} — {fmt_size(f)} freed, {fn} skipped"))
            else:
                self.after(0, lambda lbl=r.label, f=freed:
                           self._log(f"{lbl} — {fmt_size(f)} freed"))
        self.after(0, lambda: self._on_delete_done(total_freed))

    def _on_delete_done(self, total_freed):
        self._del_btn.configure(text="  Delete Selected  ")
        self._scan_btn.configure(state="normal")
        self._log(f"Done! {fmt_size(total_freed)} freed. Run another scan to refresh.")
        messagebox.showinfo("Cleanup Complete", f"{fmt_size(total_freed)} freed from disk.")

    def _log(self, msg):
        self._log_box.configure(state="normal")
        self._log_box.insert("end", msg + "\n")
        self._log_box.see("end")
        self._log_box.configure(state="disabled")


# ── Tool 2: System Info ──────────────────────────────────────────────────────

class SystemInfoTool(ctk.CTkFrame):
    """Tiled dashboard: KPI tiles for OS/CPU/RAM/GPU/Uptime/Identity at the
    top, then a STORAGE section with one tile per drive."""

    def __init__(self, master):
        super().__init__(master, fg_color="transparent")
        self.grid_columnconfigure(0, weight=1)
        self._loaded = False

        hdr, rule = make_page_header(
            self, title="Overview",
            subtitle="System snapshot at a glance.")
        hdr.grid(row=0, column=0, sticky="ew")
        rule.grid(row=1, column=0, sticky="ew", pady=(2, 10))

        # Action row: Refresh button (kept consistent with Cleanup's pattern)
        action_row = ctk.CTkFrame(self, fg_color="transparent")
        action_row.grid(row=2, column=0, sticky="ew", pady=(0, 14))
        ctk.CTkButton(action_row, text="Refresh",
                      width=100, height=34,
                      font=_font(11, "bold", "Segoe UI"),
                      fg_color=ACCENT, hover_color="#3a8aef",
                      text_color="#0a1322", corner_radius=R_BASE,
                      command=self._load
                      ).pack(side="left")
        ctk.CTkLabel(action_row, text="Click Refresh to re-sample live values.",
                     font=_font(11), text_color=INK_DIM
                     ).pack(side="left", padx=14)

        # Scrollable body — tiles stack vertically.
        self._body = ctk.CTkScrollableFrame(self, fg_color="transparent",
                                             scrollbar_button_color=BG_RAISED,
                                             scrollbar_button_hover_color=BG_HOVER)
        self._body.grid(row=3, column=0, sticky="nsew")
        self._body.grid_columnconfigure(0, weight=1)
        self.grid_rowconfigure(3, weight=1)

    def _on_show(self):
        if not self._loaded:
            self._load()

    def _load(self):
        for w in self._body.winfo_children():
            w.destroy()
        self._loaded = True

        import platform

        # Tiles are placed in pack-based rows below. Skip the grid container.

        # Data gathering
        os_ver = f"Windows {platform.version()}"
        build  = platform.win32_ver()[1] if hasattr(platform, "win32_ver") else ""
        cpu    = platform.processor() or "Unknown"
        cpu    = cpu[:32] + "…" if len(cpu) > 32 else cpu
        cores  = os.cpu_count() or 0
        try:
            total_ram, avail_ram = _get_ram_info()
            used_ram = total_ram - avail_ram
            ram_pct  = used_ram / total_ram if total_ram else 0
        except Exception:
            total_ram = avail_ram = used_ram = 0
            ram_pct = 0
        try:
            uptime = _get_uptime_str()
        except Exception:
            uptime = "Unknown"
        hostname = os.environ.get("COMPUTERNAME", "Unknown")
        username = os.environ.get("USERNAME", "Unknown")
        gpu = "Unknown"
        try:
            r = subprocess.run(
                ["powershell", "-NoProfile", "-Command",
                 "(Get-CimInstance Win32_VideoController).Name"],
                capture_output=True, text=True, timeout=5,
                creationflags=0x08000000)
            gpu = r.stdout.strip().split("\n")[0] or "Unknown"
        except Exception:
            pass
        gpu_short = gpu[:32] + "…" if len(gpu) > 32 else gpu

        ram_colour = GREEN if ram_pct < 0.7 else ORANGE if ram_pct < 0.9 else RED_SOFT

        # Single-column tile stack — each tile is full content width.
        # Reliable across all CTk + DPI quirks, and reads cleanly as a
        # Datadog-style status dashboard.

        t = Tile(self._body, "Operating system", accent=INFO)
        t.kpi("Windows 11", f"Build {build}" if build else os_ver, value_size=20)
        t.frame.pack(fill="x", pady=(0, 8))

        t = Tile(self._body, "Processor", accent=ACCENT)
        t.kpi(cpu, f"{cores} logical cores", value_size=16)
        t.frame.pack(fill="x", pady=(0, 8))

        t = Tile(self._body, "Memory", accent=ram_colour)
        if total_ram:
            t.gauge(f"{fmt_size(used_ram)} / {fmt_size(total_ram)}",
                     pct=ram_pct, colour=ram_colour,
                     sub=f"{ram_pct*100:.0f}% used  ·  {fmt_size(avail_ram)} free")
        else:
            t.kpi("Unknown", "")
        t.frame.pack(fill="x", pady=(0, 8))

        t = Tile(self._body, "Graphics", accent=PURPLE)
        t.kpi(gpu_short, "", value_size=16)
        t.frame.pack(fill="x", pady=(0, 8))

        t = Tile(self._body, "Uptime", accent=GREEN)
        t.kpi(uptime, "since last boot", value_size=18)
        t.frame.pack(fill="x", pady=(0, 8))

        t = Tile(self._body, "Identity", accent=INK_DIM)
        t.kpi(hostname, f"signed in as {username}", value_size=16)
        t.frame.pack(fill="x", pady=(0, 8))

        # ── Storage section ─────────────────────────────────────────────
        ctk.CTkLabel(self._body, text="STORAGE",
                     font=_font(9, "bold", "Cascadia Mono"),
                     text_color=INK_MUTE
                     ).pack(anchor="w", pady=(8, 8))

        for letter in "CDEFGHIJKLMNOP":
            drive = f"{letter}:\\"
            if not os.path.exists(drive):
                continue
            try:
                usage = shutil.disk_usage(drive)
            except OSError:
                continue
            pct = usage.used / usage.total if usage.total else 0
            colour = GREEN if pct < 0.7 else ORANGE if pct < 0.9 else RED_SOFT

            t = Tile(self._body, f"Drive {letter}:", accent=colour,
                     padding_y=(10, 12))
            t.gauge(f"{fmt_size(usage.used)}  /  {fmt_size(usage.total)}",
                     pct=pct, colour=colour,
                     sub=f"{pct*100:.0f}% used  ·  {fmt_size(usage.free)} free")
            t.frame.pack(fill="x", pady=(0, 6))


# ── Tool 3: Large File Hunter ────────────────────────────────────────────────

class LargeFileTool(ctk.CTkFrame):

    def __init__(self, master):
        super().__init__(master, fg_color="transparent")
        self.grid_columnconfigure(0, weight=1)
        self.grid_rowconfigure(5, weight=1)
        self._scanning = False

        hdr, rule = make_page_header(
            self, title="Large Files",
            subtitle="Find heavy files anywhere on disk.")
        hdr.grid(row=0, column=0, sticky="ew")
        rule.grid(row=1, column=0, sticky="ew", pady=(2, 10))

        # Action row
        actions = ctk.CTkFrame(self, fg_color="transparent")
        actions.grid(row=2, column=0, sticky="ew", pady=(0, 12))
        self._scan_btn = ctk.CTkButton(actions, text="Scan",
                      width=100, height=34, font=_font(11, "bold"),
                      fg_color=ACCENT, hover_color="#3a8aef",
                      text_color="#0a1322", corner_radius=R_BASE,
                      command=self._start_scan)
        self._scan_btn.pack(side="left")
        self._status = ctk.CTkLabel(actions,
                                     text="Set a path and a minimum size, then click Scan.",
                                     font=_font(11), text_color=INK_DIM)
        self._status.pack(side="left", padx=14)

        # Summary tile
        self._tile = Tile(self, "Large files", accent=ACCENT)
        self._tile.kpi("—", "no scan yet", value_size=20)
        self._tile.frame.grid(row=3, column=0, sticky="ew", pady=(0, 12))

        # Path + min controls (compact row under the tile)
        ctrl = ctk.CTkFrame(self, fg_color="transparent")
        ctrl.grid(row=4, column=0, sticky="ew", pady=(0, 10))
        ctrl.grid_columnconfigure(1, weight=1)

        ctk.CTkLabel(ctrl, text="path", font=_font(10),
                     text_color=INK_MUTE).grid(row=0, column=0, padx=(0, 8))
        self._path_entry = ctk.CTkEntry(ctrl, height=32, font=_font(11, "normal", "Cascadia Mono"),
                                         fg_color=BG_PANEL, border_color=BORDER,
                                         text_color=INK, corner_radius=R_BASE)
        self._path_entry.insert(0, str(expand(r"%USERPROFILE%")))
        self._path_entry.grid(row=0, column=1, sticky="ew", padx=4)

        ctk.CTkButton(ctrl, text="browse", width=70, height=32,
                      font=_font(10),
                      fg_color="transparent", hover_color=BG_HOVER,
                      text_color=INK_DIM, border_width=1, border_color=BORDER,
                      corner_radius=R_BASE,
                      command=self._browse).grid(row=0, column=2, padx=4)

        ctk.CTkLabel(ctrl, text="min", font=_font(10),
                     text_color=INK_MUTE).grid(row=0, column=3, padx=(12, 6))
        self._min_var = ctk.StringVar(value="100 MB")
        ctk.CTkOptionMenu(ctrl, values=["10 MB", "50 MB", "100 MB", "500 MB", "1 GB"],
                          variable=self._min_var, width=96, height=32,
                          font=_font(11),
                          fg_color=BG_PANEL, button_color=BG_RAISED,
                          button_hover_color=BG_HOVER, text_color=INK,
                          dropdown_fg_color=BG_PANEL,
                          corner_radius=R_BASE
                          ).grid(row=0, column=4, padx=4)

        tree_frame, self._tree = make_tree(self, [("Size", 110), ("Path", 540), ("Modified", 140)])
        tree_frame.grid(row=5, column=0, sticky="nsew")
        self._tree.bind("<Double-1>", self._on_double_click)

    def _browse(self):
        d = filedialog.askdirectory()
        if d:
            self._path_entry.delete(0, "end")
            self._path_entry.insert(0, d)

    def _parse_min(self) -> int:
        s = self._min_var.get()
        parts = s.split()
        val = float(parts[0])
        unit = parts[1] if len(parts) > 1 else "MB"
        mult = {"KB": 1024, "MB": 1024**2, "GB": 1024**3}.get(unit, 1024**2)
        return int(val * mult)

    def _start_scan(self):
        if self._scanning:
            return
        self._scanning = True
        self._scan_btn.configure(state="disabled", text="Scanning…")
        self._tree.delete(*self._tree.get_children())
        self._status.configure(text="Scanning…")
        path = self._path_entry.get().strip()
        min_bytes = self._parse_min()
        threading.Thread(target=self._scan_worker, args=(path, min_bytes), daemon=True).start()

    def _scan_worker(self, path_str, min_bytes):
        results = []
        try:
            for root, dirs, files in os.walk(path_str):
                for f in files:
                    try:
                        fp = Path(root) / f
                        st = fp.stat()
                        if st.st_size >= min_bytes:
                            mtime = datetime.fromtimestamp(st.st_mtime).strftime("%Y-%m-%d %H:%M")
                            results.append((st.st_size, str(fp), mtime))
                    except (PermissionError, OSError):
                        pass
        except (PermissionError, OSError):
            pass
        results.sort(reverse=True)
        self.after(0, lambda: self._on_done(results))

    def _on_done(self, results):
        self._scanning = False
        self._scan_btn.configure(state="normal", text="Scan")
        total_size = sum(r[0] for r in results)
        for size, path, mtime in results[:1000]:
            self._tree.insert("", "end", values=(fmt_size(size), path, mtime))
        cap_note = "  ·  showing first 1,000" if len(results) > 1000 else ""
        self._status.configure(
            text=f"{len(results):,} files{cap_note}", text_color=INK_DIM)
        self._tile.value_lbl.configure(text=fmt_size(total_size))
        self._tile.sub_lbl.configure(text=f"across {len(results):,} files")

    def _on_double_click(self, event):
        sel = self._tree.selection()
        if sel:
            path = self._tree.item(sel[0])["values"][1]
            try:
                subprocess.Popen(["explorer", "/select,", str(path)])
            except OSError:
                pass


# ── Tool 4: Startup Manager ─────────────────────────────────────────────────

class StartupTool(ctk.CTkFrame):

    def __init__(self, master):
        super().__init__(master, fg_color="transparent")
        self.grid_columnconfigure(0, weight=1)
        self.grid_rowconfigure(4, weight=1)
        self._loaded = False

        hdr, rule = make_page_header(
            self, title="Startup",
            subtitle="Programs that launch when Windows boots.")
        hdr.grid(row=0, column=0, sticky="ew")
        rule.grid(row=1, column=0, sticky="ew", pady=(2, 10))

        # Action row
        actions = ctk.CTkFrame(self, fg_color="transparent")
        actions.grid(row=2, column=0, sticky="ew", pady=(0, 12))
        ctk.CTkButton(actions, text="Refresh",
                      width=100, height=34, font=_font(11, "bold"),
                      fg_color=ACCENT, hover_color="#3a8aef",
                      text_color="#0a1322", corner_radius=R_BASE,
                      command=self._load).pack(side="left")
        ctk.CTkButton(actions, text="Open folder",
                      width=120, height=34, font=_font(11),
                      fg_color="transparent", hover_color=BG_HOVER,
                      text_color=INK_DIM,
                      border_width=1, border_color=BORDER,
                      corner_radius=R_BASE,
                      command=self._open_startup_folder).pack(side="left", padx=(8, 14))
        self._status = ctk.CTkLabel(actions, text="Reading registry…",
                                     font=_font(11), text_color=INK_DIM)
        self._status.pack(side="left")

        # Summary tile
        self._tile = Tile(self, "Startup entries", accent=GREEN)
        self._tile.kpi("—", "registry Run + RunOnce + Startup folder", value_size=20)
        self._tile.frame.grid(row=3, column=0, sticky="ew", pady=(0, 12))

        tree_frame, self._tree = make_tree(self, [("Name", 200), ("Command", 400), ("Location", 200)])
        tree_frame.grid(row=4, column=0, sticky="nsew")

    def _on_show(self):
        if not self._loaded:
            self._load()

    def _open_startup_folder(self):
        folder = expand(r"%APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup")
        if folder.exists():
            subprocess.Popen(["explorer", str(folder)])

    def _load(self):
        self._loaded = True
        self._tree.delete(*self._tree.get_children())
        entries = []

        # Registry keys
        reg_locations = [
            (winreg.HKEY_CURRENT_USER, r"Software\Microsoft\Windows\CurrentVersion\Run", "HKCU\\...\\Run"),
            (winreg.HKEY_LOCAL_MACHINE, r"Software\Microsoft\Windows\CurrentVersion\Run", "HKLM\\...\\Run"),
            (winreg.HKEY_CURRENT_USER, r"Software\Microsoft\Windows\CurrentVersion\RunOnce", "HKCU\\...\\RunOnce"),
            (winreg.HKEY_LOCAL_MACHINE, r"Software\Microsoft\Windows\CurrentVersion\RunOnce", "HKLM\\...\\RunOnce"),
        ]
        for hive, subkey, loc_label in reg_locations:
            try:
                key = winreg.OpenKey(hive, subkey, 0, winreg.KEY_READ)
                i = 0
                while True:
                    try:
                        name, value, _ = winreg.EnumValue(key, i)
                        entries.append((name, str(value), loc_label))
                        i += 1
                    except OSError:
                        break
                winreg.CloseKey(key)
            except (FileNotFoundError, OSError):
                pass

        # Startup folder
        startup_dir = expand(r"%APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup")
        if startup_dir.exists():
            for f in startup_dir.iterdir():
                if f.is_file():
                    entries.append((f.stem, str(f), "Startup Folder"))

        for name, cmd, loc in entries:
            self._tree.insert("", "end", values=(name, cmd, loc))

        self._status.configure(text=f"{len(entries)} entries found",
                                text_color=INK_DIM)
        self._tile.value_lbl.configure(text=str(len(entries)))
        self._tile.sub_lbl.configure(text="things that launch at boot")


# ── Tool 5: Installed Programs ───────────────────────────────────────────────

class ProgramsTool(ctk.CTkFrame):

    def __init__(self, master):
        super().__init__(master, fg_color="transparent")
        self.grid_columnconfigure(0, weight=1)
        self.grid_rowconfigure(5, weight=1)
        self._loaded = False
        self._all_items = []

        hdr, rule = make_page_header(
            self, title="Programs",
            subtitle="Everything registered as installed software.")
        hdr.grid(row=0, column=0, sticky="ew")
        rule.grid(row=1, column=0, sticky="ew", pady=(2, 10))

        # Action row
        actions = ctk.CTkFrame(self, fg_color="transparent")
        actions.grid(row=2, column=0, sticky="ew", pady=(0, 12))
        ctk.CTkButton(actions, text="Refresh",
                      width=100, height=34, font=_font(11, "bold"),
                      fg_color=ACCENT, hover_color="#3a8aef",
                      text_color="#0a1322", corner_radius=R_BASE,
                      command=self._load).pack(side="left")
        self._status = ctk.CTkLabel(actions, text="Reading registry…",
                                     font=_font(11), text_color=INK_DIM)
        self._status.pack(side="left", padx=14)

        # Summary tile
        self._tile = Tile(self, "Installed programs", accent=INFO)
        self._tile.kpi("—", "across HKLM + HKCU registry hives", value_size=20)
        self._tile.frame.grid(row=3, column=0, sticky="ew", pady=(0, 12))

        # Search box
        self._search = ctk.CTkEntry(self, placeholder_text="Search programs…",
                                     height=34, font=_font(12),
                                     fg_color=BG_PANEL, border_color=BORDER,
                                     text_color=INK, corner_radius=R_BASE)
        self._search.grid(row=4, column=0, sticky="ew", pady=(0, 10))
        self._search.bind("<KeyRelease>", lambda e: self._filter())

        tree_frame, self._tree = make_tree(self, [
            ("Name", 250), ("Publisher", 160), ("Version", 100),
            ("Size", 90), ("Installed", 100)])
        tree_frame.grid(row=5, column=0, sticky="nsew")

    def _on_show(self):
        if not self._loaded:
            self._load()

    def _load(self):
        self._loaded = True
        self._all_items.clear()
        self._tree.delete(*self._tree.get_children())

        reg_paths = [
            (winreg.HKEY_LOCAL_MACHINE, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall"),
            (winreg.HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall"),
            (winreg.HKEY_CURRENT_USER, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall"),
        ]
        seen = set()
        for hive, path in reg_paths:
            try:
                key = winreg.OpenKey(hive, path, 0, winreg.KEY_READ)
                for i in range(winreg.QueryInfoKey(key)[0]):
                    try:
                        subname = winreg.EnumKey(key, i)
                        subkey = winreg.OpenKey(key, subname, 0, winreg.KEY_READ)
                        def _val(name):
                            try:
                                return str(winreg.QueryValueEx(subkey, name)[0])
                            except (FileNotFoundError, OSError):
                                return ""
                        display = _val("DisplayName")
                        if not display or display in seen:
                            winreg.CloseKey(subkey)
                            continue
                        seen.add(display)
                        publisher = _val("Publisher")
                        version = _val("DisplayVersion")
                        est_size = _val("EstimatedSize")
                        try:
                            size_str = fmt_size(int(est_size) * 1024) if est_size else ""
                        except (ValueError, TypeError):
                            size_str = ""
                        install_date = _val("InstallDate")
                        if len(install_date) == 8:
                            install_date = f"{install_date[:4]}-{install_date[4:6]}-{install_date[6:]}"
                        self._all_items.append((display, publisher, version, size_str, install_date))
                        winreg.CloseKey(subkey)
                    except OSError:
                        pass
                winreg.CloseKey(key)
            except (FileNotFoundError, OSError):
                pass

        self._all_items.sort(key=lambda x: x[0].lower())
        self._filter()
        self._status.configure(text=f"{len(self._all_items)} programs found",
                                text_color=INK_DIM)
        self._tile.value_lbl.configure(text=f"{len(self._all_items):,}")
        self._tile.sub_lbl.configure(text="across HKLM + HKCU registry hives")

    def _filter(self):
        q = self._search.get().lower()
        self._tree.delete(*self._tree.get_children())
        for item in self._all_items:
            if q and q not in item[0].lower() and q not in item[1].lower():
                continue
            self._tree.insert("", "end", values=item)


# ── Tool 6: Network Connections ──────────────────────────────────────────────

class NetworkTool(ctk.CTkFrame):

    def __init__(self, master):
        super().__init__(master, fg_color="transparent")
        self.grid_columnconfigure(0, weight=1)
        self.grid_rowconfigure(4, weight=1)
        self._loaded = False

        hdr, rule = make_page_header(
            self, title="Network",
            subtitle="Active TCP/UDP connections and owning processes.")
        hdr.grid(row=0, column=0, sticky="ew")
        rule.grid(row=1, column=0, sticky="ew", pady=(2, 10))

        # Action row
        actions = ctk.CTkFrame(self, fg_color="transparent")
        actions.grid(row=2, column=0, sticky="ew", pady=(0, 12))
        ctk.CTkButton(actions, text="Refresh",
                      width=100, height=34, font=_font(11, "bold"),
                      fg_color=ACCENT, hover_color="#3a8aef",
                      text_color="#0a1322", corner_radius=R_BASE,
                      command=self._load).pack(side="left")
        self._status = ctk.CTkLabel(actions, text="Click Refresh to load connections.",
                                     font=_font(11), text_color=INK_DIM)
        self._status.pack(side="left", padx=14)

        # Summary tiles
        tile_row = ctk.CTkFrame(self, fg_color="transparent")
        tile_row.grid(row=3, column=0, sticky="ew", pady=(0, 12))

        self._tile_total = Tile(tile_row, "Total connections", accent=INFO)
        self._tile_total.kpi("—", "click Refresh", value_size=20)
        self._tile_total.frame.pack(fill="x", pady=(0, 6))

        self._tile_est = Tile(tile_row, "Established", accent=GREEN)
        self._tile_est.kpi("—", "TCP traffic flowing", value_size=20)
        self._tile_est.frame.pack(fill="x", pady=(0, 6))

        self._tile_listen = Tile(tile_row, "Listening", accent=ACCENT)
        self._tile_listen.kpi("—", "open ports awaiting clients", value_size=20)
        self._tile_listen.frame.pack(fill="x", pady=(0, 6))

        # Table
        tree_frame, self._tree = make_tree(self, [
            ("Proto", 60), ("Local Address", 180), ("Remote Address", 180),
            ("State", 110), ("PID", 60), ("Process", 150)])
        tree_frame.grid(row=4, column=0, sticky="nsew")

        self._tree.tag_configure("established", foreground=GREEN)
        self._tree.tag_configure("listening", foreground=ACCENT)
        self._tree.tag_configure("time_wait", foreground=INK_MUTE)

    def _on_show(self):
        if not self._loaded:
            self._load()

    def _load(self):
        self._loaded = True
        self._tree.delete(*self._tree.get_children())
        threading.Thread(target=self._load_worker, daemon=True).start()

    def _load_worker(self):
        # Get PID -> process name map
        pid_map = {}
        try:
            r = subprocess.run(["tasklist", "/fo", "CSV", "/nh"],
                              capture_output=True, text=True, timeout=10,
                              creationflags=0x08000000)
            for row in csv.reader(io.StringIO(r.stdout)):
                if len(row) >= 2:
                    try:
                        pid_map[row[1].strip()] = row[0].strip().strip('"')
                    except (ValueError, IndexError):
                        pass
        except Exception:
            pass

        # Parse netstat
        connections = []
        try:
            r = subprocess.run(["netstat", "-ano"],
                              capture_output=True, text=True, timeout=10,
                              creationflags=0x08000000)
            for line in r.stdout.splitlines():
                parts = line.split()
                if len(parts) >= 4 and parts[0] in ("TCP", "UDP"):
                    proto = parts[0]
                    local = parts[1]
                    if proto == "TCP":
                        remote = parts[2]
                        state = parts[3] if len(parts) > 3 else ""
                        pid = parts[4] if len(parts) > 4 else ""
                    else:
                        remote = parts[2] if len(parts) > 2 and ":" in parts[2] else "*:*"
                        state = ""
                        pid = parts[-1]
                    proc = pid_map.get(pid, "")
                    connections.append((proto, local, remote, state, pid, proc))
        except Exception:
            pass

        self.after(0, lambda: self._on_done(connections))

    def _on_done(self, connections):
        est_count = 0
        listen_count = 0
        for proto, local, remote, state, pid, proc in connections:
            tag = ""
            if "ESTABLISHED" in state:
                tag = "established"
                est_count += 1
            elif "LISTENING" in state or "LISTEN" in state:
                tag = "listening"
                listen_count += 1
            elif "TIME_WAIT" in state or "CLOSE_WAIT" in state:
                tag = "time_wait"
            self._tree.insert("", "end", values=(proto, local, remote, state, pid, proc),
                             tags=(tag,) if tag else ())
        self._status.configure(text=f"{len(connections)} active connections",
                                text_color=INK_DIM)
        # Update summary tiles
        self._tile_total.value_lbl.configure(text=str(len(connections)))
        self._tile_total.sub_lbl.configure(text="TCP + UDP")
        self._tile_est.value_lbl.configure(text=str(est_count))
        self._tile_est.sub_lbl.configure(text="TCP traffic flowing")
        self._tile_listen.value_lbl.configure(text=str(listen_count))
        self._tile_listen.sub_lbl.configure(text="open ports")


# ── Tool 7: Duplicate Finder ────────────────────────────────────────────────

class DuplicateTool(ctk.CTkFrame):

    def __init__(self, master):
        super().__init__(master, fg_color="transparent")
        self.grid_columnconfigure(0, weight=1)
        self.grid_rowconfigure(5, weight=1)
        self._scanning = False
        self._cancel = threading.Event()

        hdr, rule = make_page_header(
            self, title="Duplicates",
            subtitle="Group identical files by content hash.")
        hdr.grid(row=0, column=0, sticky="ew")
        rule.grid(row=1, column=0, sticky="ew", pady=(2, 10))

        # Action row
        actions = ctk.CTkFrame(self, fg_color="transparent")
        actions.grid(row=2, column=0, sticky="ew", pady=(0, 12))
        self._scan_btn = ctk.CTkButton(actions, text="Find duplicates",
                      width=140, height=34, font=_font(11, "bold"),
                      fg_color=ACCENT, hover_color="#3a8aef",
                      text_color="#0a1322", corner_radius=R_BASE,
                      command=self._start_scan)
        self._scan_btn.pack(side="left")
        self._status = ctk.CTkLabel(actions,
                                     text="Pick a folder + minimum size, then run.",
                                     font=_font(11), text_color=INK_DIM)
        self._status.pack(side="left", padx=14)

        # Summary tile
        self._tile = Tile(self, "Wasted space", accent=PURPLE)
        self._tile.kpi("—", "no scan yet", value_size=20)
        self._tile.frame.grid(row=3, column=0, sticky="ew", pady=(0, 12))

        # Compact controls under the tile
        ctrl = ctk.CTkFrame(self, fg_color="transparent")
        ctrl.grid(row=4, column=0, sticky="ew", pady=(0, 10))
        ctrl.grid_columnconfigure(1, weight=1)

        ctk.CTkLabel(ctrl, text="folder", font=_font(10),
                     text_color=INK_MUTE).grid(row=0, column=0, padx=(0, 8))
        self._path_entry = ctk.CTkEntry(ctrl, height=32, font=_font(11, "normal", "Cascadia Mono"),
                                         fg_color=BG_PANEL, border_color=BORDER,
                                         text_color=INK, corner_radius=R_BASE)
        self._path_entry.insert(0, str(expand(r"%USERPROFILE%")))
        self._path_entry.grid(row=0, column=1, sticky="ew", padx=4)
        ctk.CTkButton(ctrl, text="browse", width=70, height=32,
                      fg_color="transparent", hover_color=BG_HOVER,
                      text_color=INK_DIM, border_width=1, border_color=BORDER,
                      font=_font(10), corner_radius=R_BASE,
                      command=self._browse).grid(row=0, column=2, padx=4)
        ctk.CTkLabel(ctrl, text="min", font=_font(10),
                     text_color=INK_MUTE).grid(row=0, column=3, padx=(12, 6))
        self._min_var = ctk.StringVar(value="1 MB")
        ctk.CTkOptionMenu(ctrl, values=["1 KB", "100 KB", "1 MB", "10 MB", "100 MB"],
                          variable=self._min_var, width=96, height=32,
                          font=_font(11),
                          fg_color=BG_PANEL, button_color=BG_RAISED,
                          button_hover_color=BG_HOVER, text_color=INK,
                          dropdown_fg_color=BG_PANEL,
                          corner_radius=R_BASE
                          ).grid(row=0, column=4, padx=4)

        tree_frame, self._tree = make_tree(self, [("Group", 60), ("Size", 100), ("Path", 550)])
        tree_frame.grid(row=5, column=0, sticky="nsew")
        self._tree.bind("<Double-1>", self._on_double_click)

    def _browse(self):
        d = filedialog.askdirectory()
        if d:
            self._path_entry.delete(0, "end")
            self._path_entry.insert(0, d)

    def _parse_min(self) -> int:
        s = self._min_var.get().split()
        val = float(s[0])
        mult = {"KB": 1024, "MB": 1024**2, "GB": 1024**3}.get(s[1] if len(s) > 1 else "MB", 1024**2)
        return int(val * mult)

    def _start_scan(self):
        if self._scanning:
            return
        self._scanning = True
        self._cancel.clear()
        self._scan_btn.configure(state="disabled", text="Scanning…")
        self._tree.delete(*self._tree.get_children())
        self._status.configure(text="Phase 1: Grouping by file size…")
        path = self._path_entry.get().strip()
        min_bytes = self._parse_min()
        threading.Thread(target=self._scan_worker, args=(path, min_bytes), daemon=True).start()

    def _scan_worker(self, path_str, min_bytes):
        # Phase 1: Group by size
        size_map = {}
        try:
            for root, dirs, files in os.walk(path_str):
                if self._cancel.is_set():
                    return
                for f in files:
                    try:
                        fp = os.path.join(root, f)
                        size = os.path.getsize(fp)
                        if size >= min_bytes:
                            size_map.setdefault(size, []).append(fp)
                    except (PermissionError, OSError):
                        pass
        except (PermissionError, OSError):
            pass

        # Only keep sizes with 2+ files
        candidates = {sz: paths for sz, paths in size_map.items() if len(paths) > 1}
        self.after(0, lambda: self._status.configure(
            text=f"Phase 2: Hashing {sum(len(v) for v in candidates.values()):,} files…"))

        # Phase 2: Hash to confirm duplicates
        groups = []
        for sz, paths in candidates.items():
            if self._cancel.is_set():
                return
            hash_map = {}
            for fp in paths:
                try:
                    h = hashlib.md5()
                    with open(fp, "rb") as fobj:
                        chunk = fobj.read(8192)
                        while chunk:
                            h.update(chunk)
                            chunk = fobj.read(8192)
                    hash_map.setdefault(h.hexdigest(), []).append(fp)
                except (PermissionError, OSError):
                    pass
            for digest, dups in hash_map.items():
                if len(dups) > 1:
                    groups.append((sz, dups))

        groups.sort(key=lambda x: x[0] * len(x[1]), reverse=True)
        self.after(0, lambda: self._on_done(groups))

    def _on_done(self, groups):
        self._scanning = False
        self._scan_btn.configure(state="normal", text="Find duplicates")
        wasted = 0
        for g, (sz, paths) in enumerate(groups, 1):
            wasted += sz * (len(paths) - 1)
            for fp in paths:
                self._tree.insert("", "end", values=(g, fmt_size(sz), fp))

        self._status.configure(text=f"{len(groups)} duplicate groups",
                                text_color=INK_DIM)
        self._tile.value_lbl.configure(text=fmt_size(wasted))
        self._tile.sub_lbl.configure(text=f"across {len(groups)} duplicate groups")

    def _on_double_click(self, event):
        sel = self._tree.selection()
        if sel:
            path = self._tree.item(sel[0])["values"][2]
            try:
                subprocess.Popen(["explorer", "/select,", str(path)])
            except OSError:
                pass


# ── Tool 8: Privacy Sweep ────────────────────────────────────────────────────

class PrivacyTool(ctk.CTkFrame):

    CATEGORIES = [
        ("Recent Documents", [expand(r"%APPDATA%\Microsoft\Windows\Recent")],
         "Shortcut links to recently opened files"),
        ("Jump Lists", [expand(r"%APPDATA%\Microsoft\Windows\Recent\AutomaticDestinations"),
                        expand(r"%APPDATA%\Microsoft\Windows\Recent\CustomDestinations")],
         "Per-app recent file lists (taskbar jump lists)"),
        ("Explorer Address Bar History", [],
         "Typed paths in File Explorer address bar (registry)"),
        ("Thumbnail Cache", [expand(r"%LOCALAPPDATA%\Microsoft\Windows\Explorer")],
         "Cached image previews — regenerated as needed"),
        ("Temp Files", [expand("%TEMP%")],
         "Temporary files created by apps"),
    ]

    def __init__(self, master):
        super().__init__(master, fg_color="transparent")
        self.grid_columnconfigure(0, weight=1)
        self.grid_rowconfigure(4, weight=1)
        self._loaded = False
        self._vars: list[ctk.BooleanVar] = []

        hdr, rule = make_page_header(
            self, title="Privacy",
            subtitle="Sweep recent files, jump lists, address-bar history.")
        hdr.grid(row=0, column=0, sticky="ew")
        rule.grid(row=1, column=0, sticky="ew", pady=(2, 10))

        # Action row
        actions = ctk.CTkFrame(self, fg_color="transparent")
        actions.grid(row=2, column=0, sticky="ew", pady=(0, 12))
        ctk.CTkButton(actions, text="Scan sizes",
                      width=120, height=34, font=_font(11, "bold"),
                      fg_color=ACCENT, hover_color="#3a8aef",
                      text_color="#0a1322", corner_radius=R_BASE,
                      command=self._scan).pack(side="left")
        self._status = ctk.CTkLabel(actions,
                                     text="Click Scan sizes to enumerate privacy data.",
                                     font=_font(11), text_color=INK_DIM)
        self._status.pack(side="left", padx=14)

        # Summary tile
        self._tile = Tile(self, "Privacy traces", accent=ORANGE)
        self._tile.kpi("—", "across recent files + caches + history", value_size=20)
        self._tile.frame.grid(row=3, column=0, sticky="ew", pady=(0, 12))

        # Categories scrollable list
        self._body = ctk.CTkScrollableFrame(self, fg_color="transparent",
                                             scrollbar_button_color=BG_RAISED,
                                             scrollbar_button_hover_color=BG_HOVER)
        self._body.grid(row=4, column=0, sticky="nsew")
        self._body.grid_columnconfigure(0, weight=1)

        # Action bar at bottom
        act = ctk.CTkFrame(self, fg_color="transparent")
        act.grid(row=5, column=0, sticky="ew", pady=(12, 0))
        self._sweep_btn = ctk.CTkButton(act, text="Sweep selected",
                                         width=140, height=34,
                                         fg_color=RED_BTN, hover_color=RED_BTN_H,
                                         text_color="#ffffff", corner_radius=R_BASE,
                                         font=_font(11, "bold"),
                                         command=self._sweep, state="disabled")
        self._sweep_btn.pack(side="right")

        self._size_labels: list[ctk.CTkLabel] = []
        self._build_list()

    def _build_list(self):
        self._vars.clear()
        self._size_labels.clear()
        for i, (name, paths, desc) in enumerate(self.CATEGORIES):
            var = ctk.BooleanVar(value=True)
            self._vars.append(var)
            row = ctk.CTkFrame(self._body, fg_color=BG_RAISED,
                                corner_radius=R_BASE,
                                border_width=1, border_color=BORDER)
            row.grid(row=i, column=0, sticky="ew", pady=2, padx=0)
            row.grid_columnconfigure(1, weight=1)

            ctk.CTkCheckBox(row, text="", variable=var, width=22,
                           checkbox_width=16, checkbox_height=16,
                           border_width=1, border_color=BORDER,
                           fg_color=ACCENT, hover_color=ACCENT_DIM,
                           corner_radius=R_SHARP
                           ).grid(row=0, column=0, rowspan=2, padx=(12, 8), pady=10)
            ctk.CTkLabel(row, text=name, font=F_BODY_B(),
                        text_color=INK, anchor="w"
                        ).grid(row=0, column=1, sticky="w", padx=0, pady=(8, 0))
            ctk.CTkLabel(row, text=desc, font=F_SMALL(),
                        text_color=INK_DIM, anchor="w"
                        ).grid(row=1, column=1, sticky="w", padx=0, pady=(0, 8))
            size_lbl = ctk.CTkLabel(row, text="—", font=F_MONO_B(11),
                                    text_color=INK_MUTE)
            size_lbl.grid(row=0, column=2, rowspan=2, padx=(8, 14), pady=10)
            self._size_labels.append(size_lbl)

    def _on_show(self):
        pass

    def _scan(self):
        self._sweep_btn.configure(state="normal")
        threading.Thread(target=self._scan_worker, daemon=True).start()

    def _scan_worker(self):
        grand_total = 0
        for i, (name, paths, _) in enumerate(self.CATEGORIES):
            total_size = 0
            if name == "Explorer Address Bar History":
                total_size = 1024  # symbolic — registry data
            else:
                for p in paths:
                    if p.exists():
                        sz, _ = dir_size(p)
                        total_size += sz
            grand_total += total_size
            self.after(0, lambda idx=i, sz=total_size: self._update_size(idx, sz))
        self.after(0, lambda gt=grand_total: self._on_scan_done(gt))

    def _on_scan_done(self, grand_total):
        self._status.configure(text="Scan complete — select categories to sweep.",
                                text_color=INK_DIM)
        self._tile.value_lbl.configure(text=fmt_size(grand_total),
                                        text_color=ORANGE if grand_total > 0 else INK)
        self._tile.sub_lbl.configure(text=f"across {len(self.CATEGORIES)} categories")

    def _update_size(self, idx, size):
        self._size_labels[idx].configure(text=fmt_size(size), text_color=size_color(size))

    def _sweep(self):
        selected = []
        for i, (name, paths, _) in enumerate(self.CATEGORIES):
            if self._vars[i].get():
                selected.append((name, paths))
        if not selected:
            return
        names = "\n".join(f"  - {n}" for n, _ in selected)
        if not messagebox.askyesno("Privacy Sweep",
            f"Clear {len(selected)} categories?\n\n{names}\n\n"
            "Close all browsers first for best results.\nThis cannot be undone.", icon="warning"):
            return
        threading.Thread(target=self._sweep_worker, args=(selected,), daemon=True).start()

    def _sweep_worker(self, selected):
        for name, paths in selected:
            if name == "Explorer Address Bar History":
                try:
                    winreg.DeleteKey(winreg.HKEY_CURRENT_USER,
                                    r"Software\Microsoft\Windows\CurrentVersion\Explorer\TypedPaths")
                except OSError:
                    pass
            else:
                for p in paths:
                    if p.exists():
                        try:
                            for child in p.iterdir():
                                try:
                                    if child.is_dir():
                                        shutil.rmtree(child, ignore_errors=True)
                                    else:
                                        child.unlink(missing_ok=True)
                                except (PermissionError, OSError):
                                    pass
                        except (PermissionError, OSError):
                            pass
        self.after(0, lambda: self._status.configure(text="Sweep complete. Click Scan Sizes to verify."))
        self.after(0, lambda: messagebox.showinfo("Privacy Sweep", "Sweep complete!"))


# ── Tool 9: Scheduled Tasks ─────────────────────────────────────────────────

class TasksTool(ctk.CTkFrame):

    def __init__(self, master):
        super().__init__(master, fg_color="transparent")
        self.grid_columnconfigure(0, weight=1)
        self.grid_rowconfigure(4, weight=1)
        self._loaded = False

        hdr, rule = make_page_header(
            self, title="Scheduled Tasks",
            subtitle="Third-party scheduled tasks (built-in Windows hidden).")
        hdr.grid(row=0, column=0, sticky="ew")
        rule.grid(row=1, column=0, sticky="ew", pady=(2, 10))

        # Action row
        actions = ctk.CTkFrame(self, fg_color="transparent")
        actions.grid(row=2, column=0, sticky="ew", pady=(0, 12))
        ctk.CTkButton(actions, text="Refresh",
                      width=100, height=34, font=_font(11, "bold"),
                      fg_color=ACCENT, hover_color="#3a8aef",
                      text_color="#0a1322", corner_radius=R_BASE,
                      command=self._load).pack(side="left")
        ctk.CTkButton(actions, text="Open scheduler",
                      width=140, height=34, font=_font(11),
                      fg_color="transparent", hover_color=BG_HOVER,
                      text_color=INK_DIM,
                      border_width=1, border_color=BORDER,
                      corner_radius=R_BASE,
                      command=lambda: subprocess.Popen(["taskschd.msc"], shell=True)
                      ).pack(side="left", padx=(8, 14))
        self._status = ctk.CTkLabel(actions, text="Loading scheduled tasks…",
                                     font=_font(11), text_color=INK_DIM)
        self._status.pack(side="left")

        # Summary tile
        self._tile = Tile(self, "Scheduled tasks", accent=PURPLE)
        self._tile.kpi("—", "third-party tasks · built-in Windows hidden",
                        value_size=20)
        self._tile.frame.grid(row=3, column=0, sticky="ew", pady=(0, 12))

        tree_frame, self._tree = make_tree(self, [
            ("Task Name", 250), ("Status", 80), ("Next Run", 150),
            ("Last Result", 90), ("Author", 180)])
        tree_frame.grid(row=4, column=0, sticky="nsew")

    def _on_show(self):
        if not self._loaded:
            self._load()

    def _load(self):
        self._loaded = True
        self._tree.delete(*self._tree.get_children())
        self._status.configure(text="Loading scheduled tasks…")
        threading.Thread(target=self._load_worker, daemon=True).start()

    def _load_worker(self):
        tasks = []
        try:
            r = subprocess.run(
                ["schtasks", "/query", "/fo", "CSV", "/v"],
                capture_output=True, text=True, timeout=30,
                creationflags=0x08000000)
            reader = csv.reader(io.StringIO(r.stdout))
            headers = next(reader, [])
            # Find column indices
            def col_idx(name):
                for i, h in enumerate(headers):
                    if name.lower() in h.lower():
                        return i
                return -1
            i_name = col_idx("TaskName")
            i_status = col_idx("Status")
            i_next = col_idx("Next Run")
            i_result = col_idx("Last Result")
            i_author = col_idx("Author")

            for row in reader:
                if len(row) <= max(i_name, i_status, i_next, i_result, i_author):
                    continue
                name = row[i_name] if i_name >= 0 else ""
                status = row[i_status] if i_status >= 0 else ""
                next_run = row[i_next] if i_next >= 0 else ""
                result = row[i_result] if i_result >= 0 else ""
                author = row[i_author] if i_author >= 0 else ""
                if name and not name.startswith("\\Microsoft\\Windows\\"):
                    tasks.append((name, status, next_run, result, author))
        except Exception:
            pass

        self.after(0, lambda: self._on_done(tasks))

    def _on_done(self, tasks):
        for name, status, next_run, result, author in tasks:
            self._tree.insert("", "end", values=(name, status, next_run, result, author))
        self._status.configure(text=f"{len(tasks)} loaded", text_color=INK_DIM)
        self._tile.value_lbl.configure(text=str(len(tasks)))
        self._tile.sub_lbl.configure(text="user + third-party scheduled tasks")


# ── Tool 10: Disk Space Analyzer ────────────────────────────────────────────

class DiskTool(ctk.CTkFrame):

    def __init__(self, master):
        super().__init__(master, fg_color="transparent")
        self.grid_columnconfigure(0, weight=1)
        self.grid_rowconfigure(3, weight=1)
        self._scanning = False
        self._path_stack = []

        hdr, rule = make_page_header(
            self, title="Disk Map",
            subtitle="Drill into folder sizes drive by drive.")
        hdr.grid(row=0, column=0, sticky="ew")
        rule.grid(row=1, column=0, sticky="ew", pady=(2, 10))

        # Path bar / drives row
        ctrl = ctk.CTkFrame(self, fg_color=BG_PANEL, corner_radius=R_BASE,
                             border_width=1, border_color=BORDER, height=44)
        ctrl.grid(row=2, column=0, sticky="ew", pady=(0, 8))
        ctrl.grid_propagate(False)
        ctrl.grid_columnconfigure(1, weight=1)

        self._back_btn = ctk.CTkButton(ctrl, text="‹  back", width=60, height=28,
                                        fg_color="transparent", hover_color=BG_HOVER,
                                        text_color=INK_DIM,
                                        border_width=1, border_color=BORDER,
                                        font=F_MICRO(), corner_radius=R_SHARP,
                                        state="disabled", command=self._go_back)
        self._back_btn.grid(row=0, column=0, padx=(10, 12), pady=8)

        self._path_lbl = ctk.CTkLabel(ctrl, text="Select a drive →",
                                       font=F_MONO(11), text_color=INK_DIM,
                                       anchor="w")
        self._path_lbl.grid(row=0, column=1, sticky="w")

        # Drive buttons
        drive_frame = ctk.CTkFrame(ctrl, fg_color="transparent")
        drive_frame.grid(row=0, column=2, padx=(0, 10))
        for letter in "CDEFGH":
            drive = f"{letter}:\\"
            if os.path.exists(drive):
                ctk.CTkButton(drive_frame, text=letter, width=32, height=28,
                              fg_color="transparent", hover_color=ACCENT_SOFT,
                              text_color=ACCENT,
                              border_width=1, border_color=ACCENT_DIM,
                              font=F_MONO_B(11), corner_radius=R_SHARP,
                              command=lambda d=drive: self._analyze(d)
                              ).pack(side="left", padx=2)

        self._body = ctk.CTkScrollableFrame(self, fg_color="transparent",
                                             scrollbar_button_color=BG_RAISED,
                                             scrollbar_button_hover_color=BG_HOVER)
        self._body.grid(row=3, column=0, sticky="nsew")
        self._body.grid_columnconfigure(0, weight=1)

        self._status = ctk.CTkLabel(self, text="", font=F_SMALL(),
                                     text_color=INK_MUTE, anchor="w")
        self._status.grid(row=4, column=0, sticky="w", pady=(4, 0))

    def _analyze(self, path_str):
        if self._scanning:
            return
        self._scanning = True
        self._path_stack.append(path_str)
        self._path_lbl.configure(text=path_str, text_color=INK)
        self._back_btn.configure(state="normal" if len(self._path_stack) > 1 else "disabled")
        self._status.configure(text="Scanning folder sizes…")
        for w in self._body.winfo_children():
            w.destroy()
        threading.Thread(target=self._scan_worker, args=(path_str,), daemon=True).start()

    def _go_back(self):
        if len(self._path_stack) > 1:
            self._path_stack.pop()
            self._analyze_from_stack()

    def _analyze_from_stack(self):
        path = self._path_stack.pop()
        self._analyze(path)

    def _scan_worker(self, path_str):
        folders = []
        other_size = 0
        try:
            base = Path(path_str)
            for entry in base.iterdir():
                try:
                    if entry.is_dir(follow_symlinks=False):
                        sz, _ = dir_size(entry)
                        if sz > 0:
                            folders.append((entry.name, sz, str(entry)))
                    elif entry.is_file(follow_symlinks=False):
                        other_size += entry.stat().st_size
                except (PermissionError, OSError):
                    pass
        except (PermissionError, OSError):
            pass
        folders.sort(key=lambda x: x[1], reverse=True)
        if other_size > 0:
            folders.append(("[files in this folder]", other_size, ""))
        self.after(0, lambda: self._on_done(folders))

    def _on_done(self, folders):
        self._scanning = False
        for w in self._body.winfo_children():
            w.destroy()

        if not folders:
            ctk.CTkLabel(self._body, text="Empty or inaccessible.",
                         font=F_BODY(), text_color=INK_DIM
                         ).grid(row=0, column=0, pady=40)
            self._status.configure(text="")
            return

        max_size = folders[0][1] if folders else 1
        total = sum(sz for _, sz, _ in folders)

        for i, (name, sz, full_path) in enumerate(folders[:50]):
            ratio = sz / max_size if max_size else 0
            pct_of_total = sz / total * 100 if total else 0
            color = GREEN if pct_of_total < 15 else ORANGE if pct_of_total < 40 else RED_SOFT

            row = ctk.CTkFrame(self._body, fg_color=BG_RAISED,
                               corner_radius=R_BASE, height=42,
                               border_width=1, border_color=BORDER)
            row.grid(row=i, column=0, sticky="ew", pady=2, padx=0)
            row.grid_propagate(False)
            row.grid_columnconfigure(1, weight=1)

            # Folder name
            name_lbl = ctk.CTkLabel(row, text=name, font=F_BODY_B(),
                                    text_color=INK, anchor="w", width=220)
            name_lbl.grid(row=0, column=0, padx=(14, 8), pady=8, sticky="w")

            # Bar (thinner, more refined)
            bar_frame = ctk.CTkFrame(row, fg_color=BG_BASE,
                                     corner_radius=2, height=8)
            bar_frame.grid(row=0, column=1, sticky="ew", padx=4, pady=16)
            bar_frame.grid_propagate(False)

            fill_width = max(4, int(ratio * 380))
            fill = ctk.CTkFrame(bar_frame, fg_color=color, corner_radius=2,
                               width=fill_width, height=8)
            fill.place(x=0, y=0)

            # Size + percentage (monospace, tabular)
            ctk.CTkLabel(row,
                         text=f"{fmt_size(sz):>10}   {pct_of_total:>5.1f}%",
                         font=F_MONO_B(11),
                         text_color=size_color(sz), width=160, anchor="e"
                         ).grid(row=0, column=2, padx=(4, 14), pady=8, sticky="e")

            # Click to drill in (only for real folders)
            if full_path:
                row.configure(cursor="hand2")
                for widget in (row, name_lbl):
                    widget.bind("<Button-1>", lambda e, p=full_path: self._analyze(p))

        self._status.configure(
            text=f"{len(folders)} items — {fmt_size(total)} total"
            + (f"  (showing top 50)" if len(folders) > 50 else ""))


# ── Home dashboard ────────────────────────────────────────────────────────────

class HomeTool(ctk.CTkFrame):
    """Fortect-style landing page: a 2-column grid of big action cards that
    launch the most common tasks. The app's front door."""

    def __init__(self, master, navigate):
        super().__init__(master, fg_color="transparent")
        self._navigate = navigate   # callback: switch to a named tool
        self.grid_columnconfigure(0, weight=1)
        self.grid_rowconfigure(2, weight=1)

        hdr, rule = make_page_header(
            self, title="WinScan",
            subtitle="Keep your PC fast, clean, and secure — pick a task to begin.")
        hdr.grid(row=0, column=0, sticky="ew")
        rule.grid(row=1, column=0, sticky="ew", pady=(2, 14))

        # Scrollable 2-column grid of action cards (Fortect-style). Now that
        # the DPI clipping is fixed (winfo == physical), grid uniform splits
        # the columns correctly and nothing spills off the right.
        body = ctk.CTkScrollableFrame(self, fg_color="transparent",
                                       scrollbar_button_color=BG_RAISED,
                                       scrollbar_button_hover_color=BG_HOVER)
        body.grid(row=2, column=0, sticky="nsew")
        body.grid_columnconfigure(0, weight=1, uniform="cards")
        body.grid_columnconfigure(1, weight=1, uniform="cards")

        # Definitions: (icon, colour, title, description, button, target tool)
        cards = [
            (ICON_CLEANUP, GREEN, "Clean up disk",
             "Remove temp files, caches, and old downloads safely.",
             "Open Cleanup", "Cleanup"),
            (ICON_DISK, ACCENT, "Analyse disk space",
             "See what's using your storage, folder by folder.",
             "Open Disk Map", "Disk Map"),
            (ICON_DUPLICATES, PURPLE, "Find duplicates",
             "Reclaim space from identical files across folders.",
             "Find Duplicates", "Duplicates"),
            (ICON_OVERVIEW, INFO, "System health",
             "Check CPU, memory, graphics, and drives at a glance.",
             "View Overview", "Overview"),
            (ICON_PRIVACY, ORANGE, "Privacy sweep",
             "Clear recent files, jump lists, and history traces.",
             "Open Privacy", "Privacy"),
            (ICON_NETWORK, INFO, "Network inspector",
             "See active connections and their owning processes.",
             "Open Network", "Network"),
        ]

        for i, (icon, colour, title, desc, btn, target) in enumerate(cards):
            card = ActionCard(body, icon=icon, icon_colour=colour,
                              title=title, description=desc,
                              button_text=btn,
                              command=lambda t=target: self._navigate(t),
                              button_colour=colour)
            row, col = divmod(i, 2)
            pad = (0, 6) if col == 0 else (6, 0)
            card.frame.grid(row=row, column=col, sticky="nsew",
                            padx=pad, pady=(0, 12))

    def _on_show(self):
        pass


# ══════════════════════════════════════════════════════════════════════════════
# Main Application
# ══════════════════════════════════════════════════════════════════════════════

# Tool registry — grouped into semantic sections for the sidebar.
# Order within each group is the order they appear.
# HomeTool is special-cased (needs a navigate callback) and added separately.
TOOL_SECTIONS = [
    ("SYSTEM", [
        ("Home",         HomeTool),
        ("Overview",     SystemInfoTool),
        ("Disk Map",     DiskTool),
    ]),
    ("CLEANUP", [
        ("Cleanup",      CleanupTool),
        ("Duplicates",   DuplicateTool),
        ("Large Files",  LargeFileTool),
        ("Privacy",      PrivacyTool),
    ]),
    ("INSPECT", [
        ("Programs",     ProgramsTool),
        ("Startup",      StartupTool),
        ("Tasks",        TasksTool),
        ("Network",      NetworkTool),
    ]),
]

# Flat list for compatibility / iteration
TOOLS = [(name, cls) for _, items in TOOL_SECTIONS for name, cls in items]


APP_VERSION = "2.1"


class WinScanApp(ctk.CTk):

    def __init__(self):
        super().__init__()
        # Pin Tk's own point-to-pixel factor so point-sized fonts don't
        # independently inflate layout width once the process is DPI-aware.
        try:
            self.tk.call("tk", "scaling", 1.0)
        except tk.TclError:
            pass
        self.title("WinScan")
        # Geometry in design pixels — window_scaling (set above) cancels CTk's
        # DPI over-multiply so this maps to the intended physical size.
        self.geometry("1180x760")
        self.minsize(980, 660)
        self.configure(fg_color=BG_BASE)
        # Explicitly clear topmost — yesterday's screenshot scripts may have set
        # it via SetWindowPos(HWND_TOPMOST). Normal Windows z-order: clicking
        # another app should put WinScan behind it.
        self.attributes("-topmost", False)

        # App icon
        if getattr(sys, "frozen", False):
            icon_path = Path(sys._MEIPASS) / "winscan.ico"
            if not icon_path.exists():
                icon_path = Path(sys.executable).parent / "winscan.ico"
        else:
            icon_path = Path(__file__).parent / "winscan.ico"
        if icon_path.exists():
            self.iconbitmap(str(icon_path))

        _setup_treeview_style()
        self._tool_frames: dict[str, ctk.CTkFrame] = {}
        self._sidebar_btns: dict[str, tuple] = {}
        self._active_tool: Optional[str] = None

        self._build_ui()
        # Default to Home (the action-card landing page). Override via env var.
        startup_tool = os.environ.get("WINSCAN_STARTUP_TOOL", "Home")
        if startup_tool not in self._tool_frames:
            startup_tool = "Home"
        self._switch_tool(startup_tool)
        self._tick_status()

    def _build_ui(self):
        self.grid_columnconfigure(1, weight=1)
        self.grid_rowconfigure(0, weight=1)

        SIDEBAR_W = _px(220)   # roomier — accommodates icons + names
        ITEM_W    = _px(204)   # explicit so items don't overflow

        # ── Sidebar (spans full height including status-bar row) ─────────────
        sidebar = ctk.CTkFrame(self, fg_color=SIDEBAR_BG, width=SIDEBAR_W,
                                corner_radius=0)
        sidebar.grid(row=0, column=0, rowspan=3, sticky="nsew")
        sidebar.grid_propagate(False)
        sidebar.grid_columnconfigure(0, weight=1)
        sidebar.grid_rowconfigure(2, weight=1)

        # ── Brand block ──────────────────────────────────────────────────────
        # Use pack inside — grid_propagate(False) was eating the label rendering
        brand = ctk.CTkFrame(sidebar, fg_color="transparent")
        brand.grid(row=0, column=0, sticky="ew", pady=(16, 14), padx=(14, 14))

        stripe = ctk.CTkFrame(brand, fg_color=ACCENT, width=3,
                               height=38, corner_radius=2)
        stripe.pack(side="left", padx=(0, 12))
        stripe.pack_propagate(False)

        text_col = ctk.CTkFrame(brand, fg_color="transparent")
        text_col.pack(side="left", fill="x", expand=True)
        ctk.CTkLabel(text_col, text="WinScan",
                     font=_font(18, "bold", "Segoe UI"),
                     text_color=INK, anchor="w"
                     ).pack(anchor="w")
        ctk.CTkLabel(text_col, text="SYSTEM TOOLKIT",
                     font=_font(8, "bold", "Cascadia Mono"),
                     text_color=ACCENT, anchor="w"
                     ).pack(anchor="w", pady=(2, 0))

        # Hairline divider
        ctk.CTkFrame(sidebar, fg_color=BORDER, height=1
                     ).grid(row=1, column=0, sticky="ew", padx=14)

        # ── Sectioned tool list (NOT scrollable — fits everything) ──────────
        nav = ctk.CTkFrame(sidebar, fg_color="transparent")
        nav.grid(row=2, column=0, sticky="nsew", pady=(8, 4))
        nav.grid_columnconfigure(0, weight=1)

        row_i = 0
        for section_name, items in TOOL_SECTIONS:
            sec_row = ctk.CTkFrame(nav, fg_color="transparent")
            sec_row.grid(row=row_i, column=0, sticky="ew",
                          padx=(18, 14), pady=(12, 6))
            sec_row.grid_columnconfigure(1, weight=1)
            ctk.CTkLabel(sec_row, text=section_name,
                         font=_font(8, "bold", "Cascadia Mono"),
                         text_color=INK_MUTE, anchor="w"
                         ).grid(row=0, column=0, sticky="w")
            # subtle right-hairline
            ctk.CTkFrame(sec_row, fg_color=BORDER, height=1
                          ).grid(row=0, column=1, sticky="ew", padx=(10, 0), pady=(6, 0))
            row_i += 1
            for name, _ in items:
                btn = self._make_nav_item(nav, name, ITEM_W)
                btn.grid(row=row_i, column=0, sticky="w", padx=8, pady=1)
                row_i += 1

        # ── Bottom area: byline ──────────────────────────────────────────────
        bottom = ctk.CTkFrame(sidebar, fg_color="transparent")
        bottom.grid(row=3, column=0, sticky="ew", pady=(4, 14), padx=8)
        bottom.grid_columnconfigure(0, weight=1)

        ctk.CTkFrame(bottom, fg_color=BORDER, height=1
                     ).grid(row=0, column=0, sticky="ew", pady=(0, 10))
        ctk.CTkLabel(bottom, text=f"v{APP_VERSION}",
                     font=F_MICRO(), text_color=INK_MUTE, anchor="w"
                     ).grid(row=1, column=0, sticky="w", padx=14)
        ctk.CTkLabel(bottom, text="cjm labs",
                     font=F_MICRO(), text_color=INK_MUTE, anchor="e"
                     ).grid(row=1, column=0, sticky="e", padx=14)

        # ── Content area ─────────────────────────────────────────────────────
        self._content = ctk.CTkFrame(self, fg_color="transparent")
        self._content.grid(row=0, column=1, sticky="nsew", padx=18, pady=(16, 8))
        self._content.grid_columnconfigure(0, weight=1)
        self._content.grid_rowconfigure(0, weight=1)

        for name, cls in TOOLS:
            # HomeTool needs a navigate callback so its cards can switch tools
            if cls is HomeTool:
                frame = cls(self._content, navigate=self._switch_tool)
            else:
                frame = cls(self._content)
            self._tool_frames[name] = frame

        # ── Status bar (uses pack — grid_propagate(False) breaks labels) ────
        # Top hairline above the bar
        ctk.CTkFrame(self, fg_color=BORDER, height=1
                     ).grid(row=1, column=1, sticky="ew")
        statusbar = ctk.CTkFrame(self, fg_color=BG_PANEL, corner_radius=0)
        statusbar.grid(row=2, column=1, sticky="ew")

        self._status_left = ctk.CTkLabel(statusbar, text="Ready",
                                          font=_font(9, "normal"),
                                          text_color=INK_DIM, anchor="w")
        self._status_left.pack(side="left", padx=14, pady=5)

        self._status_clock = ctk.CTkLabel(statusbar, text="",
                                           font=_font(9, "normal", "Cascadia Mono"),
                                           text_color=INK_MUTE)
        self._status_clock.pack(side="right", padx=(8, 14), pady=5)

        self._status_ram = ctk.CTkLabel(statusbar, text="",
                                         font=_font(9, "normal", "Cascadia Mono"),
                                         text_color=INK_DIM)
        self._status_ram.pack(side="right", padx=10, pady=5)

        self._status_disk = ctk.CTkLabel(statusbar, text="",
                                          font=_font(9, "normal", "Cascadia Mono"),
                                          text_color=INK_DIM)
        self._status_disk.pack(side="right", padx=10, pady=5)

    def _make_nav_item(self, parent, name: str, width: int) -> ctk.CTkFrame:
        """Build a sidebar nav item as a CTkButton + accent stripe overlay.
        CTkButton renders far more reliably than nested CTkFrame+CTkLabel."""
        # Container frame so we can overlay the indicator bar
        container = ctk.CTkFrame(parent, fg_color="transparent",
                                  width=width, height=36)
        container.grid_propagate(False)
        container.grid_columnconfigure(0, weight=1)

        icon_glyph = TOOL_ICONS.get(name, "·")

        # Button — text contains icon + name, padded for the indicator
        btn = ctk.CTkButton(
            container,
            text=f"   {icon_glyph}    {name}",
            anchor="w",
            font=_font(11, "normal", "Segoe UI"),
            fg_color="transparent",
            hover_color=BG_PANEL,
            text_color=INK_DIM,
            corner_radius=R_BASE,
            height=36,
            border_spacing=4,
            command=lambda n=name: self._switch_tool(n),
        )
        btn.grid(row=0, column=0, sticky="ew")

        # Left indicator bar — placed on top of the button via `place`
        bar = ctk.CTkFrame(container, fg_color="transparent",
                            width=3, height=20, corner_radius=2)
        bar.place(relx=0, rely=0.5, anchor="w", x=2)

        self._sidebar_btns[name] = (container, btn, bar)
        return container

    def _switch_tool(self, name: str):
        if name == self._active_tool:
            return

        # Deactivate previous
        if self._active_tool and self._active_tool in self._tool_frames:
            self._tool_frames[self._active_tool].grid_remove()
            _, btn, bar = self._sidebar_btns[self._active_tool]
            btn.configure(fg_color="transparent",
                           text_color=INK_DIM,
                           font=_font(11, "normal", "Segoe UI"))
            bar.configure(fg_color="transparent")

        # Activate new
        self._active_tool = name
        frame = self._tool_frames[name]
        frame.grid(row=0, column=0, sticky="nsew")

        _, btn, bar = self._sidebar_btns[name]
        btn.configure(fg_color=ACCENT_SOFT,
                       text_color=INK,
                       font=_font(11, "bold", "Segoe UI"))
        bar.configure(fg_color=ACCENT)

        if hasattr(self, "_status_left"):
            self._status_left.configure(text=name)

        if hasattr(frame, "_on_show"):
            frame._on_show()

    def _tick_status(self):
        # Disk free for C:
        try:
            usage = shutil.disk_usage("C:\\")
            pct_free = usage.free / usage.total * 100 if usage.total else 0
            self._status_disk.configure(
                text=f"C:\\  {fmt_size(usage.free)} free  ({pct_free:.0f}%)")
        except OSError:
            self._status_disk.configure(text="")

        # RAM
        try:
            total, avail = _get_ram_info()
            used_pct = (total - avail) / total * 100 if total else 0
            self._status_ram.configure(
                text=f"RAM  {used_pct:.0f}%  ·  {fmt_size(avail)} free")
        except Exception:
            self._status_ram.configure(text="")

        # Clock
        self._status_clock.configure(text=datetime.now().strftime("%H:%M"))

        # Re-tick every 5s
        self.after(5000, self._tick_status)


# ── Entry point ───────────────────────────────────────────────────────────────

def main():
    app = WinScanApp()
    app.mainloop()


if __name__ == "__main__":
    main()
