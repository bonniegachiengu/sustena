' scripts/lore_keepalive_launcher.vbs
'
' Launches lore-keepalive.ps1 with genuinely no visible window, for the same
' reason keepalive_launcher.vbs does it this way: PowerShell's own
' -WindowStyle Hidden still flashes a console on some builds because the
' console is allocated before the hide takes effect. wscript.exe is a
' GUI-subsystem executable and never allocates one.
'
' Registered as the Scheduled Task action in register-lore-task.ps1:
'   wscript.exe "<repo>\scripts\lore_keepalive_launcher.vbs"

Set objShell = CreateObject("WScript.Shell")
scriptDir = CreateObject("Scripting.FileSystemObject").GetParentFolderName(WScript.ScriptFullName)
psScript = scriptDir & "\lore-keepalive.ps1"

cmd = "powershell.exe -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File """ & psScript & """"

' 0 = SW_HIDE, False = don't wait for it to exit
objShell.Run cmd, 0, False
