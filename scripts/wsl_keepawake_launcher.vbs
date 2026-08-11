' scripts/wsl_keepawake_launcher.vbs
'
' Launches wsl-keepawake.ps1 with genuinely no visible window, and detached
' so it outlives whatever started it.
'
' Same reasoning as keepalive_launcher.vbs: PowerShell's own -WindowStyle
' Hidden still flashes a console on some builds because the console is
' allocated before the hide takes effect. wscript.exe is GUI-subsystem and
' never allocates one, and Run()'s windowStyle=0 starts the child hidden from
' the very first instant.
'
' The detached part matters more here than it does for keepalive: this holder
' must keep running for days. keepalive.ps1 starts it via this launcher and
' then exits; passing False as Run()'s third argument means "don't wait", so
' the holder is not tied to the watchdog's short lifetime.
'
' Used by:
'   keepalive.ps1  (restarts the holder if it finds it missing)
'   register-task.ps1 (starts it once at registration time)

Set objShell = CreateObject("WScript.Shell")
scriptDir = CreateObject("Scripting.FileSystemObject").GetParentFolderName(WScript.ScriptFullName)
psScript = scriptDir & "\wsl-keepawake.ps1"

cmd = "powershell.exe -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File """ & psScript & """"

' 0 = SW_HIDE, False = don't wait for it to exit
objShell.Run cmd, 0, False
