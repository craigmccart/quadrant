"""WinScan — Home dashboard."""

import customtkinter as ctk

from app.theme import (
    ACCENT, BG_HOVER, BG_RAISED, GREEN, ICON_CLEANUP, ICON_DISK, ICON_DUPLICATES,
    ICON_NETWORK, ICON_OVERVIEW, ICON_PRIVACY, INFO, ORANGE, PURPLE,
)
from app.widgets import ActionCard, make_page_header


# ── Home dashboard ────────────────────────────────────────────────────────────

class HomeTool(ctk.CTkFrame):
    """Fortect-style landing page: a 2-column grid of big action cards that
    launch the most common tasks. The app's front door."""

    def __init__(self, master, navigate):
        super().__init__(master, fg_color="transparent")
        self._navigate = navigate   # callback: switch to a named tool
        self.grid_columnconfigure(0, weight=1)
        self.grid_rowconfigure(2, weight=1)

        hdr, rule = make_page_header(
            self, title="WinScan",
            subtitle="Keep your PC fast, clean, and secure — pick a task to begin.")
        hdr.grid(row=0, column=0, sticky="ew")
        rule.grid(row=1, column=0, sticky="ew", pady=(2, 14))

        # Scrollable 2-column grid of action cards (Fortect-style). Now that
        # the DPI clipping is fixed (winfo == physical), grid uniform splits
        # the columns correctly and nothing spills off the right.
        body = ctk.CTkScrollableFrame(self, fg_color="transparent",
                                       scrollbar_button_color=BG_RAISED,
                                       scrollbar_button_hover_color=BG_HOVER)
        body.grid(row=2, column=0, sticky="nsew")
        body.grid_columnconfigure(0, weight=1, uniform="cards")
        body.grid_columnconfigure(1, weight=1, uniform="cards")

        # Definitions: (icon, colour, title, description, button, target tool)
        cards = [
            (ICON_CLEANUP, GREEN, "Clean up disk",
             "Remove temp files, caches, and old downloads safely.",
             "Open Cleanup", "Cleanup"),
            (ICON_DISK, ACCENT, "Analyse disk space",
             "See what's using your storage, folder by folder.",
             "Open Disk Map", "Disk Map"),
            (ICON_DUPLICATES, PURPLE, "Find duplicates",
             "Reclaim space from identical files across folders.",
             "Find Duplicates", "Duplicates"),
            (ICON_OVERVIEW, INFO, "System health",
             "Check CPU, memory, graphics, and drives at a glance.",
             "View Overview", "Overview"),
            (ICON_PRIVACY, ORANGE, "Privacy sweep",
             "Clear recent files, jump lists, and history traces.",
             "Open Privacy", "Privacy"),
            (ICON_NETWORK, INFO, "Network inspector",
             "See active connections and their owning processes.",
             "Open Network", "Network"),
        ]

        for i, (icon, colour, title, desc, btn, target) in enumerate(cards):
            card = ActionCard(body, icon=icon, icon_colour=colour,
                              title=title, description=desc,
                              button_text=btn,
                              command=lambda t=target: self._navigate(t),
                              button_colour=colour)
            row, col = divmod(i, 2)
            pad = (0, 6) if col == 0 else (6, 0)
            card.frame.grid(row=row, column=col, sticky="nsew",
                            padx=pad, pady=(0, 12))

    def _on_show(self):
        pass
