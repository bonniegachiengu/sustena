' scripts/keepalive_launcher.vbs
'
' Launches keepalive.ps1 with genuinely no visible window. Windows PowerShell's
' own -WindowStyle Hidden still briefly flashes a console on some builds because
' the console is allocated before the hide takes effect. wscript.exe never
' allocates a console at all (it's a GUI-subsystem executable), and Run()'s
' windowStyle=0 tells CreateProcess to start the child hidden from the start --
' there's nothing to flash.
'
' Registered as the Scheduled Task action in register-task.ps1:
'   wscript.exe "<repo>\scripts\keepalive_launcher.vbs"

Set objShell = CreateObject("WScript.Shell")
scriptDir = CreateObject("Scripting.FileSystemObject").GetParentFolderName(WScript.ScriptFullName)
psScript = scriptDir & "\keepalive.ps1"

cmd = "powershell.exe -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File """ & psScript & """"

' 0 = SW_HIDE, False = don't wait for it to exit
objShell.Run cmd, 0, False
