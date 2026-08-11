<#
.SYNOPSIS
    Sustena XII watchdog. Checks the backend and frontend, restarts whichever
    is down, and refuses to restart-loop into a corrupted SQLite database.

.DESCRIPTION
    Meant to be triggered on a repeating schedule by Task Scheduler (see
    register-task.ps1). Each run is a single check-and-fix pass, then exits -
    the repetition comes from the scheduled task, not a loop in here.

    What "down" means is deliberately strict:
      - Backend: a real GET to /health must return HTTP 200 with
        body.status == "ok". A process that owns port 9000 but is hung, or
        that answers with status:"degraded" because its DB is unhappy, is
        NOT counted as healthy.
      - Frontend: a real GET to http://localhost:3000 must return HTTP 200.

    Before restarting something that's down, this script waits
    $ConfirmDelaySeconds and checks again. That's the guard against fighting
    you when you deliberately stop something yourself (e.g. Ctrl+C on
    `npm run dev` to restart it with a change) - a real outage is still down
    on the second check; a deliberate restart usually isn't.

    Backend corruption handling (the one behaviour that must never
    restart-loop): if the backend is down AND a direct SQLite
    `PRAGMA integrity_check` against sustena.db fails, this script writes a
    marker file and stops touching the backend entirely until the marker is
    gone. It re-checks the DB on every run so it recovers automatically the
    moment you fix it - it just won't hammer a broken file in the meantime.

    HEALTH-CHECK TARGET - a real bug this script had, found and fixed
    2026-08-01 during the node-zero reliability pass: it used to hit
    'http://localhost:9000/health'. On this machine, Bonnie also regularly
    runs a SEPARATE `uvicorn --reload --port 9000` dev process bound to
    127.0.0.1 for local development. Windows lets both a 127.0.0.1-specific
    bind and a 0.0.0.0 wildcard bind coexist on the same port - and a
    127.0.0.1 destination always resolves to the MORE SPECIFIC bind. So
    'localhost' silently checked the dev --reload process (which is always
    fresh, since it reloads on every file save) while the actual
    tunnel-facing 0.0.0.0 process - the one the public hostname reaches -
    could go stale indefinitely with this watchdog reporting "healthy" the
    whole time. This exact blind spot is why the public site went stale
    repeatedly (see CLAUDE.md's Slice 9/10/"node zero" notes) even with
    this watchdog running. Fixed by resolving the machine's real LAN IPv4
    fresh on every run (Get-BackendCheckUrl) instead of 'localhost' - a
    connection to that address can ONLY be answered by the wildcard
    (0.0.0.0) listener, since a 127.0.0.1-only bind never accepts traffic
    addressed to a different local IP. This also now compares the running
    process's own reported git_commit (a field /health gained in the same
    pass) against the repo's current HEAD and logs a WARNING on mismatch -
    visibility only, it does NOT auto-redeploy, since the working tree may
    legitimately hold uncommitted WIP that isn't meant to go live yet. Use
    scripts\deploy.ps1 to actually roll a new commit out.
#>

# -- Paths - everything is relative to this script's location ----------------
$RepoRoot   = Split-Path -Parent $PSScriptRoot
$ApiDir     = Join-Path $RepoRoot 'apps\api'
$WebDir     = Join-Path $RepoRoot 'apps\web'
$DbPath     = Join-Path $ApiDir  'sustena.db'
$LogDir     = Join-Path $PSScriptRoot 'logs'
$LogFile    = Join-Path $LogDir 'keepalive.log'
$CorruptMarker = Join-Path $LogDir 'backend.db-corrupt'

# Which Python starts the backend. Bonnie's backend is currently running out
# of the GLOBAL Python install, not apps/api/venv (verified - the venv has a
# different fastapi/uvicorn version that's never actually been run). Default
# to what's proven to work; switch this once the venv is verified if you want
# the isolated environment instead.
$PythonExe = 'python'   # e.g. change to "$ApiDir\venv\Scripts\python.exe" once verified

$BackendAuthHeader = @{ Authorization = 'Bearer dev-admin-token' }   # unused by /health, kept for future authenticated checks
$FrontendUrl       = 'http://localhost:3000'
$ConfirmDelaySeconds = 15   # grace period before treating "down" as real, not a deliberate restart

# Resolved fresh on every run, not hardcoded - see the HEALTH-CHECK TARGET
# note above. Falls back to 'localhost' only if no LAN adapter is found
# (e.g. an offline machine), which is strictly worse than not checking at
# all but keeps the watchdog from crashing outright.
function Get-BackendCheckUrl {
    $ip = (Get-NetIPAddress -AddressFamily IPv4 -ErrorAction SilentlyContinue |
        Where-Object {
            $_.IPAddress -ne '127.0.0.1' -and
            $_.IPAddress -notlike '169.254*' -and
            $_.InterfaceAlias -notmatch 'Loopback|vEthernet.*WSL'
        } | Select-Object -First 1 -ExpandProperty IPAddress)
    if (-not $ip) { $ip = 'localhost' }
    return "http://${ip}:9000/health"
}
$HttpTimeoutSeconds  = 5
$StartupVerifySeconds = 10  # how long to wait after launching before checking it actually came up

New-Item -ItemType Directory -Force -Path $LogDir | Out-Null

function Write-Log {
    param([string]$Message)
    $ts = Get-Date -Format 'yyyy-MM-dd HH:mm:ss'
    Add-Content -Path $LogFile -Value "$ts  $Message"
}

# -- Single-instance guard ----------------------------------------------------
# If a previous run is still mid-launch (e.g. Task Scheduler fired again before
# the last pass finished), exit quietly rather than double-start anything.
$mutex = New-Object System.Threading.Mutex($false, 'Global\SustenaKeepaliveMutex')
if (-not $mutex.WaitOne(0)) {
    exit 0
}

try {

    # -- Backend health --------------------------------------------------------
    # Returns 'ok', 'db_error' (process is up but reports its own DB unhealthy),
    # or 'unreachable' (no response at all - down, hung, or never started).
    function Get-BackendStatus {
        try {
            $r = Invoke-WebRequest -Uri (Get-BackendCheckUrl) -TimeoutSec $HttpTimeoutSeconds -UseBasicParsing
            if ($r.StatusCode -ne 200) { return 'unreachable' }
            $body = $r.Content | ConvertFrom-Json
            Test-StalenessAndLog -RunningCommit $body.git_commit
            if ($body.status -eq 'ok') { return 'ok' } else { return 'db_error' }
        } catch {
            return 'unreachable'
        }
    }

    # Visibility only - never restarts anything on its own. A mismatch means
    # the running process predates the repo's current HEAD (normal right
    # after `git commit`, until scripts\deploy.ps1 is run; a problem if it
    # persists). Logs at most once per distinct stale commit, not every
    # 5-minute tick, so a known-stale state doesn't spam the log forever.
    $script:LastLoggedStaleCommit = $null
    function Test-StalenessAndLog {
        param([string]$RunningCommit)
        if ([string]::IsNullOrWhiteSpace($RunningCommit) -or $RunningCommit -eq 'unknown') { return }
        try {
            $head = (& git -C $RepoRoot rev-parse --short=12 HEAD 2>$null).Trim()
        } catch {
            return
        }
        if (-not $head) { return }
        if ($RunningCommit -ne $head -and $RunningCommit -ne $script:LastLoggedStaleCommit) {
            Write-Log "BACKEND: STALE - running commit $RunningCommit, repo HEAD is $head. Run scripts\deploy.ps1 to roll the current commit out."
            $script:LastLoggedStaleCommit = $RunningCommit
        } elseif ($RunningCommit -eq $head) {
            $script:LastLoggedStaleCommit = $null
        }
    }

    # Direct, dependency-free integrity check against the DB file itself -
    # this is the same check (PRAGMA integrity_check) the OneDrive-corruption
    # incident in CLAUDE.md was diagnosed with. Doesn't need the venv or any
    # project package, just Python's stdlib sqlite3, so it works even when the
    # backend can't start at all.
    # Returns 'ok', 'missing' (no db file yet - not corruption, fine to start),
    # or the raw integrity_check failure text.
    function Test-DbIntegrity {
        if (-not (Test-Path $DbPath)) { return 'missing' }
        $checkScript = @"
import sqlite3, sys
try:
    conn = sqlite3.connect(r'$DbPath', timeout=5)
    result = conn.execute('PRAGMA integrity_check').fetchone()[0]
    conn.close()
    print(result)
except sqlite3.Error as e:
    print('ERROR:' + str(e))
"@
        $output = & $PythonExe -c $checkScript 2>&1 | Out-String
        $output = $output.Trim()
        if ($output -eq 'ok') { return 'ok' }
        return $output   # corruption description, or a locked/busy transient error
    }

    # Launches the backend, then WAITS and RE-CHECKS before claiming anything.
    # Issuing Start-Process only proves a process object was created -- it
    # proves nothing about whether uvicorn actually bound the port. Returns
    # $true only if /health confirms it afterward; logs whichever actually
    # happened, never an assumed outcome.
    function Start-Backend {
        try {
            # --host 0.0.0.0, not the uvicorn default (127.0.0.1): the WSL-side
            # cloudflared tunnel reaches this over the WSL2 NAT gateway, which
            # only routes to interfaces the process is actually bound to.
            # Loopback-only would make the backend unreachable from the tunnel
            # regardless of any cloudflared config. See CLAUDE.md's public-
            # hostname prep notes for the full reachability verification.
            Start-Process -FilePath $PythonExe `
                -ArgumentList @('-m', 'uvicorn', 'sustena.api.main:app', '--host', '0.0.0.0', '--port', '9000') `
                -WorkingDirectory $ApiDir `
                -WindowStyle Hidden
        } catch {
            Write-Log "BACKEND: failed to launch process (cwd=$ApiDir, python=$PythonExe) - $($_.Exception.Message)"
            return $false
        }
        Start-Sleep -Seconds $StartupVerifySeconds
        if ((Get-BackendStatus) -eq 'ok') {
            Write-Log "BACKEND: launched and CONFIRMED healthy after ${StartupVerifySeconds}s (cwd=$ApiDir, python=$PythonExe)"
            return $true
        } else {
            Write-Log "BACKEND: launch command issued but NOT responding healthy after ${StartupVerifySeconds}s (cwd=$ApiDir, python=$PythonExe) - will re-check next cycle, not assuming it's up"
            return $false
        }
    }

    $backendStatus = Get-BackendStatus

    if ($backendStatus -eq 'ok') {
        if (Test-Path $CorruptMarker) {
            Remove-Item $CorruptMarker -Force
            Write-Log 'BACKEND: healthy again - cleared the db-corrupt marker'
        }
        # quiet - nothing to do
    }
    elseif ($backendStatus -eq 'db_error') {
        # Process is up and answering, but it's telling us its own DB check
        # failed. Restarting the process won't fix a bad file, so don't.
        if (-not (Test-Path $CorruptMarker)) {
            New-Item -ItemType File -Path $CorruptMarker -Force | Out-Null
            Write-Log "BACKEND: /health reports db_status=error while the process IS running. Not restarting - this needs manual attention (check $DbPath). Marker written: $CorruptMarker"
        }
    }
    else {
        # 'unreachable'. Could be: not started yet, a deliberate restart in
        # progress, a hang, or a crash-on-startup from a corrupt DB.
        if (Test-Path $CorruptMarker) {
            # Already known-bad as of the last run - re-check rather than assume.
            $integrity = Test-DbIntegrity
            if ($integrity -eq 'ok' -or $integrity -eq 'missing') {
                Remove-Item $CorruptMarker -Force
                Write-Log "BACKEND: db integrity check now passes ($integrity) - clearing marker and attempting restart"
                Start-Backend
            } else {
                Write-Log "BACKEND: still down, db still failing integrity check - not restarting. ($integrity)"
            }
        } else {
            # Not previously known-bad. Give a deliberate restart room to
            # finish before acting.
            Start-Sleep -Seconds $ConfirmDelaySeconds
            if ((Get-BackendStatus) -eq 'ok') {
                # It came back on its own - someone was restarting it. Do nothing.
            } else {
                $integrity = Test-DbIntegrity
                if ($integrity -eq 'ok' -or $integrity -eq 'missing') {
                    Write-Log "BACKEND: down, db integrity ok ($integrity) - starting"
                    Start-Backend
                } else {
                    New-Item -ItemType File -Path $CorruptMarker -Force | Out-Null
                    Write-Log "BACKEND: down AND db integrity check FAILED - stopping here rather than restart-looping. Details: $integrity"
                    Write-Log "BACKEND: fix per CLAUDE.md's prior incident - verify $DbPath, restore a backup or let init_db() recreate it deliberately, then delete $CorruptMarker to resume."
                }
            }
        }
    }

    # -- Public reachability + the WSL holder -----------------------------------
    # THE BLIND SPOT THIS CLOSES (measured 2026-08-11):
    #
    # Every check above this point tests the WINDOWS-side backend. But the
    # public site is reached through a cloudflared tunnel running INSIDE WSL,
    # and WSL2 shuts its VM down when idle. When that happens the tunnel dies,
    # BOTH hostnames return HTTP 530, and every check above still reports
    # green - because the backend genuinely is fine. This watchdog was
    # watching the one thing that had not broken.
    #
    # Measured: with no WSL activity, `wsl --list --running` reported "There
    # are no running distributions" within 150 seconds and both hostnames went
    # to 530. The site was only ever up while something happened to be using
    # WSL.
    #
    # The fix is wsl-keepawake.ps1, which holds one WSL session permanently
    # open. This block's job is to confirm that holder is alive and restart it
    # if not - and, independently, to report what a real user actually gets.
    #
    # DELIBERATELY NOT DONE HERE: nothing touches cloudflared, the tunnel
    # config, or VOS. If the holder is up, WSL is up, and the hostnames still
    # fail, that is logged for a human - not "fixed" by this script.

    $WslHolderMarker   = 'sustena-wsl-holder'
    $WslHolderLauncher = Join-Path $PSScriptRoot 'wsl_keepawake_launcher.vbs'
    $PublicHosts       = @(
        'https://sustena.vyybandasky.online/health',
        'https://app.vyybandasky.online/'
    )

    function Test-WslHolderRunning {
        # Matches the wsl.exe process the holder keeps open. Checking wsl.exe
        # (not powershell.exe) on purpose: a PowerShell match would also catch
        # any diagnostic command that merely MENTIONS the script name, which
        # produced a false "two holders running" reading during development.
        $p = Get-CimInstance Win32_Process -Filter "Name = 'wsl.exe'" -ErrorAction SilentlyContinue |
             Where-Object { $_.CommandLine -like "*$WslHolderMarker*" }
        return [bool]$p
    }

    function Start-WslHolder {
        if (-not (Test-Path $WslHolderLauncher)) {
            Write-Log "WSL-HOLDER: launcher missing at $WslHolderLauncher - cannot start. Public site will drop when WSL idles out."
            return $false
        }
        try {
            # Via wscript so it starts hidden AND detached - it must outlive
            # this watchdog run, which exits in seconds.
            Start-Process -FilePath 'wscript.exe' -ArgumentList @("`"$WslHolderLauncher`"") -WindowStyle Hidden
        } catch {
            Write-Log "WSL-HOLDER: failed to launch - $($_.Exception.Message)"
            return $false
        }
        Start-Sleep -Seconds $StartupVerifySeconds
        if (Test-WslHolderRunning) {
            Write-Log "WSL-HOLDER: started and CONFIRMED running after ${StartupVerifySeconds}s."
            return $true
        }
        Write-Log "WSL-HOLDER: launch issued but NOT confirmed running after ${StartupVerifySeconds}s - will re-check next cycle, not assuming it's up."
        return $false
    }

    function Test-PublicHost {
        param([string]$Url)
        try {
            $r = Invoke-WebRequest -Uri $Url -TimeoutSec $HttpTimeoutSeconds -UseBasicParsing
            return @{ Ok = ($r.StatusCode -eq 200); Code = $r.StatusCode }
        } catch {
            $code = 0
            if ($_.Exception.Response) { $code = [int]$_.Exception.Response.StatusCode }
            return @{ Ok = $false; Code = $code }
        }
    }

    if (-not (Test-WslHolderRunning)) {
        Write-Log 'WSL-HOLDER: not running - starting (WSL would otherwise idle out and take the public site down)'
        Start-WslHolder | Out-Null
    }

    # Report what a real user gets. Same confirm-before-acting discipline used
    # everywhere else here: one failure can be a blip, so re-check after the
    # grace period before writing an outage line.
    $publicFailures = @()
    foreach ($u in $PublicHosts) {
        if (-not (Test-PublicHost -Url $u).Ok) { $publicFailures += $u }
    }

    if ($publicFailures.Count -gt 0) {
        Start-Sleep -Seconds $ConfirmDelaySeconds
        foreach ($u in $publicFailures) {
            $res = Test-PublicHost -Url $u
            if ($res.Ok) { continue }   # recovered during the grace period

            $wslUp    = [bool]((wsl.exe --list --running 2>$null) -join ' ' -match 'Ubuntu')
            $holderUp = Test-WslHolderRunning

            if (-not $holderUp) {
                Write-Log "PUBLIC: $u returned HTTP $($res.Code) and the WSL holder is NOT running - starting it."
                Start-WslHolder | Out-Null
            }
            elseif (-not $wslUp) {
                Write-Log "PUBLIC: $u returned HTTP $($res.Code); holder process exists but WSL reports no running distro - it should reopen within seconds, re-checking next cycle."
            }
            else {
                # Holder up, WSL up, still unreachable. That points at
                # cloudflared or the tunnel - shared VOS infrastructure this
                # script deliberately does not touch. Say so plainly instead
                # of restarting something it was told to leave alone.
                Write-Log "PUBLIC: $u returned HTTP $($res.Code) while WSL and the holder are both UP. Backend is reachable locally, so this points at cloudflared/the tunnel (shared with VOS) - NOT restarting it automatically. Needs a human."
            }
        }
    }

    # -- Frontend --------------------------------------------------------------
    # Runs `npm run dev` (Vite, HMR) - this machine is Bonnie's active dev
    # environment, not a production mirror, so the dev server is the right
    # default. Same confirm-before-restart pattern as the backend, since this
    # is exactly the process he'll deliberately Ctrl+C and relaunch by hand.
    function Test-FrontendHealthy {
        try {
            $r = Invoke-WebRequest -Uri $FrontendUrl -TimeoutSec $HttpTimeoutSeconds -UseBasicParsing
            return $r.StatusCode -eq 200
        } catch {
            return $false
        }
    }

    # Same discipline as Start-Backend: verify before claiming, log what
    # actually happened.
    function Start-Frontend {
        try {
            Start-Process -FilePath 'npm.cmd' -ArgumentList @('run', 'dev') `
                -WorkingDirectory $WebDir `
                -WindowStyle Hidden
        } catch {
            Write-Log "FRONTEND: failed to launch process (cwd=$WebDir) - $($_.Exception.Message)"
            return $false
        }
        Start-Sleep -Seconds $StartupVerifySeconds
        if (Test-FrontendHealthy) {
            Write-Log "FRONTEND: launched and CONFIRMED responding after ${StartupVerifySeconds}s (cwd=$WebDir)"
            return $true
        } else {
            Write-Log "FRONTEND: launch command issued but NOT responding after ${StartupVerifySeconds}s (cwd=$WebDir) - will re-check next cycle, not assuming it's up"
            return $false
        }
    }

    if (-not (Test-FrontendHealthy)) {
        Start-Sleep -Seconds $ConfirmDelaySeconds
        if (-not (Test-FrontendHealthy)) {
            Write-Log 'FRONTEND: down - starting'
            Start-Frontend
        }
        # else: came back on its own during the grace period - leave it alone.
    }

}
finally {
    $mutex.ReleaseMutex() | Out-Null
}
