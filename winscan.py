"""
WinScan — Windows System Toolkit
Powered by Google Gemini AI
"""

import ctypes

# Make the process DPI-aware as the VERY FIRST thing, before importing Tk/CTk.
# This must win the race before Tk creates its first window — once an HWND
# exists, Windows refuses to change DPI awareness, so a later call is silently
# ignored (which is why earlier attempts had no effect). PerMonitorV2 = (-4).
try:
    ctypes.windll.user32.SetProcessDpiAwarenessContext(ctypes.c_void_p(-4))
except (AttributeError, OSError):
    try:
        ctypes.windll.shcore.SetProcessDpiAwareness(2)
    except (AttributeError, OSError):
        pass

from app.main import main

if __name__ == "__main__":
    main()
