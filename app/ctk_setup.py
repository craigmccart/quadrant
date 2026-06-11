"""WinScan — CustomTkinter global setup (appearance + DPI scaling). Import for side effects."""

import ctypes

import customtkinter as ctk


# ══════════════════════════════════════════════════════════════════════════════
# GUI
# ══════════════════════════════════════════════════════════════════════════════

ctk.set_appearance_mode("dark")
ctk.set_default_color_theme("blue")

# DPI FIX — two independent CTk scaling axes:
#   • window_scaling controls the WINDOW geometry (geometry × detected_dpi × this)
#   • widget_scaling controls FONT/WIDGET size (size × detected_dpi × this)
# Every earlier attempt only touched widget_scaling / deactivate_automatic_dpi.
# The real culprit was window geometry: CTk multiplied 1180 × 1.75 = 2065 logical
# px, wider than the ~1463-logical desktop at 175%, so ~40% spilled off-screen.
# Lowering window_scaling to ~1/DPI cancels that multiply (geometry lands back at
# the intended physical size and content fits), while widget_scaling stays high
# so fonts/buttons remain large and crisp (good for nystagmus).
_DPI = 1.0
try:
    _DPI = ctypes.windll.user32.GetDpiForSystem() / 96.0
except (AttributeError, OSError):
    pass
if _DPI > 1.05:
    ctk.set_window_scaling(1.0 / _DPI)   # cancel the geometry over-multiply
    ctk.set_widget_scaling(1.0)          # keep fonts/widgets at full DPI size
