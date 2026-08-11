<#
.SYNOPSIS
    Holds the WSL2 VM open so the cloudflared tunnel stays connected and the
    public site stays reachable.

.DESCRIPTION
    THE PROBLEM THIS SOLVES (measured, not assumed — 2026-08-11):

    WSL2 shuts its VM down when nothing is using it. `cloudflared` runs inside
    WSL, so when the VM stops, the tunnel dies with it and BOTH public
    hostnames return HTTP 530 — while the Windows-side backend stays perfectly
    healthy. That is why the old watchdog reported everything green during a
    total public outage: it was watching the one thing that had not broken.

    Measured directly: left alone for 150 seconds with no WSL activity,
    `wsl --list --running` reported "There are no running distributions" and
    both hostnames returned 530. Any `wsl` command brought it back within
    seconds. So the outage is idle shutdown, not a tunnel fault.

    THE FIX

    Keep one WSL session permanently open. An open session is enough to stop
    the VM idling out. This script runs `sleep infinity` inside WSL and simply
    waits on it. If WSL ever goes down anyway — a reboot, `wsl --shutdown`, an
    upgrade — the call returns, and the loop immediately opens a new session.

    WHY A HOLDER AND NOT PERIODIC POKING

    The 5-minute watchdog could poke WSL awake on each run, but the VM was
    measured dying inside 150 seconds. That leaves real gaps where the site is
    down. A held-open session has no gap at all. The watchdog's job is only to
    confirm THIS process is alive and restart it if not.

    WHAT THIS DOES NOT TOUCH

    Nothing about VOS, cloudflared, the tunnel config, or .wslconfig. It opens
    a session and holds it. Stop this process and the machine is exactly as it
    was — the only change is that WSL is allowed to idle out again.

.NOTES
    Launched hidden via wsl_keepawake_launcher.vbs; supervised by keepalive.ps1.
    Run it directly in a console to watch what it is doing.
#>

$Distro   = 'Ubuntu'
$LogDir   = Join-Path $PSScriptRoot 'logs'
$LogFile  = Join-Path $LogDir 'wsl-keepawake.log'

# Appears verbatim in the Windows-side wsl.exe command line, which is how
# keepalive.ps1 and the single-instance guard below recognise this process.
# `exec -a` renames the process inside WSL too, so it is identifiable on both
# sides of the boundary.
$Marker = 'sustena-wsl-holder'

New-Item -ItemType Directory -Force -Path $LogDir | Out-Null

function Write-Log {
    param([string]$Message)
    $ts = Get-Date -Format 'yyyy-MM-dd HH:mm:ss'
    Add-Content -Path $LogFile -Value "$ts  $Message"
}

# -- Single-instance guard --------------------------------------------------
# Two holders would work but would double the log noise and confuse the
# watchdog's "is it running" check. Look for an EXISTING wsl.exe carrying our
# marker, excluding this process's own not-yet-started child.
$existing = Get-CimInstance Win32_Process -Filter "Name = 'wsl.exe'" -ErrorAction SilentlyContinue |
    Where-Object { $_.CommandLine -like "*$Marker*" }

if ($existing) {
    Write-Log "HOLDER: already running (PID $($existing[0].ProcessId)) - this instance is exiting, not starting a second one."
    exit 0
}

Write-Log "HOLDER: starting - will hold WSL distro '$Distro' open indefinitely."

$consecutiveFailures = 0

while ($true) {
    $startedAt = Get-Date

    try {
        # Blocks for as long as the WSL VM stays up. Returns when WSL goes
        # down for any reason.
        & wsl.exe -d $Distro -- bash -lc "exec -a $Marker sleep infinity" 2>&1 | Out-Null
    }
    catch {
        Write-Log "HOLDER: wsl.exe threw - $($_.Exception.Message)"
    }

    $heldFor = [int]((Get-Date) - $startedAt).TotalSeconds

    # A session that held for a decent stretch then ended is normal (reboot,
    # deliberate `wsl --shutdown`). One that dies instantly, repeatedly, means
    # something is actually wrong — back off so we do not spin a tight loop,
    # and say so in the log rather than restarting silently forever.
    if ($heldFor -lt 10) {
        $consecutiveFailures++
        $backoff = [Math]::Min(60, 5 * $consecutiveFailures)
        if ($consecutiveFailures -eq 1 -or $consecutiveFailures % 10 -eq 0) {
            Write-Log "HOLDER: session ended after ${heldFor}s (failure #$consecutiveFailures). Is distro '$Distro' healthy? Retrying in ${backoff}s."
        }
        Start-Sleep -Seconds $backoff
    }
    else {
        if ($consecutiveFailures -gt 0) {
            Write-Log "HOLDER: recovered after $consecutiveFailures short-lived attempt(s)."
        }
        $consecutiveFailures = 0
        Write-Log "HOLDER: WSL session ended after ${heldFor}s (reboot or wsl --shutdown). Reopening immediately."
        Start-Sleep -Seconds 2
    }
}
