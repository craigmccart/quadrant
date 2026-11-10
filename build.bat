@echo off
setlocal
title Quadrant - Build Portable EXE
cd /d "%~dp0"

echo Building Quadrant (release). The first build takes a few minutes.
cargo build --release -p quadrant
if %errorlevel% neq 0 (
    echo.
    echo ERROR: the build failed. See the output above.
    pause & exit /b 1
)

if not exist dist mkdir dist
copy /y target\release\Quadrant.exe dist\Quadrant.exe >nul

echo.
echo  Output:  %~dp0dist\Quadrant.exe
echo  It is one portable file. It needs the WebView2 runtime, which ships
echo  with Windows 11 and current Windows 10.
echo.
pause
