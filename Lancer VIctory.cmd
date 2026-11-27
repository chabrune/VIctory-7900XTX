@echo off
rem Opens the settings window and starts the game (scripts\lanceur.ps1).
start "" powershell.exe -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File "%~dp0scripts\lanceur.ps1"
