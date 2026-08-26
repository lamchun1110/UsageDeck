@echo off
setlocal
"%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe" -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "%~dp0sign-windows.ps1" -FilePath "%~1"
set "usagedeck_exit_code=%ERRORLEVEL%"
endlocal & exit /b %usagedeck_exit_code%
