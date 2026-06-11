"""WinScan — Tool 5: Installed Programs."""

import winreg

import customtkinter as ctk

from app.theme import _font, ACCENT, BG_PANEL, BORDER, INFO, INK, INK_DIM, R_BASE
from app.helpers import fmt_size
from app.widgets import make_page_header, make_tree, Tile


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
