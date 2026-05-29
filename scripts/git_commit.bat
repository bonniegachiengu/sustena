@echo off
REM Sustena XII — Git commit helper
REM Double-click to stage, commit, and push all changes

cd /d "C:\Users\DELL\OneDrive\Documents\Projects\Sustena XII\Sustena XII"

REM Clear any stale git locks from crashed sessions
if exist ".git\index.lock"           del /f ".git\index.lock"
if exist ".git\HEAD.lock"            del /f ".git\HEAD.lock"
if exist ".git\refs\heads\main.lock" del /f ".git\refs\heads\main.lock"
if exist ".git\refs\heads\dev.lock"  del /f ".git\refs\heads\dev.lock"
if exist ".git\MERGE_HEAD.lock"      del /f ".git\MERGE_HEAD.lock"

REM Initialize git if not already done
if not exist ".git" (
    git init
    git branch -m main
    git remote add origin https://github.com/bonniegachiengu/sustena.git
    echo Git initialized and remote set.
)

REM Configure identity
git config user.email "bonniegachiengu@gmail.com"
git config user.name "bonniegachiengu"

REM Stage all changes
git add -A

REM Show what will be committed
echo.
echo === CHANGES STAGED ===
git status --short
echo.

REM Optional: prompt for custom message, default to timestamp
set /p MSG="Commit message (Enter = use timestamp): "
if "%MSG%"=="" set MSG=checkpoint: sustena update %date% %time%

git commit -m "%MSG%"

REM Push
git push origin main

echo.
echo Done. Check https://github.com/bonniegachiengu/sustena
pause
