"""WinScan — design tokens: colours, radii, icons, DPI scale and fonts."""

import customtkinter as ctk


# ── Design tokens ─────────────────────────────────────────────────────────────
#
# Identity: "instrument panel" — deep slate base, multiple saturated semantic
# colours used purposefully (green for safe, amber for caution, red for danger,
# cobalt for brand/action). High-contrast ink. Names kept stable so existing
# widgets keep rendering without churn — only the values changed.

# Surfaces — deep slate with subtle hue, layered depth (3+ levels)
BG_DEEP    = "#0d1117"   # app canvas
BG_CARD    = "#161b22"   # panels
BG_ROW     = "#1c232e"   # raised cards / rows
BG_ROW_A   = "#262d3a"   # hover / second alt
SIDEBAR_BG = "#0a0d12"   # sidebar (deepest)
BORDER     = "#2a3140"   # hairline
BORDER_HI  = "#3d4555"   # focus / strong divider

# Ink — high contrast, slightly cool
TEXT_MAIN  = "#f0f3f8"   # primary
TEXT_DIM   = "#9da7b3"   # secondary
TEXT_MUTE  = "#6b7484"   # tertiary

# Brand — cobalt blue. Reads as Windows-native + professional tool.
ACCENT     = "#4d9eff"   # primary brand / action
ACCENT_DIM = "#1f4d80"   # pressed / border
ACCENT_SOFT = "#0e2640"  # active item bg tint

# Semantic — saturated, visible at a glance
GREEN      = "#5ed590"   # safe / success / OK
ORANGE     = "#f0b340"   # warning (now distinct from brand)
ORANGE_SOFT = "#3a2c10"  # warning bg tint
RED_SOFT   = "#ff7070"   # danger inline (popping)
RED_BTN    = "#c94545"   # destructive button
RED_BTN_H  = "#e5605b"   # destructive hover
INFO       = "#7eb6ff"   # info accent
PURPLE     = "#b687f5"   # extra category accent (used sparingly)

# Aliases (semantic re-export)
BG_BASE     = BG_DEEP
BG_PANEL    = BG_CARD
BG_RAISED   = BG_ROW
BG_HOVER    = BG_ROW_A
INK         = TEXT_MAIN
INK_DIM     = TEXT_DIM
INK_MUTE    = TEXT_MUTE
SIGNAL      = ACCENT
OK          = GREEN
WARN        = ORANGE
DANGER      = RED_SOFT

# Geometry — small rounding (utility tool feel, not consumer app)
R_SHARP     = 3
R_BASE      = 5
R_LARGE     = 8

# Icon glyphs (Segoe UI Symbol / Segoe MDL2 Assets compatible — Windows native)
# Single-character symbols that render reliably on Windows. Keep this small.
ICON_OVERVIEW    = "◉"   # focus / info
ICON_DISK        = "◓"   # disk usage
ICON_CLEANUP     = "✦"   # sparkle
ICON_DUPLICATES  = "⊜"   # equal
ICON_LARGE       = "⬡"   # hex / large
ICON_PRIVACY     = "◐"   # half
ICON_PROGRAMS    = "▦"   # grid
ICON_STARTUP     = "▶"   # play
ICON_TASKS       = "◷"   # clock
ICON_NETWORK     = "⌬"   # network

ICON_HOME        = "⌂"   # house

TOOL_ICONS = {
    "Home":        ICON_HOME,
    "Overview":    ICON_OVERVIEW,
    "Disk Map":    ICON_DISK,
    "Cleanup":     ICON_CLEANUP,
    "Duplicates":  ICON_DUPLICATES,
    "Large Files": ICON_LARGE,
    "Privacy":     ICON_PRIVACY,
    "Programs":    ICON_PROGRAMS,
    "Startup":     ICON_STARTUP,
    "Tasks":       ICON_TASKS,
    "Network":     ICON_NETWORK,
}

# Colour-code each tool for the sidebar accent dot (multi-colour, not all amber)
TOOL_COLOURS = {
    "Home":        ACCENT,
    "Overview":    INFO,
    "Disk Map":    ACCENT,
    "Cleanup":     GREEN,
    "Duplicates":  PURPLE,
    "Large Files": ACCENT,
    "Privacy":     ORANGE,
    "Programs":    INFO,
    "Startup":     GREEN,
    "Tasks":       PURPLE,
    "Network":     INFO,
}

# Type scale — DPI-aware.
#
# We force CTk's own scaling to 1.0 (see GUI section) so geometry maps 1:1 to
# physical pixels and content can't overflow the window. The downside is that
# fonts then render at their raw pixel size — tiny on a high-DPI display. So we
# detect the real DPI here and scale fonts (and key widget dimensions) up by it.
# Base sizes are also generous for legibility (Craig has nystagmus + dyspraxia).
#
# Use the F_* helpers everywhere; never hand-roll `ctk.CTkFont(...)`.

# CTk auto-scales fonts/widgets for DPI, so these are RAW logical sizes and
# _px is a no-op. Base sizes are generous for legibility (nystagmus + dyspraxia).
DPI_SCALE = 1.0

def _px(n: int) -> int:
    return int(n)

def _font(size_pt: int, weight: str = "normal", family: str = "Segoe UI"):
    """Build a font at a logical point size — CTk applies DPI scaling."""
    return ctk.CTkFont(family, size_pt, weight)

def F_DISPLAY(): return _font(22, "bold")
def F_H1():      return _font(17, "bold")
def F_H2():      return _font(14, "bold")
def F_BODY():    return _font(13)
def F_BODY_B():  return _font(13, "bold")
def F_SMALL():   return _font(12)
def F_MICRO():   return _font(11)
def F_MONO(s=13):return _font(s, "normal", "Cascadia Mono")
def F_MONO_B(s=13):return _font(s, "bold", "Cascadia Mono")
