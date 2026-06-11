"""WinScan — configuration constants."""


# ── Configuration ─────────────────────────────────────────────────────────────

GEMINI_MODEL = "gemini-3.1-flash-lite-preview"
OLD_FILE_DAYS = 90
LARGE_FILE_MIN_BYTES = 100 * 1024 * 1024  # 100 MB
DRILL_DOWN_CAP = 200  # max files shown in drill-down per category

SAFE_EXTENSIONS = {
    ".jpg", ".jpeg", ".png", ".gif", ".bmp", ".tiff", ".tif", ".heic",
    ".heif", ".raw", ".cr2", ".nef", ".arw", ".dng", ".webp", ".svg",
    ".mp4", ".mov", ".avi", ".mkv", ".wmv", ".flv", ".m4v", ".mpg", ".mpeg",
    ".mp3", ".wav", ".flac", ".aac", ".ogg", ".wma", ".m4a",
    ".pdf", ".doc", ".docx", ".xls", ".xlsx", ".ppt", ".pptx",
    ".odt", ".ods", ".odp", ".rtf", ".txt", ".md",
    ".py", ".js", ".ts", ".cs", ".cpp", ".c", ".h", ".java", ".go",
    ".rs", ".rb", ".php", ".swift", ".kt",
}


APP_VERSION = "2.1"
