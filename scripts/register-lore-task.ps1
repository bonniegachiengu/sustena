<#
.SYNOPSIS
    Registers the Sustena Lore watchdog as a Windows Task Scheduler task:
    fires every 5 minutes, indefinitely. Safe to re-run - replaces any
    existing registration of the same name.

.DESCRIPTION
    lore-keepalive.ps1 has referenced this script in its own header since it
    was written, and lore_keepalive_launcher.vbs names it as its registrar,
    but it was never actually created - so the 'SustenaLore' task has never
    existed on this machine. That is the whole reason the blog goes down and
    stays down: the process dies with the session, and nothing brings it back.
    Confirmed directly (Get-ScheduledTask returned nothing for 'SustenaLore'
    while 'SustenaKeepalive' was present and healthy) rather than inferred.

    This mirrors register-task.ps1 exactly, including the two Windows quirks
    that script established empirically on this machine. They are restated
    here because they are the reason this file looks the way it does, not
    because they were re-derived:

    1. New-ScheduledTaskTrigger's -RepetitionDuration serializes a large
       TimeSpan into an out-of-range ISO 8601 duration that Task Scheduler's
       XML validator rejects (HRESULT 0x80041318). Fix: build the trigger with
       -RepetitionInterval only, then set Repetition.Duration to an EMPTY
       string, which is what the GUI's "repeat indefinitely" produces.

    2. No -AtLogOn trigger: registering any task with one fails with "Access
       is denied" from a non-elevated session. A repeating trigger with
       -StartWhenAvailable needs no elevation and picks up missed runs.

    Like register-task.ps1, this VERIFIES by querying the task back rather
    than assuming Register-ScheduledTask worked, and prints success only if
    the expected trigger is actually there.

.NOTES
    Run once, as yourself - no elevation needed:
        powershell -ExecutionPolicy Bypass -File .\scripts\register-lore-task.ps1
#>

$TaskName     = 'SustenaLore'
$RepoRoot     = Split-Path -Parent $PSScriptRoot
$ScriptPath   = Join-Path $PSScriptRoot 'lore-keepalive.ps1'
$LauncherPath = Join-Path $PSScriptRoot 'lore_keepalive_launcher.vbs'

if (-not (Test-Path $ScriptPath)) {
    Write-Host "FAILED: can't find lore-keepalive.ps1 at $ScriptPath - run this from an unmoved checkout."
    exit 1
}
if (-not (Test-Path $LauncherPath)) {
    Write-Host "FAILED: can't find lore_keepalive_launcher.vbs at $LauncherPath - run this from an unmoved checkout."
    exit 1
}

# Launch via the VBS wrapper, not powershell.exe directly -- wscript.exe never
# allocates a console, so there is no window to flash.
$action = New-ScheduledTaskAction -Execute 'wscript.exe' -Argument "`"$LauncherPath`""

# Build with -RepetitionInterval only, then clear Duration to an empty string.
$trigger = New-ScheduledTaskTrigger -Once -At (Get-Date) -RepetitionInterval (New-TimeSpan -Minutes 5)
$trigger.Repetition.Duration = ''
$trigger.Repetition.StopAtDurationEnd = $false

$settings = New-ScheduledTaskSettingsSet `
    -MultipleInstances IgnoreNew `
    -ExecutionTimeLimit (New-TimeSpan -Minutes 10) `
    -AllowStartIfOnBatteries `
    -DontStopIfGoingOnBatteries `
    -StartWhenAvailable

Unregister-ScheduledTask -TaskName $TaskName -Confirm:$false -ErrorAction SilentlyContinue

$registerFailed = $false
$registerError  = $null
try {
    Register-ScheduledTask -TaskName $TaskName `
        -Action $action `
        -Trigger $trigger `
        -Settings $settings `
        -Description "Keeps the Sustena Lore site answering on 127.0.0.1:9100, which the cloudflared ingress for lore.vyybandasky.online points at. See $RepoRoot\scripts\lore-keepalive.ps1" `
        -ErrorAction Stop `
        | Out-Null
} catch {
    $registerFailed = $true
    $registerError  = $_.Exception.Message
}

if ($registerFailed) {
    Write-Host "REGISTRATION FAILED: $registerError"
    Write-Host "The task was NOT created. Nothing below this line applies."
    exit 1
}

# --- Verify, don't assume. ---
$task = Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue

if (-not $task) {
    Write-Host "REGISTRATION FAILED: Register-ScheduledTask did not throw, but Get-ScheduledTask can't find '$TaskName' afterward."
    exit 1
}

$hasRepeatTrigger = $task.Triggers | Where-Object {
    $_.CimClass.CimClassName -eq 'MSFT_TaskTimeTrigger' -and $_.Repetition.Interval -eq 'PT5M'
}

if (-not $hasRepeatTrigger) {
    Write-Host "REGISTRATION INCOMPLETE: task '$TaskName' exists but doesn't have the expected 5-minute repeat trigger."
    Write-Host "Not treating this as success. Check: Get-ScheduledTask -TaskName '$TaskName' | Select -Expand Triggers"
    exit 1
}

Write-Host "VERIFIED: '$TaskName' is registered, repeats every 5 minutes indefinitely, State=$($task.State)."
Write-Host "  Action: $($task.Actions[0].Execute) $($task.Actions[0].Arguments)"

Write-Host ""
Write-Host "Running it once now to confirm end-to-end..."
try {
    Start-ScheduledTask -TaskName $TaskName -ErrorAction Stop
} catch {
    Write-Host "VERIFIED registration, but the immediate test run failed to launch: $($_.Exception.Message)"
    Write-Host "The task is registered correctly and will still fire on its own schedule."
    exit 1
}

Start-Sleep -Seconds 6

# The real check is not the task's exit code but whether the blog answers.
$serving = $false
try {
    $r = Invoke-WebRequest -Uri 'http://127.0.0.1:9100/' -UseBasicParsing -TimeoutSec 8
    $serving = ($r.StatusCode -eq 200 -and $r.Content -match 'Sustena Lore')
} catch { }

if ($serving) {
    Write-Host "CONFIRMED: the blog is answering on 127.0.0.1:9100 (and the tunnel maps lore.vyybandasky.online to it)."
} else {
    Write-Host "The task ran, but 127.0.0.1:9100 is not serving the blog yet."
    Write-Host "Check $(Join-Path $RepoRoot 'apps\api\lore-keepalive.log') for what it reported."
}
