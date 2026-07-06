"""WinScan — Tool 9: Scheduled Tasks."""

import csv
import io
import subprocess
import threading

import customtkinter as ctk

from app.theme import _font, ACCENT, ACCENT_HOVER, ON_ACCENT, BG_HOVER, BORDER, INK_DIM, PURPLE, R_BASE
from app.widgets import make_page_header, make_tree, Tile


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
                      fg_color=ACCENT, hover_color=ACCENT_HOVER,
                      text_color=ON_ACCENT, corner_radius=R_BASE,
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
