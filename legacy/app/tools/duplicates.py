"""WinScan — Tool 7: Duplicate Finder."""

import os
import hashlib
import subprocess
import threading

import customtkinter as ctk
from tkinter import filedialog

from app.theme import (ACCENT_HOVER, ON_ACCENT, 
    _font, ACCENT, BG_HOVER, BG_PANEL, BG_RAISED, BORDER, INK, INK_DIM, INK_MUTE,
    MONO_FAMILY, PURPLE, R_BASE,
)
from app.helpers import expand, fmt_size
from app.widgets import make_page_header, make_tree, Tile


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
                      fg_color=ACCENT, hover_color=ACCENT_HOVER,
                      text_color=ON_ACCENT, corner_radius=R_BASE,
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
        self._path_entry = ctk.CTkEntry(ctrl, height=32, font=_font(11, "normal", MONO_FAMILY),
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
