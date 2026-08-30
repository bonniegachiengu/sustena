' scripts/lore_watchdog_launcher.vbs
'
' Launches lore-watchdog-loop.ps1 detached and with no visible window, for the
' same reason the other launchers in this directory do it this way: wscript.exe
' is a GUI-subsystem executable and never allocates a console, so there is
' nothing to flash.
'
' This is the no-elevation fallback for the SustenaLore scheduled task. See
' docs/LORE_STATUS.md.

Set objShell = CreateObject("WScript.Shell")
scriptDir = CreateObject("Scripting.FileSystemObject").GetParentFolderName(WScript.ScriptFullName)
psScript = scriptDir & "\lore-watchdog-loop.ps1"

cmd = "powershell.exe -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File """ & psScript & """"

' 0 = SW_HIDE, False = don't wait for it to exit
objShell.Run cmd, 0, False
