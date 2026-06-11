"""WinScan — app shell: tool registry, main window, entry point."""

import os
import sys
import shutil
from pathlib import Path
from typing import Optional
from datetime import datetime

import customtkinter as ctk
import tkinter as tk

from app import ctk_setup  # noqa: F401  (appearance + DPI scaling side effects)
from app.config import APP_VERSION
from app.theme import (
    _font, _px, ACCENT, ACCENT_SOFT, BG_BASE, BG_PANEL, BORDER, F_MICRO, INK,
    INK_DIM, INK_MUTE, R_BASE, SIDEBAR_BG, TOOL_ICONS,
)
from app.helpers import _get_ram_info, fmt_size
from app.widgets import _setup_treeview_style
from app.tools.cleanup import CleanupTool
from app.tools.system_info import SystemInfoTool
from app.tools.large_files import LargeFileTool
from app.tools.startup import StartupTool
from app.tools.programs import ProgramsTool
from app.tools.network import NetworkTool
from app.tools.duplicates import DuplicateTool
from app.tools.privacy import PrivacyTool
from app.tools.tasks import TasksTool
from app.tools.disk import DiskTool
from app.tools.home import HomeTool


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
