@echo off
setlocal
title WinScan - Build Portable EXE

echo ============================================================
echo  WinScan - Build Portable EXE
echo ============================================================
echo.

:: Install / upgrade dependencies
echo [1/3] Installing dependencies...
pip install --upgrade customtkinter google-generativeai pyinstaller
if %errorlevel% neq 0 (
    echo ERROR: pip install failed. Make sure Python is in PATH.
    pause & exit /b 1
)

echo.
echo [2/3] Building portable EXE with PyInstaller...
echo       (This takes ~60-90 seconds)
echo.

pyinstaller ^
    --onefile ^
    --windowed ^
    --name "WinScan" ^
    --icon "%~dp0winscan.ico" ^
    --add-data "%~dp0winscan.ico;." ^
    --collect-all customtkinter ^
    --collect-all google.generativeai ^
    --hidden-import=google.generativeai ^
    --hidden-import=PIL ^
    "%~dp0winscan.py"

if %errorlevel% neq 0 (
    echo.
    echo ERROR: PyInstaller build failed. See output above.
    pause & exit /b 1
)

echo.
echo [3/3] Done!
echo.
echo  Output:  %~dp0dist\WinScan.exe
echo.
echo  Copy WinScan.exe anywhere — it is fully portable.
echo  Your saved API key will be stored as .winscan_key
echo  in the same folder as the EXE.
echo.
pause
