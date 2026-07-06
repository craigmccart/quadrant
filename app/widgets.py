"""WinScan — shared UI building blocks: treeviews, page headers, tiles, cards."""

import customtkinter as ctk
import tkinter as tk
from tkinter import ttk

from app.theme import (ACCENT_HOVER, BORDER_HI, HOVER_DARK, ON_ACCENT, TINT, 
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
                    # clam draws a light/dark bevel around the widget — flatten
                    # it to the surface colours so no pale rim shows on navy.
                    lightcolor=BG_CARD, darkcolor=BG_CARD, bordercolor=BORDER,
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
    """Create a themed Treeview inside a bordered card. columns = [(name, width), ...]

    Uses CTkScrollbar (theme-aware) — native tk.Scrollbar ignores bg colours on
    Windows and renders a glaring white strip on dark surfaces."""
    frame = ctk.CTkFrame(parent, fg_color=BG_CARD, corner_radius=R_BASE,
                         border_width=1, border_color=BORDER)
    col_ids = [c[0] for c in columns]
    tree = ttk.Treeview(frame, columns=col_ids, show="headings",
                        style="Dark.Treeview", selectmode="extended", height=height)
    vsb = ctk.CTkScrollbar(frame, orientation="vertical", command=tree.yview,
                           fg_color="transparent",
                           button_color=BG_HOVER, button_hover_color=BORDER_HI)
    hsb = ctk.CTkScrollbar(frame, orientation="horizontal", command=tree.xview,
                           fg_color="transparent",
                           button_color=BG_HOVER, button_hover_color=BORDER_HI)
    tree.configure(yscrollcommand=vsb.set, xscrollcommand=hsb.set)
    for name, width in columns:
        tree.heading(name, text=name, command=lambda c=name: treeview_sort(tree, c))
        tree.column(name, width=width, minwidth=50)
    tree.grid(row=0, column=0, sticky="nsew", padx=(10, 0), pady=10)
    vsb.grid(row=0, column=1, sticky="ns", pady=10)
    hsb.grid(row=1, column=0, sticky="ew", padx=(10, 0))
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
                      fg_color=ACCENT, hover_color=ACCENT_HOVER,
                      text_color=ON_ACCENT, corner_radius=R_BASE,
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
        ctk.CTkLabel(header, text=title,
                     font=_font(11, "bold", "Segoe UI"),
                     text_color=INK_DIM
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
        # Fortect card anatomy: title top-left, icon badge top-right,
        # blue-grey description, ONE mint-green pill CTA bottom-left.
        # button_colour is accepted for compatibility but ignored — every CTA
        # is the same green; per-card rainbow buttons read AI-generated.
        self.frame = ctk.CTkFrame(parent, fg_color=BG_CARD,
                                   corner_radius=R_LARGE,
                                   border_width=1, border_color=BORDER)

        # Header row: title left, badge right
        top = ctk.CTkFrame(self.frame, fg_color="transparent")
        top.pack(fill="x", padx=20, pady=(18, 0))

        badge = ctk.CTkFrame(top, fg_color=self._tint(icon_colour),
                             width=46, height=46, corner_radius=R_BASE,
                             border_width=1, border_color=BORDER_HI)
        badge.pack(side="right", anchor="n")
        badge.pack_propagate(False)
        ctk.CTkLabel(badge, text=icon,
                     font=_font(20, "normal", "Segoe UI"),
                     text_color=icon_colour).pack(expand=True)

        self._title_lbl = ctk.CTkLabel(top, text=title,
                     font=_font(16, "bold", "Segoe UI"),
                     text_color=INK, anchor="w", justify="left", wraplength=200)
        self._title_lbl.pack(side="left", anchor="n", pady=(6, 0))

        # Description. wraplength set dynamically from the card's real width
        # via <Configure> so text always fits regardless of DPI/column width.
        self._desc_lbl = ctk.CTkLabel(self.frame, text=description,
                     font=_font(12, "normal", "Segoe UI"),
                     text_color=INK_DIM, anchor="w", justify="left", wraplength=200)
        self._desc_lbl.pack(anchor="w", fill="x", padx=20, pady=(8, 0))

        self.frame.bind("<Configure>", self._on_resize)

        # Spacer pushes the button to the bottom for even card heights
        ctk.CTkFrame(self.frame, fg_color="transparent", height=4
                     ).pack(fill="both", expand=True)

        # The one green CTA (compact pill, not full-width web-button)
        ctk.CTkButton(self.frame, text=button_text,
                      font=_font(12, "bold", "Segoe UI"),
                      width=150, height=38, corner_radius=R_BASE,
                      fg_color=ACCENT, hover_color=ACCENT_HOVER,
                      text_color=ON_ACCENT,
                      command=command
                      ).pack(anchor="w", padx=20, pady=(16, 20))

    def _on_resize(self, event):
        # event.width is in scaled tk pixels; CTk multiplies wraplength by the
        # widget-scaling factor again, so divide it out to avoid double-scaling.
        try:
            scale = ctk.ScalingTracker.get_widget_scaling(self.frame)
        except Exception:
            scale = 1.0
        inner = max(120, int(event.width / scale) - 40)
        # The title shares its row with the 46px badge + gap
        self._title_lbl.configure(wraplength=max(100, inner - 62))
        self._desc_lbl.configure(wraplength=inner)

    @staticmethod
    def _tint(hex_colour: str) -> str:
        """Pale pastel chip background for an accent (see theme.TINT)."""
        return TINT.get(hex_colour, BG_PANEL)

    @staticmethod
    def _darken(hex_colour: str) -> str:
        """Hover for filled buttons (see theme.HOVER_DARK)."""
        return HOVER_DARK.get(hex_colour, hex_colour)

    def grid(self, **kwargs):  return self.frame.grid(**kwargs)
    def pack(self, **kwargs):  return self.frame.pack(**kwargs)
