"""WinScan — Tool 2: System Info."""

import os
import shutil
import subprocess

import customtkinter as ctk

from app.theme import (
    _font, ACCENT, BG_HOVER, BG_RAISED, GREEN, INFO, INK_DIM, INK_MUTE, ORANGE,
    PURPLE, R_BASE, RED_SOFT,
)
from app.helpers import _get_ram_info, _get_uptime_str, fmt_size
from app.widgets import make_page_header, Tile


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
