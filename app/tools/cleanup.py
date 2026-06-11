"""WinScan — Tool 1: Cleanup."""

import os
import subprocess
import threading
from pathlib import Path
from typing import Optional

import customtkinter as ctk
import tkinter as tk
from tkinter import messagebox

from app.config import DRILL_DOWN_CAP, GEMINI_MODEL
from app.theme import (
    _font, _px, ACCENT, ACCENT_DIM, BG_BASE, BG_CARD, BG_DEEP, BG_HOVER, BG_PANEL,
    BG_RAISED, BORDER, DPI_SCALE, F_BODY, F_BODY_B, F_H2, F_MICRO, F_MONO, F_MONO_B,
    F_SMALL, GREEN, INK, INK_DIM, INK_MUTE, ORANGE, PURPLE, R_BASE, R_SHARP,
    RED_BTN, RED_BTN_H, RED_SOFT, TEXT_DIM,
)
from app.helpers import fmt_size, size_color
from app.widgets import _StringVarShim, make_page_header
from app.scanner import delete_category, scan_all, ScanResult
from app.gemini import ask_gemini
from app.settings import key_file


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
        return key_file()

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
