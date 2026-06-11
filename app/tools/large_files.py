"""WinScan — Tool 3: Large File Hunter."""

import os
import subprocess
import threading
from pathlib import Path
from datetime import datetime

import customtkinter as ctk
from tkinter import filedialog

from app.theme import (
    _font, ACCENT, BG_HOVER, BG_PANEL, BG_RAISED, BORDER, INK, INK_DIM, INK_MUTE,
    R_BASE,
)
from app.helpers import expand, fmt_size
from app.widgets import make_page_header, make_tree, Tile


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
