# Sustena Lore -- the watchdog, as a loop instead of a scheduled task.
#
# lore-keepalive.ps1 is written to be run repeatedly by Task Scheduler, which is
# where it belongs. Registering that task needs elevation, and this account does
# not have it, so this script supplies the same repetition from user space: run
# the check, sleep, run it again, forever.
#
# It is strictly a fallback. Once the task in docs/LORE_STATUS.md is registered
# from an elevated shell, this can be killed and forgotten -- the task does the
# same job and survives a reboot, which this cannot.
#
# Launched detached and hidden by lore_watchdog_launcher.vbs.

$ErrorActionPreference = 'Continue'

$here      = Split-Path -Parent $MyInvocation.MyCommand.Path
$keepalive = Join-Path $here 'lore-keepalive.ps1'
$log       = 'C:\Users\DELL\dev\sustena-lore\apps\api\lore-keepalive.log'

Add-Content -Path $log -Encoding utf8 -Value (
    '{0}  watchdog loop started (pid {1})' -f (Get-Date -Format 'yyyy-MM-dd HH:mm:ss'), $PID
)

while ($true) {
    try {
        & $keepalive
    } catch {
        # ★ A failed check must never end the loop -- that would turn one bad
        #   minute into the site being down until somebody noticed.
        Add-Content -Path $log -Encoding utf8 -Value (
            '{0}  watchdog: check threw: {1}' -f (Get-Date -Format 'yyyy-MM-dd HH:mm:ss'), $_.Exception.Message
        )
    }
    Start-Sleep -Seconds 300
}
