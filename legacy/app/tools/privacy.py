"""WinScan — Tool 8: Privacy Sweep."""

import shutil
import threading
import winreg

import customtkinter as ctk
from tkinter import messagebox

from app.theme import (ACCENT_HOVER, ON_ACCENT, 
    _font, ACCENT, ACCENT_DIM, BG_HOVER, BG_RAISED, BORDER, F_BODY_B, F_MONO_B,
    F_SMALL, INK, INK_DIM, INK_MUTE, ORANGE, R_BASE, R_SHARP, RED_BTN, RED_BTN_H,
)
from app.helpers import dir_size, expand, fmt_size, size_color
from app.widgets import make_page_header, Tile


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
                      fg_color=ACCENT, hover_color=ACCENT_HOVER,
                      text_color=ON_ACCENT, corner_radius=R_BASE,
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
                                         text_color=ON_ACCENT, corner_radius=R_BASE,
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
