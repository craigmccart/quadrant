"""WinScan — Tool 10: Disk Space Analyzer."""

import os
import threading
from pathlib import Path

import customtkinter as ctk

from app.theme import (
    ACCENT, ACCENT_DIM, ACCENT_SOFT, BG_BASE, BG_HOVER, BG_PANEL, BG_RAISED, BORDER,
    F_BODY, F_BODY_B, F_MICRO, F_MONO, F_MONO_B, F_SMALL, GREEN, INK, INK_DIM,
    INK_MUTE, ORANGE, R_BASE, R_SHARP, RED_SOFT,
)
from app.helpers import dir_size, fmt_size, size_color
from app.widgets import make_page_header


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
