<#
.SYNOPSIS
    Registers the Sustena keepalive watchdog as a Windows Task Scheduler task:
    fires every 5 minutes, indefinitely. Safe to re-run - replaces any
    existing registration of the same name.

.DESCRIPTION
    This script VERIFIES the registration by querying the task back after
    creating it, rather than assuming Register-ScheduledTask worked. It only
    prints success if Get-ScheduledTask confirms the task actually exists
    with the expected trigger. On any failure, it prints the real error and
    stops - no "next step" instructions for a task that isn't there.

    Two Windows quirks this works around, both verified empirically on this
    machine before finalizing this script rather than assumed:

    1. New-ScheduledTaskTrigger's -RepetitionDuration parameter serializes
       [TimeSpan]::MaxValue (or any very large span) into an out-of-range
       ISO 8601 duration string that Task Scheduler's XML validator rejects
       (HRESULT 0x80041318, "Duration:P99999999DT23H59M59S"). Fix: build the
       trigger, then set its Repetition.Duration to an EMPTY string directly.
       An empty <Duration> is what Task Scheduler's own "repeat indefinitely"
       option produces - it is not the same as a huge duration.

    2. An -AtLogOn trigger is deliberately NOT used here. Tested directly on
       this machine: registering ANY task with an AtLogOn trigger fails with
       "Access is denied" from a non-elevated PowerShell session - with or
       without an explicit -Principal, with or without other triggers. Only
       fixable by running this script elevated (Run as Administrator), which
       is an extra hurdle for a script meant to "just work." Instead, this
       task uses only a repeating trigger with -StartWhenAvailable, which
       does NOT require elevation (verified) and, per Task Scheduler's
       documented behavior for missed triggers, should also pick up shortly
       after you log back in following a reboot or logoff - though that
       specific catch-up path needs an actual logoff/logon cycle to observe,
       which wasn't practical to test in this session. Flagging that as
       documented-but-not-directly-verified, not asserting it as confirmed.

    Since you're already logged in when you run this, "every 5 minutes
    starting now" needs a nudge to produce an immediate first run rather than
    making you wait up to 5 minutes - so after a verified registration, this
    script runs the task once immediately (Start-ScheduledTask).

.NOTES
    Run this once, as yourself - no admin/elevation needed for this version:
        powershell -ExecutionPolicy Bypass -File .\scripts\register-task.ps1

    If you'd rather have a true AtLogOn trigger as well (belt-and-suspenders
    for the reboot case), re-run this from an elevated PowerShell and ask to
    add it back in - that combination registers fine when elevated, just not
    from a normal session.
#>

$TaskName    = 'SustenaKeepalive'
$RepoRoot    = Split-Path -Parent $PSScriptRoot
$ScriptPath  = Join-Path $PSScriptRoot 'keepalive.ps1'
$LauncherPath = Join-Path $PSScriptRoot 'keepalive_launcher.vbs'
$LogFile     = Join-Path $PSScriptRoot 'logs\keepalive.log'

if (-not (Test-Path $ScriptPath)) {
    Write-Host "FAILED: can't find keepalive.ps1 at $ScriptPath - run this from an unmoved checkout."
    exit 1
}
if (-not (Test-Path $LauncherPath)) {
    Write-Host "FAILED: can't find keepalive_launcher.vbs at $LauncherPath - run this from an unmoved checkout."
    exit 1
}

# Launch via the VBS wrapper, not powershell.exe directly -- wscript.exe never
# allocates a console, so there is no window to flash. See keepalive_launcher.vbs.
$action = New-ScheduledTaskAction -Execute 'wscript.exe' `
    -Argument "`"$LauncherPath`""

# The fix: build with -RepetitionInterval only, then clear Duration directly
# to an empty string. Do NOT pass -RepetitionDuration with a large TimeSpan -
# that's what produces the out-of-range XML this workaround avoids.
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
        -Description "Checks Sustena backend (9000) and frontend (3000), restarts whichever is down. See $RepoRoot\scripts\keepalive.ps1" `
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

# --- Verify, don't assume. Query the task back and check what's actually there. ---
$task = Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue

if (-not $task) {
    Write-Host "REGISTRATION FAILED: Register-ScheduledTask did not throw, but Get-ScheduledTask can't find '$TaskName' afterward."
    exit 1
}

$hasRepeatTrigger = $task.Triggers | Where-Object {
    $_.CimClass.CimClassName -eq 'MSFT_TaskTimeTrigger' -and $_.Repetition.Interval -eq 'PT5M'
}
$actionPath = $task.Actions[0].Execute

if (-not $hasRepeatTrigger) {
    Write-Host "REGISTRATION INCOMPLETE: task '$TaskName' exists but doesn't have the expected 5-minute repeat trigger."
    Write-Host "Not treating this as success. Check manually: Get-ScheduledTask -TaskName '$TaskName' | Select -Expand Triggers"
    exit 1
}

Write-Host "VERIFIED: '$TaskName' is registered, repeats every 5 minutes indefinitely, State=$($task.State)."
Write-Host "  Action: $actionPath"

# --- Run it once now, rather than making you wait up to 5 minutes. ---
Write-Host ""
Write-Host "Running it once now to confirm end-to-end..."
try {
    Start-ScheduledTask -TaskName $TaskName -ErrorAction Stop
} catch {
    Write-Host "VERIFIED registration, but the immediate test run failed to launch: $($_.Exception.Message)"
    Write-Host "The task is registered correctly and will still fire on its own schedule."
    exit 1
}

Start-Sleep -Seconds 3
$runningTask = Get-ScheduledTask -TaskName $TaskName | Get-ScheduledTaskInfo
Write-Host "  LastRunTime: $($runningTask.LastRunTime)"
Write-Host "  LastTaskResult: $($runningTask.LastTaskResult) (0 or 267009/'still running' are both fine here - keepalive.ps1 can take up to ~25s if it has to confirm a start)"
Write-Host ""
Write-Host "Check $LogFile in about 30 seconds - if both servers are healthy it will be untouched (no new lines); if anything was down, you'll see exactly what it did."
