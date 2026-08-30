# Sustena Lore -- keep the essays answering.
#
# The blog is served by its own small process (sustena.lore_site.serve) rather
# than by the application backend. That is deliberate: publishing an essay must
# never require -- or force -- a deploy of an application somebody else is
# mid-change on. See sustena/lore_site/serve.py for the full reasoning.
#
# This script is the watchdog for that process, and it follows the lesson the
# node-zero pass learned the hard way: check the port you actually serve on,
# not a convenient one. It probes the real listener and starts it only when the
# probe genuinely fails, so running this on a schedule is safe and idempotent.
#
# Registered by scripts/register-lore-task.ps1 as the scheduled task
# "SustenaLore", at logon and every 5 minutes.

$ErrorActionPreference = 'Stop'

$Port    = 9100
$AppDir  = 'C:\Users\DELL\dev\sustena-lore\apps\api'
$Python  = 'C:\Users\DELL\AppData\Local\Programs\Python\Python311\python.exe'
$LogFile = Join-Path $AppDir 'lore-keepalive.log'

function Write-Log([string]$Message) {
    $line = ('{0}  {1}' -f (Get-Date -Format 'yyyy-MM-dd HH:mm:ss'), $Message)
    Add-Content -Path $LogFile -Value $line -Encoding utf8
}

function Test-LoreUp {
    try {
        $r = Invoke-WebRequest -Uri ("http://127.0.0.1:{0}/" -f $Port) `
                               -UseBasicParsing -TimeoutSec 8
        # ★ 200 is not enough on its own: the application shell would also
        #   answer 200. The blog is only up if the blog is what answered.
        return ($r.StatusCode -eq 200 -and $r.Content -match 'Sustena Lore')
    } catch {
        return $false
    }
}

if (Test-LoreUp) { exit 0 }

Write-Log "lore not answering on $Port - starting it"

if (-not (Test-Path $Python)) { Write-Log "FATAL: python not found at $Python"; exit 1 }
if (-not (Test-Path $AppDir)) { Write-Log "FATAL: app dir not found at $AppDir"; exit 1 }

Start-Process -FilePath $Python `
              -ArgumentList '-m', 'sustena.lore_site.serve', '--host', '127.0.0.1', '--port', "$Port" `
              -WorkingDirectory $AppDir `
              -WindowStyle Hidden

Start-Sleep -Seconds 4
if (Test-LoreUp) { Write-Log "lore is up on $Port" } else { Write-Log "WARN: still not answering after start" }
