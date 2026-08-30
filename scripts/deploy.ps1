<#
.SYNOPSIS
    Sustena XII deploy: rebuild the frontend and restart the public-facing
    backend together, then verify the result is actually live and current.

.DESCRIPTION
    This is the ONE command that should be run after landing a commit meant
    to reach sustena.vyybandasky.online. It exists because every prior
    "stale site" incident in this project's history (see CLAUDE.md's Slice
    9/10/13 notes) was some variant of "the frontend got rebuilt but the
    backend wasn't restarted" or "the backend got restarted but a stray
    second process on the same port meant the wrong one answered" - i.e.
    the two halves of a deploy were done separately, by hand, and drifted.

    What it does, in order:
      1. `npm run build` in apps/web - produces a fresh apps/web/dist from
         whatever is currently on disk (NOT necessarily HEAD - see the
         -RequireClean note below).
      2. Stops whichever process currently owns the PUBLIC (0.0.0.0:9000)
         listener - never touches a separate 127.0.0.1-only dev/--reload
         process if one happens to be running alongside it.
      3. Starts a fresh backend process the same way keepalive.ps1 does
         (same command, same cwd), so the watchdog's own expectations about
         what "the backend" looks like stay consistent with what a manual
         deploy produces.
      4. Polls /health (via the same LAN-IP resolution keepalive.ps1 uses -
         see that script's HEALTH-CHECK TARGET note for why 'localhost'
         is NOT safe here) until it reports healthy, then compares its
         git_commit against the repo's current HEAD and prints a clear
         PASS/mismatch line. A mismatch is only expected/fine when you
         intentionally deployed a dirty working tree (-AllowDirty); by
         default this script refuses to do that, matching the standing
         "don't silently ship uncommitted WIP to the public site" rule.

.PARAMETER AllowDirty
    Without this switch, the script refuses to build/deploy if `git status`
    shows uncommitted changes to tracked files that aren't explicitly
    exempted - a deploy is supposed to ship a commit, not whatever happens
    to be sitting in the working tree. Pass -AllowDirty to override for a
    deliberate one-off (e.g. testing an uncommitted fix live before
    committing it) - the script still tells you exactly what's dirty.

.NOTES
    Run from anywhere:
        powershell -ExecutionPolicy Bypass -File .\scripts\deploy.ps1
#>

param(
    [switch]$AllowDirty
)

$RepoRoot = Split-Path -Parent $PSScriptRoot
$ApiDir   = Join-Path $RepoRoot 'apps\api'
$WebDir   = Join-Path $RepoRoot 'apps\web'
$PythonExe = 'python'

function Write-Step {
    param([string]$Message)
    Write-Host ""
    Write-Host "== $Message ==" -ForegroundColor Cyan
}

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

# -- 0. Dirty-tree guard -------------------------------------------------------
Write-Step "Checking working tree"
Push-Location $RepoRoot
try {
    $dirty = git status --porcelain=v1 --untracked-files=no
} finally {
    Pop-Location
}

if ($dirty -and -not $AllowDirty) {
    Write-Host "REFUSING TO DEPLOY: working tree has uncommitted changes to tracked files:" -ForegroundColor Red
    Write-Host $dirty
    Write-Host ""
    Write-Host "Commit first, or re-run with -AllowDirty to deploy the working tree as-is." -ForegroundColor Yellow
    exit 1
}
if ($dirty) {
    Write-Host "WARNING: deploying a DIRTY working tree (-AllowDirty). Uncommitted changes:" -ForegroundColor Yellow
    Write-Host $dirty
}

$headBefore = (& git -C $RepoRoot rev-parse --short=12 HEAD).Trim()
Write-Host "HEAD: $headBefore"

# -- 1. Frontend build ---------------------------------------------------------
Write-Step "Building frontend (npm run build)"
Push-Location $WebDir
try {
    npm run build
    if ($LASTEXITCODE -ne 0) {
        Write-Host "FRONTEND BUILD FAILED (exit $LASTEXITCODE) - stopping, backend NOT touched." -ForegroundColor Red
        exit 1
    }
} finally {
    Pop-Location
}
Write-Host "Frontend build OK." -ForegroundColor Green

# -- 1b. Sustena Lore ----------------------------------------------------------
# The blog is served by the same process, dispatched by Host, so its static
# build belongs to the same deploy. It is generated (dist/ is gitignored), and
# the build is strict: an essay with a construct the renderer does not know
# stops the deploy here rather than losing a paragraph in public.
Write-Step "Building Sustena Lore (static essays)"
Push-Location $ApiDir
try {
    & $PythonExe -m sustena.lore_site.build
    if ($LASTEXITCODE -ne 0) {
        Write-Host "LORE BUILD FAILED (exit $LASTEXITCODE) - stopping, backend NOT touched." -ForegroundColor Red
        exit 1
    }
} finally {
    Pop-Location
}
Write-Host "Lore build OK." -ForegroundColor Green

# -- 2. Stop the public-facing backend process only ----------------------------
Write-Step "Stopping the public-facing (0.0.0.0:9000) backend process"
$conn = Get-NetTCPConnection -LocalPort 9000 -LocalAddress 0.0.0.0 -ErrorAction SilentlyContinue
if ($conn) {
    Write-Host "Stopping PID $($conn.OwningProcess)"
    Stop-Process -Id $conn.OwningProcess -Force
    Start-Sleep -Seconds 2
} else {
    Write-Host "No process currently bound to 0.0.0.0:9000 - nothing to stop."
}

# -- 3. Start a fresh backend process -------------------------------------------
Write-Step "Starting fresh backend process"
Start-Process -FilePath $PythonExe `
    -ArgumentList @('-m', 'uvicorn', 'sustena.api.main:app', '--host', '0.0.0.0', '--port', '9000') `
    -WorkingDirectory $ApiDir `
    -WindowStyle Hidden

# -- 4. Verify --------------------------------------------------------------
Write-Step "Verifying"
$healthUrl = Get-BackendCheckUrl
Write-Host "Health-check target: $healthUrl"

$ok = $false
$body = $null
for ($i = 0; $i -lt 6; $i++) {
    Start-Sleep -Seconds 5
    try {
        $r = Invoke-WebRequest -Uri $healthUrl -TimeoutSec 5 -UseBasicParsing
        if ($r.StatusCode -eq 200) {
            $body = $r.Content | ConvertFrom-Json
            if ($body.status -eq 'ok') { $ok = $true; break }
        }
    } catch {
        # not up yet, keep polling
    }
}

if (-not $ok) {
    Write-Host "DEPLOY FAILED: backend did not report healthy within 30s. Check for a startup error (missing dependency, syntax error, etc)." -ForegroundColor Red
    exit 1
}

Write-Host "Backend healthy: db_status=$($body.db_status) claude_status=$($body.claude_status)" -ForegroundColor Green

if ($body.git_commit -eq $headBefore) {
    Write-Host "VERIFIED: running commit ($($body.git_commit)) matches repo HEAD ($headBefore)." -ForegroundColor Green
} elseif ($dirty) {
    Write-Host "Running commit reports $($body.git_commit) (repo HEAD is $headBefore) - expected, this was a -AllowDirty deploy of an uncommitted tree." -ForegroundColor Yellow
} else {
    Write-Host "MISMATCH: running commit ($($body.git_commit)) does not match repo HEAD ($headBefore). Something is wrong - investigate before trusting this deploy." -ForegroundColor Red
    exit 1
}

Write-Host ""
Write-Host "Deploy complete." -ForegroundColor Green
