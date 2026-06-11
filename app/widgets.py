"""WinScan — shared UI building blocks: treeviews, page headers, tiles, cards."""

import customtkinter as ctk
import tkinter as tk
from tkinter import ttk

from app.theme import (
    _font, _px, ACCENT, ACCENT_SOFT, BG_BASE, BG_CARD, BG_DEEP, BG_HOVER, BG_PANEL,
    BG_RAISED, BORDER, GREEN, INFO, INK, INK_DIM, INK_MUTE, ORANGE, PURPLE, R_BASE,
    R_LARGE, RED_SOFT,
)


def treeview_sort(tree: ttk.Treeview, col: str, reverse: bool = False):
    data = [(tree.set(k, col), k) for k in tree.get_children("")]
    try:
        data.sort(key=lambda x: float(x[0].replace(",", "").replace(" ", "")), reverse=reverse)
    except ValueError:
        data.sort(key=lambda x: x[0].lower(), reverse=reverse)
    for i, (_, k) in enumerate(data):
        tree.move(k, "", i)
    tree.heading(col, command=lambda: treeview_sort(tree, col, not reverse))

def _setup_treeview_style():
    # Fonts/row height scaled to physical pixels (CTk scaling is 1.0).
    style = ttk.Style()
    style.theme_use("clam")
    style.configure("Dark.Treeview",
                    background=BG_BASE, foreground=INK,
                    fieldbackground=BG_BASE, borderwidth=0,
                    font=("Segoe UI", -_px(11)), rowheight=_px(30))
    style.configure("Dark.Treeview.Heading",
                    background=BG_PANEL, foreground=INK_DIM,
                    borderwidth=0, relief="flat",
                    font=("Segoe UI", -_px(10), "bold"),
                    padding=(_px(8), _px(8)))
    style.map("Dark.Treeview",
              background=[("selected", ACCENT_SOFT)],
              foreground=[("selected", INK)])
    style.map("Dark.Treeview.Heading",
              background=[("active", BG_HOVER)],
              foreground=[("active", INK)])

def make_tree(parent, columns: list[tuple[str, int]], height: int = 20) -> tuple[ctk.CTkFrame, ttk.Treeview]:
    """Create a dark-themed Treeview with scrollbar. columns = [(name, width), ...]"""
    frame = ctk.CTkFrame(parent, fg_color="transparent")
    col_ids = [c[0] for c in columns]
    tree = ttk.Treeview(frame, columns=col_ids, show="headings",
                        style="Dark.Treeview", selectmode="extended", height=height)
    vsb = tk.Scrollbar(frame, orient="vertical", command=tree.yview,
                       bg=BG_CARD, troughcolor=BG_DEEP, bd=0, width=10)
    hsb = tk.Scrollbar(frame, orient="horizontal", command=tree.xview,
                       bg=BG_CARD, troughcolor=BG_DEEP, bd=0, width=10)
    tree.configure(yscrollcommand=vsb.set, xscrollcommand=hsb.set)
    for name, width in columns:
        tree.heading(name, text=name, command=lambda c=name: treeview_sort(tree, c))
        tree.column(name, width=width, minwidth=50)
    tree.grid(row=0, column=0, sticky="nsew")
    vsb.grid(row=0, column=1, sticky="ns")
    hsb.grid(row=1, column=0, sticky="ew")
    frame.grid_columnconfigure(0, weight=1)
    frame.grid_rowconfigure(0, weight=1)
    return frame, tree

def make_page_header(parent, title: str, subtitle: str = "",
                     action_text: str = None, action_cmd=None,
                     secondary_text: str = None, secondary_cmd=None) -> tuple:
    """Standard tool page header — title, subtitle, optional inline actions on the right.

    Uses pack() inside the header frame because grid + grid_propagate(False) was
    causing CTkLabel + CTkButton children to fail to render in some configurations.
    """
    hdr = ctk.CTkFrame(parent, fg_color="transparent")

    # Right side: actions packed from the right. Widths scale with DPI so
    # the buttons stay proportional to the larger DPI-scaled text.
    if action_text:
        ctk.CTkButton(hdr, text=action_text,
                      width=_px(100), height=_px(34),
                      font=_font(11, "bold", "Segoe UI"),
                      fg_color=ACCENT, hover_color="#3a8aef",
                      text_color="#0a1322", corner_radius=R_BASE,
                      command=action_cmd
                      ).pack(side="right", padx=(8, 0), pady=8)
    if secondary_text:
        ctk.CTkButton(hdr, text=secondary_text,
                      width=_px(100), height=_px(34),
                      font=_font(11, "normal", "Segoe UI"),
                      fg_color="transparent", hover_color=BG_HOVER,
                      text_color=INK_DIM,
                      border_width=1, border_color=BORDER,
                      corner_radius=R_BASE, command=secondary_cmd
                      ).pack(side="right", padx=(8, 0), pady=8)

    # Left side: title + subtitle packed left
    text_col = ctk.CTkFrame(hdr, fg_color="transparent")
    text_col.pack(side="left", fill="x", expand=True, pady=4)

    ctk.CTkLabel(text_col, text=title,
                 font=_font(20, "bold", "Segoe UI"),
                 text_color=INK, anchor="w"
                 ).pack(anchor="w")
    if subtitle:
        ctk.CTkLabel(text_col, text=subtitle,
                     font=_font(11, "normal", "Segoe UI"),
                     text_color=INK_DIM, anchor="w"
                     ).pack(anchor="w", pady=(2, 0))

    # Hairline under header
    rule = ctk.CTkFrame(parent, fg_color=BORDER, height=1)
    return hdr, rule


class _StringVarShim:
    """Adapter that lets the rest of the code call .get()/.delete()/.insert()
    on what used to be a CTkEntry but is now a hidden StringVar (the key entry
    moved into a Settings dialog)."""
    def __init__(self, var: ctk.StringVar):
        self._var = var
    def get(self) -> str:
        return self._var.get()
    def delete(self, *_args, **_kwargs):
        self._var.set("")
    def insert(self, _idx, value: str):
        self._var.set(value)


class Tile:
    """Reusable dashboard tile. The standard pattern across all tools:
    a panel with a small-caps title (with optional accent dot) and a body
    you can populate with KPIs, gauges, lists, or arbitrary widgets.

    Use one of the convenience methods (.kpi, .gauge, .row, .text) for
    common content, or pack/grid into `.body` directly for anything custom.
    """
    def __init__(self, parent, title: str, accent: str = None,
                 height: int = None, width: int = None,
                 padding_y: tuple = (12, 14)):
        self.frame = ctk.CTkFrame(parent,
                                   fg_color=BG_PANEL,
                                   corner_radius=R_BASE,
                                   border_width=1, border_color=BORDER,
                                   width=width if width else 0,
                                   height=height if height else 0)
        if height or width:
            self.frame.grid_propagate(False)
            self.frame.pack_propagate(False)

        # Header: optional accent dot + small-caps mono title
        header = ctk.CTkFrame(self.frame, fg_color="transparent")
        header.pack(fill="x", padx=14, pady=(padding_y[0], 6))
        if accent:
            ctk.CTkFrame(header, fg_color=accent,
                         width=8, height=8, corner_radius=4
                         ).pack(side="left", padx=(0, 8))
        ctk.CTkLabel(header, text=title.upper(),
                     font=_font(9, "bold", "Cascadia Mono"),
                     text_color=INK_MUTE
                     ).pack(side="left")

        # Body — caller adds content here
        self.body = ctk.CTkFrame(self.frame, fg_color="transparent")
        self.body.pack(fill="both", expand=True, padx=14, pady=(0, padding_y[1]))

    # ── Convenience content methods ──────────────────────────────────

    def kpi(self, value: str, sub: str = "", value_colour: str = None,
            value_size: int = 20):
        """Big number + optional subline below. Returns (value_lbl, sub_lbl)
        so callers can update the labels later via .configure(text=...)."""
        self.value_lbl = ctk.CTkLabel(self.body, text=value,
                                       font=_font(value_size, "bold"),
                                       text_color=value_colour or INK, anchor="w")
        self.value_lbl.pack(anchor="w")
        self.sub_lbl = ctk.CTkLabel(self.body, text=sub,
                                     font=_font(11), text_color=INK_DIM,
                                     anchor="w")
        self.sub_lbl.pack(anchor="w", pady=(4, 0))
        return self.value_lbl, self.sub_lbl

    def gauge(self, value: str, pct: float, colour: str, sub: str = ""):
        """Big value + thin progress bar + optional subline."""
        ctk.CTkLabel(self.body, text=value,
                     font=_font(16, "bold"),
                     text_color=INK, anchor="w"
                     ).pack(anchor="w")
        bar = ctk.CTkProgressBar(self.body, height=6,
                                  fg_color=BG_BASE,
                                  progress_color=colour,
                                  corner_radius=2)
        bar.set(max(0.0, min(1.0, pct)))
        bar.pack(fill="x", pady=(8, 0))
        if sub:
            ctk.CTkLabel(self.body, text=sub,
                         font=_font(11), text_color=INK_DIM, anchor="w"
                         ).pack(anchor="w", pady=(6, 0))

    def text(self, text: str, colour: str = None):
        """Plain paragraph text body."""
        ctk.CTkLabel(self.body, text=text,
                     font=_font(11), text_color=colour or INK_DIM,
                     anchor="w", justify="left"
                     ).pack(anchor="w", fill="x")

    def grid(self, **kwargs):  return self.frame.grid(**kwargs)
    def pack(self, **kwargs):  return self.frame.pack(**kwargs)


class ActionCard:
    """Fortect-style action row: a wide panel with a coloured icon chip, a
    title + description on the left, and one big prominent button on the right.

    Horizontal layout (rather than a 2-column grid of tall cards) because it
    packs more actions per screen and sidesteps CTk's high-DPI 2-column quirk.
    """
    def __init__(self, parent, icon: str, icon_colour: str,
                 title: str, description: str,
                 button_text: str, command,
                 button_colour: str = None):
        button_colour = button_colour or ACCENT
        self.frame = ctk.CTkFrame(parent, fg_color=BG_RAISED,
                                   corner_radius=R_LARGE,
                                   border_width=1, border_color=BORDER)

        # Icon chip on its own line (gives the title the full card width)
        chip = ctk.CTkFrame(self.frame, fg_color=self._tint(icon_colour),
                            width=48, height=48, corner_radius=R_BASE)
        chip.pack(anchor="w", padx=20, pady=(20, 0))
        chip.pack_propagate(False)
        ctk.CTkLabel(chip, text=icon,
                     font=_font(22, "normal", "Segoe UI"),
                     text_color=icon_colour).pack(expand=True)

        # Title + description. wraplength is set dynamically from the card's
        # real width via <Configure> so text always fits regardless of DPI
        # scaling or column width.
        self._title_lbl = ctk.CTkLabel(self.frame, text=title,
                     font=_font(17, "bold", "Segoe UI"),
                     text_color=INK, anchor="w", justify="left", wraplength=200)
        self._title_lbl.pack(anchor="w", fill="x", padx=20, pady=(12, 0))

        self._desc_lbl = ctk.CTkLabel(self.frame, text=description,
                     font=_font(12, "normal", "Segoe UI"),
                     text_color=INK_DIM, anchor="w", justify="left", wraplength=200)
        self._desc_lbl.pack(anchor="w", fill="x", padx=20, pady=(6, 0))

        self.frame.bind("<Configure>", self._on_resize)

        # Spacer pushes the button to the bottom for even card heights
        ctk.CTkFrame(self.frame, fg_color="transparent", height=4
                     ).pack(fill="both", expand=True)

        # Big prominent button, full card width
        ctk.CTkButton(self.frame, text=button_text,
                      font=_font(13, "bold", "Segoe UI"),
                      height=46, corner_radius=R_BASE,
                      fg_color=button_colour, hover_color=self._darken(button_colour),
                      text_color="#0a1322",
                      command=command
                      ).pack(fill="x", padx=20, pady=(16, 20))

    def _on_resize(self, event):
        # event.width is in scaled tk pixels; CTk multiplies wraplength by the
        # widget-scaling factor again, so divide it out to avoid double-scaling.
        try:
            scale = ctk.ScalingTracker.get_widget_scaling(self.frame)
        except Exception:
            scale = 1.0
        wrap = max(120, int(event.width / scale) - 40)
        self._title_lbl.configure(wraplength=wrap)
        self._desc_lbl.configure(wraplength=wrap)

    @staticmethod
    def _tint(hex_colour: str) -> str:
        """A very dark tint of the accent for the icon chip background."""
        tints = {
            ACCENT: "#13243a", GREEN: "#13301f", ORANGE: "#322611",
            PURPLE: "#241733", INFO: "#16273d", RED_SOFT: "#331717",
        }
        return tints.get(hex_colour, BG_PANEL)

    @staticmethod
    def _darken(hex_colour: str) -> str:
        darks = {
            ACCENT: "#3a8aef", GREEN: "#4cbb78", ORANGE: "#d99c2c",
            PURPLE: "#9f6fe0", INFO: "#5a9be0", RED_SOFT: "#e5605b",
        }
        return darks.get(hex_colour, hex_colour)

    def grid(self, **kwargs):  return self.frame.grid(**kwargs)
    def pack(self, **kwargs):  return self.frame.pack(**kwargs)
