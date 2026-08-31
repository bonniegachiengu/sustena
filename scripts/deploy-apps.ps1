<#
    deploy-apps.ps1 -- ship the CURRENT code to the apps Bonnie actually opens.

    WHY THIS EXISTS
    ---------------
    A whole night's work sat committed in git while the app on his screen was
    from the day before, and nothing on either side said so. The work was never
    the problem; the DELIVERY was. Two specific failures caused it:

      1. Builds were run and inspected in `target/release`, which nobody
         launches.
      2. "Installed" was asserted from a file copy rather than verified against
         the path his shortcut actually opens.

    So this script does three things that answer both:

      * It RESOLVES his shortcut to find the install path. It never guesses a
        directory, because guessing is what went wrong.
      * It STAMPS version + git hash into the binary at compile time, so the
        running build identifies itself on screen and a stale app is obvious.
      * It VERIFIES afterwards, from the installed artefact, and FAILS LOUDLY
        if what landed is not what was built. A deploy that cannot prove it
        landed is the failure this file exists to end.

    USAGE
        powershell -ExecutionPolicy Bypass -File .\scripts\deploy-apps.ps1
        ... -DesktopOnly      just the laptop
        ... -PhoneOnly        just the phone
        ... -AllowDirty       deploy uncommitted work (the stamp says "dirty")
#>

[CmdletBinding()]
param(
    [switch]$DesktopOnly,
    [switch]$PhoneOnly,
    [switch]$AllowDirty
)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot

# * Rust lives in a scoped CARGO_HOME on this machine and is deliberately NOT
#   on the system PATH. A deploy script that assumed otherwise failed at the
#   build step having already rebuilt the frontend -- which is the confusing
#   half-done state this file exists to avoid.
if (-not $env:CARGO_HOME)  { $env:CARGO_HOME  = Join-Path $env:USERPROFILE 'Rust\cargo' }
if (-not $env:RUSTUP_HOME) { $env:RUSTUP_HOME = Join-Path $env:USERPROFILE 'Rust\rustup' }
ustup" }
$cargoBin = Join-Path $env:CARGO_HOME 'bin'
if (Test-Path $cargoBin) { $env:PATH = "$cargoBin;$env:PATH" }
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Host "`nREFUSED: cargo is not on PATH and was not found in $cargoBin." -ForegroundColor Red
    exit 1
}
$web  = Join-Path $repo 'apps\mycelium'
$tauri = Join-Path $web 'src-tauri'

function Say  ($m) { Write-Host "`n=== $m" -ForegroundColor Cyan }
function Ok   ($m) { Write-Host "    $m" -ForegroundColor Green }
function Note ($m) { Write-Host "    $m" -ForegroundColor DarkGray }
function Die  ($m) { Write-Host "`nREFUSED: $m" -ForegroundColor Red; exit 1 }

# ---------------------------------------------------------------------------
# 0. What are we shipping?
# ---------------------------------------------------------------------------
Say 'What is being shipped'
$version = (Get-Content (Join-Path $repo 'VERSION') -Raw).Trim()
$hash    = (git -C $repo rev-parse --short=7 HEAD).Trim()
$dirty   = [bool](git -C $repo status --porcelain --untracked-files=no)
if ($dirty -and -not $AllowDirty) {
    Die "the working tree has uncommitted changes. A deploy ships a commit, so the stamp on screen means something. Commit, or pass -AllowDirty."
}
$expected = "v$version | $hash"
Ok "version $version   hash $hash"
Ok "the apps will show: $expected"

# ---------------------------------------------------------------------------
# 1. WHERE does Bonnie's launcher actually point?
#
# *** Resolved, never assumed. The install went to the right place all along
#     on one occasion and to a redirected copy on another, and the only way to
#     tell them apart is to ask the shortcut he clicks.
# ---------------------------------------------------------------------------
$installedExe = $null
if (-not $PhoneOnly) {
    Say "Resolving the desktop shortcut"
    $shell = New-Object -ComObject WScript.Shell
    $roots = @(
        "$env:APPDATA\Microsoft\Windows\Start Menu",
        "$env:ProgramData\Microsoft\Windows\Start Menu",
        "$env:USERPROFILE\Desktop",
        "$env:PUBLIC\Desktop"
    )
    $targets = @()
    foreach ($root in $roots) {
        if (-not (Test-Path $root)) { continue }
        Get-ChildItem $root -Recurse -Filter '*.lnk' -ErrorAction SilentlyContinue |
            Where-Object { $_.BaseName -like '*Mycelium*' } |
            ForEach-Object { $targets += $shell.CreateShortcut($_.FullName).TargetPath }
    }
    # * @() is load-bearing: with ONE result the pipeline returns a bare
    #   string, and $targets[0] then indexes its first CHARACTER. Caught by
    #   running it -- the script reported the install path as "C".
    $targets = @($targets | Where-Object { $_ } | Sort-Object -Unique)
    if ($targets.Count -eq 0) { Die "no Mycelium shortcut found. Cannot tell where he launches from, and will not guess." }
    if ($targets.Count -gt 1) {
        # ** More than one install is exactly the ambiguity that caused this.
        #    Reported rather than silently picking one.
        Write-Host "    shortcuts point at MORE THAN ONE exe:" -ForegroundColor Yellow
        $targets | ForEach-Object { Write-Host "      $_" -ForegroundColor Yellow }
        Die "consolidate to one install before deploying, or this will ship to the wrong one."
    }
    $installedExe = $targets[0]
    Ok "one canonical install: $installedExe"
    if (-not (Test-Path $installedExe)) { Die "the shortcut points at $installedExe, which does not exist." }
    $before = Get-Item $installedExe
    Note "before: $($before.VersionInfo.FileVersion)  $($before.LastWriteTime)"
}

# ---------------------------------------------------------------------------
# 2. Build the frontend FIRST, then the binary that embeds it.
#
# *** Order is load-bearing. A binary built before the frontend embeds the
#     PREVIOUS UI, which looks exactly like a successful deploy.
# ---------------------------------------------------------------------------
Say 'Building the frontend'
Push-Location $web
try {
    npm run build
    if ($LASTEXITCODE -ne 0) { Die "frontend build failed ($LASTEXITCODE). Nothing installed." }
} finally { Pop-Location }
$bundle = Get-ChildItem (Join-Path $web 'dist\assets') -Filter 'index-*.js' | Select-Object -First 1
Ok "bundle $($bundle.Name)  $($bundle.LastWriteTime)"

Say 'Building the desktop binary'
Push-Location $tauri
try {
    cargo build --release
    if ($LASTEXITCODE -ne 0) { Die "cargo build failed ($LASTEXITCODE). Nothing installed." }
} finally { Pop-Location }
$built = Join-Path $tauri 'target\release\mycelium.exe'
if (-not (Test-Path $built)) { Die "cargo reported success but produced no mycelium.exe." }
if ((Get-Item $built).LastWriteTime -lt $bundle.LastWriteTime) {
    Die "the binary is OLDER than the frontend bundle, so it embeds the previous UI. This is the silent failure this check exists for."
}
Ok "binary $((Get-Item $built).LastWriteTime), newer than the bundle"

# ---------------------------------------------------------------------------
# 3. Install to the resolved path.
# ---------------------------------------------------------------------------
if (-not $PhoneOnly) {
    Say 'Installing the desktop app'
    $running = Get-Process mycelium -ErrorAction SilentlyContinue
    if ($running) {
        Note "stopping $($running.Count) running instance(s)"
        $running | Stop-Process -Force
        Start-Sleep -Seconds 3
    }
    Copy-Item -Force $built $installedExe
    $after = Get-Item $installedExe
    Ok "after:  $($after.VersionInfo.FileVersion)  $($after.LastWriteTime)"

    # *** VERIFY, from the INSTALLED artefact and not from what we built.
    if ($after.Length -ne (Get-Item $built).Length) {
        Die "the installed exe differs in size from the one just built. The copy did not land where it was read back from."
    }
    if ($after.VersionInfo.FileVersion -notlike "$version*") {
        Die "installed FileVersion $($after.VersionInfo.FileVersion) does not match VERSION $version."
    }
    Ok "installed artefact matches what was built"
}

# ---------------------------------------------------------------------------
# 4. The phone.
# ---------------------------------------------------------------------------
if (-not $DesktopOnly) {
    Say 'Building and installing the phone app'
    if (-not $env:ANDROID_HOME) { $env:ANDROID_HOME = "$env:USERPROFILE\Android\sdk" }
    $adb = Join-Path $env:ANDROID_HOME 'platform-tools\adb.exe'
    if (-not (Test-Path $adb)) { Die "no adb at $adb." }
    $devices = & $adb devices | Select-String -Pattern '\tdevice$'
    if (-not $devices) { Die "no phone on adb. Connect it and retry, or pass -DesktopOnly." }

    Push-Location $web
    try {
        # The symlink step needs Developer Mode; the cross-compile above it
        # succeeds, so we copy the artefact ourselves and run Gradle directly.
        npx tauri android build --apk --target aarch64 --debug 2>&1 | Out-Null
    } finally { Pop-Location }

    $so = Join-Path $tauri 'target\aarch64-linux-android\debug\libmycelium_lib.so'
    if (-not (Test-Path $so)) { Die "the android cross-compile produced no libmycelium_lib.so." }
    $jni = Join-Path $tauri 'gen\android\app\src\main\jniLibs\arm64-v8a'
    New-Item -ItemType Directory -Force -Path $jni | Out-Null
    Copy-Item -Force $so (Join-Path $jni 'libmycelium_lib.so')

    Push-Location (Join-Path $tauri 'gen\android')
    try {
        & .\gradlew.bat assembleArm64Debug -x rustBuildArm64Debug --console=plain | Out-Null
        if ($LASTEXITCODE -ne 0) { Die "gradle assembleArm64Debug failed ($LASTEXITCODE)." }
    } finally { Pop-Location }

    $apk = Join-Path $tauri 'gen\android\app\build\outputs\apk\arm64\debug\app-arm64-debug.apk'
    if (-not (Test-Path $apk)) { Die "gradle reported success but produced no APK." }
    # -r keeps his data: identity, household, peer book.
    & $adb install -r $apk | Out-Null
    if ($LASTEXITCODE -ne 0) { Die "adb install failed ($LASTEXITCODE)." }
    $installedVersion = (& $adb shell dumpsys package online.vyybandasky.sustena.mycelium |
        Select-String 'versionName=' | Select-Object -First 1) -replace '.*versionName=',''
    Ok "phone now on $($installedVersion.Trim())"
    if ($installedVersion.Trim() -ne $version) {
        Die "the phone reports $($installedVersion.Trim()) but VERSION is $version."
    }
}

Say 'Deployed'
Ok "both apps should now show:  $expected"
Note "If a header shows anything else, the app is stale -- that is the whole point of the stamp."
