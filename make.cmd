@echo off
rem Lets Windows users type ".\make <target>" without changing PowerShell's script policy.
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0scripts\make.ps1" %*
exit /b %ERRORLEVEL%
