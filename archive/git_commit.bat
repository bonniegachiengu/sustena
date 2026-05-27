@echo off
REM Sustena XII — Git commit helper
REM Run this from any location after each Claude session

cd /d "C:\Users\DELL\OneDrive\Documents\Projects\Sustena XII\Sustena XII"

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

REM Commit with timestamp
git commit -m "Sustena XII: strategy update %date% %time%"

REM Push — enter GitHub Personal Access Token when prompted for password
REM Username: bonniegachiengu
REM Password: [your GitHub PAT from github.com > Settings > Developer Settings > PATs]
git push origin main

echo.
echo Done. Check https://github.com/bonniegachiengu/sustena
pause
