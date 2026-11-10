"""WinScan — design tokens: colours, radii, icons, DPI scale and fonts."""

import customtkinter as ctk


# ── Design tokens ─────────────────────────────────────────────────────────────
#
# Identity: premium security-suite dark (Fortect reference). Deep desaturated
# navy surfaces with layered depth, ONE consistent mint-green CTA everywhere
# (a single green across all buttons is what reads premium; per-card rainbow
# CTAs read AI-generated), cool blue-grey secondary text, and coloured glyphs
# inside neutral dark badges. Names kept stable — only the values change.

# Surfaces — deep navy, cards one step lighter, quiet navy hairlines
BG_DEEP    = "#161d2e"   # app canvas
BG_CARD    = "#1f2739"   # panels / cards
BG_ROW     = "#242e44"   # raised cards / rows / nested chips
BG_ROW_A   = "#2a3550"   # hover / second alt
SIDEBAR_BG = "#131927"   # sidebar (deepest)
BORDER     = "#2b3550"   # hairline
BORDER_HI  = "#3a4767"   # focus / strong divider / badge outlines

# Ink — near-white headings, cool blue-grey body (Fortect's text ramp)
TEXT_MAIN  = "#f2f5fa"   # primary
TEXT_DIM   = "#9aa5bd"   # secondary
TEXT_MUTE  = "#67718c"   # tertiary

# Brand/CTA — Fortect mint green. THE action colour: every primary button,
# toggle, progress bar and the active-nav indicator use this one green.
ACCENT     = "#3ddc84"   # primary action
ACCENT_DIM = "#1d7a4b"   # pressed / checkbox borders
ACCENT_SOFT = "#1a3328"  # active nav pill / selected rows (dark green tint)
ACCENT_HOVER = "#2fc873" # primary button hover
ON_ACCENT  = "#0b1526"   # dark navy text on mint (Spotify/WhatsApp pattern)

# Semantic — bright enough to read as text on navy
GREEN      = "#3ddc84"   # safe / success (same family as ACCENT, intentional)
ORANGE     = "#f5b84f"   # warning
ORANGE_SOFT = "#3a2d12"  # warning bg tint
RED_SOFT   = "#ff6b6b"   # danger inline
RED_BTN    = "#d64545"   # destructive button
RED_BTN_H  = "#e25e5e"   # destructive hover (brightens — dark-theme rule)
INFO       = "#62a6ff"   # informational blue (Fortect's icon blue)
PURPLE     = "#a78bfa"   # AI features only, used sparingly

# Dark tints per accent — icon-badge / risk-badge backgrounds
TINT = {
    ACCENT: "#143323", GREEN: "#143323", ORANGE: "#3a2d12",
    PURPLE: "#272040", INFO: "#16283e", RED_SOFT: "#3a1a1a",
}
# Hover for filled colour buttons
HOVER_DARK = {
    ACCENT: "#2fc873", GREEN: "#2fc873", ORANGE: "#d99c2c",
    PURPLE: "#8f6fe8", INFO: "#4a8fe8", RED_SOFT: "#e25e5e",
}

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

# Geometry — softer rounding (premium suite feel, Fortect reference)
R_SHARP     = 4
R_BASE      = 8
R_LARGE     = 10

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

# Monospace family. Consolas ships on every Windows install; Cascadia Mono
# does not, so on a clean machine it silently fell back to a proportional font.
MONO_FAMILY = "Consolas"
def F_MONO(s=13):return _font(s, "normal", MONO_FAMILY)
def F_MONO_B(s=13):return _font(s, "bold", MONO_FAMILY)
