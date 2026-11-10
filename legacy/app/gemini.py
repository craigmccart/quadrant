"""WinScan — Google Gemini client for the AI cleanup summary."""

from app.config import GEMINI_MODEL
from app.helpers import fmt_size
from app.scanner import ScanResult


# ── Gemini ────────────────────────────────────────────────────────────────────

def ask_gemini(api_key: str, scan_results: list[ScanResult]) -> str:
    try:
        import google.generativeai as genai
    except ImportError:
        return "google-generativeai not installed.\nRun:  pip install google-generativeai"

    genai.configure(api_key=api_key)
    model = genai.GenerativeModel(GEMINI_MODEL)

    total = sum(r.total_bytes for r in scan_results)
    lines = "\n".join(f"- {r.label}: {fmt_size(r.total_bytes)} ({r.note})"
                      for r in scan_results)

    prompt = f"""You are a Windows system cleanup assistant. A user scanned their Windows 11 PC.
Give a concise, friendly analysis (max 280 words) covering:
1. Total potential space savings
2. Which categories are 100% safe to delete immediately
3. Which need a little care (e.g. large downloads worth reviewing)
4. Any gotchas or warnings
5. Recommended clean-up order

Be direct and practical — the user is technically capable.

Scan results (total: {fmt_size(total)}):
{lines}"""

    try:
        response = model.generate_content(prompt)
        return response.text
    except Exception as e:
        return f"Gemini error: {e}"
