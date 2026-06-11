@echo off
:: WinScan - run directly with Python (no build needed)
:: For best results, right-click -> Run as Administrator

pip show customtkinter >nul 2>&1
if %errorlevel% neq 0 (
    echo Installing dependencies...
    pip install customtkinter google-generativeai
)

python "%~dp0winscan.py"
