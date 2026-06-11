"""WinScan — settings storage: location of the saved Gemini API key."""

import sys
from pathlib import Path


def key_file() -> Path:
    """Path of the plain-text API key file (.winscan_key).

    Next to the EXE when frozen, otherwise next to winscan.py
    (the project root, one level above this package).
    """
    base = Path(sys.executable).parent if getattr(sys, "frozen", False) \
           else Path(__file__).parent.parent
    return base / ".winscan_key"
