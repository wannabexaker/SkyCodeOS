@echo off
REM start-skycode.bat — One-click launcher (forwards to start-skycode.ps1)
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%~dp0start-skycode.ps1" %*