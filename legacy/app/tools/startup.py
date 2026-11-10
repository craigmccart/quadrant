"""WinScan — Tool 4: Startup Manager."""

import subprocess
import winreg

import customtkinter as ctk

from app.theme import _font, ACCENT, ACCENT_HOVER, ON_ACCENT, BG_HOVER, BORDER, GREEN, INK_DIM, R_BASE
from app.helpers import expand
from app.widgets import make_page_header, make_tree, Tile


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
                      fg_color=ACCENT, hover_color=ACCENT_HOVER,
                      text_color=ON_ACCENT, corner_radius=R_BASE,
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
