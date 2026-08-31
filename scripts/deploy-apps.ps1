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
# *** The EXACT string the header renders, not an approximation of it.
#     `commands.rs` formats "v{version} <middot> {hash}", so a script that
#     promised "v1.1.2 | abc1234" would send him looking for text no app ever
#     prints. Built from a char code to keep this file ASCII -- a literal
#     non-ASCII glyph in here once broke the PowerShell 5.1 parser outright.
$dot      = [char]0x00B7
$expected = "v$version $dot $hash"
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
# * The bundle's CONTENT is what gets embedded, not its timestamp. Recorded
#   before the build so a no-op rebuild is not mistaken for a stale binary.
$distAssets = Join-Path $web 'dist\assets'
$bundleBefore = $null
if (Test-Path $distAssets) {
    $b = Get-ChildItem $distAssets -Filter 'index-*.js' | Select-Object -First 1
    if ($b) { $bundleBefore = (Get-FileHash $b.FullName -Algorithm SHA256).Hash }
}
Push-Location $web
try {
    npm run build
    if ($LASTEXITCODE -ne 0) { Die "frontend build failed ($LASTEXITCODE). Nothing installed." }
} finally { Pop-Location }
$bundle = Get-ChildItem (Join-Path $web 'dist\assets') -Filter 'index-*.js' | Select-Object -First 1
$bundleAfter = (Get-FileHash $bundle.FullName -Algorithm SHA256).Hash
$uiChanged = ($bundleBefore -ne $bundleAfter)
Ok "bundle $($bundle.Name)  $($bundle.LastWriteTime)"
Note $(if ($uiChanged) { 'the UI changed, so the binary must be rebuilt after it' } else { 'the UI is byte-identical to the last build' })

$built = Join-Path $tauri 'target\release\mycelium.exe'
if (-not $PhoneOnly) {
    Say 'Building the desktop binary'
    Push-Location $tauri
    try {
        cargo build --release
        if ($LASTEXITCODE -ne 0) { Die "cargo build failed ($LASTEXITCODE). Nothing installed." }
    } finally { Pop-Location }
    if (-not (Test-Path $built)) { Die "cargo reported success but produced no mycelium.exe." }

    # *** The staleness rule, stated over CONTENT rather than clocks.
    #     cargo does not relink when nothing changed, so a binary older than a
    #     rebuilt-but-identical bundle is fine -- it already embeds those exact
    #     bytes. Only a bundle whose CONTENT moved can leave a binary stale.
    if ($uiChanged -and (Get-Item $built).LastWriteTime -lt $bundle.LastWriteTime) {
        Die "the UI changed but the binary is older than it, so it embeds the previous UI. This is the silent failure this check exists for."
    }
    Ok "binary $((Get-Item $built).LastWriteTime)"
}

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

    # =======================================================================
    # SELF-VERIFY -- a deploy that cannot PROVE it landed the right bytes
    # must fail, not print success.
    #
    # *** Two "verified" reports on 31 Aug were wrong because they asserted
    #     from the act of copying rather than reading back what arrived.
    #     Everything below reads the INSTALLED file.
    # =======================================================================
    $srcHash = (Get-FileHash $built -Algorithm SHA256).Hash
    $depHash = (Get-FileHash $installedExe -Algorithm SHA256).Hash
    Note "source   $srcHash"
    Note "deployed $depHash"
    if ($srcHash -ne $depHash) {
        Die "the deployed exe does not hash-match the one just built. The copy did not land where it was read back from -- that is the 'shipped to a path he never opens' bug, caught."
    }
    Ok 'hash match: the bytes at his launch path are the bytes just built'

    if ($after.VersionInfo.FileVersion -notlike "$version*") {
        Die "installed FileVersion $($after.VersionInfo.FileVersion) does not match VERSION $version."
    }

    # *** The header string, confirmed INSIDE the artefact.
    #
    # ** Why this particular check is sound when grepping for UI text is not:
    #    Tauri embeds the frontend compressed, so screen text is NOT findable
    #    in the binary -- proven by control, since wording visibly on his lock
    #    screen does not appear in it either. The build hash arrives through an
    #    `env!` macro instead, as a plain Rust string, so it genuinely is
    #    findable. It is also the one thing he reads off the screen to know
    #    whether he is current, which makes it worth asserting.
    $blob = [System.Text.Encoding]::ASCII.GetString([System.IO.File]::ReadAllBytes($installedExe))
    if (-not $blob.Contains($hash)) {
        Die "the deployed exe does not contain the build hash '$hash', so its header cannot render '$expected'. It was built from a different commit than this deploy believes."
    }
    Ok "header will render: $expected"

    # =======================================================================
    # THE WEBVIEW CACHE. Mandatory, every time.
    #
    # *** On 31 Aug a hash-correct exe was installed and the next launch STILL
    #     showed the old interface. Tauri hands the frontend to WebView2, which
    #     caches it under %LOCALAPPDATA% -- and that cache OUTLIVES the exe. A
    #     new binary can therefore serve the previous UI, which looks exactly
    #     like a deploy that never happened. Clearing it by hand once was not a
    #     fix; doing it on every deploy is.
    #
    # ** ONLY the asset caches, and ONLY under Local. His household --
    #    identity, peers.json, events -- lives in ROAMING and is never touched
    #    by anything in this script.
    # =======================================================================
    $identifier = (Get-Content (Join-Path $tauri 'tauri.conf.json') -Raw | ConvertFrom-Json).identifier
    $webview = Join-Path $env:LOCALAPPDATA (Join-Path $identifier 'EBWebView\Default')
    if (Test-Path $webview) {
        foreach ($c in 'Cache', 'Code Cache', 'GPUCache') {
            $p = Join-Path $webview $c
            if (Test-Path $p) {
                Remove-Item -Recurse -Force $p -ErrorAction SilentlyContinue
                if (Test-Path $p) {
                    Die "could not clear the WebView cache at $p. Close the app and run again -- leaving it risks serving the old UI out of the new binary, which is the failure this step exists for."
                }
                Note "cleared WebView $c"
            }
        }
        Ok 'WebView asset caches cleared -- the new UI cannot be masked by the old'
    } else {
        Note 'no WebView cache present (first run on this machine)'
    }
    $roaming = Join-Path $env:APPDATA $identifier
    if (Test-Path $roaming) { Ok "household data untouched at $roaming" }
}

# ---------------------------------------------------------------------------
# 4. The phone.
# ---------------------------------------------------------------------------
$phoneSkipped = $false
if (-not $DesktopOnly) {
    Say 'The phone'
    if (-not $env:ANDROID_HOME) { $env:ANDROID_HOME = "$env:USERPROFILE\Android\sdk" }
    $adb = Join-Path $env:ANDROID_HOME 'platform-tools\adb.exe'
    if (-not (Test-Path $adb)) { Die "no adb at $adb." }
    $devices = & $adb devices | Select-String -Pattern '\tdevice$'

    # *** A NO-OP, not a failure. His phone is off USB most of the time and
    #     that is normal, not an error. A deploy that goes red for the ordinary
    #     case trains a person to ignore red -- which is exactly how a real
    #     failure gets waved through. The desktop half above still stands on
    #     its own, and this says plainly that it did.
    if (-not $devices) {
        Note 'no device on adb -- skipping the phone, which is fine.'
        Note 'the desktop deploy above is complete and verified on its own.'
        $phoneSkipped = $true
    }
}
if (-not $DesktopOnly -and -not $phoneSkipped) {
    Say 'Building and installing the phone app'

    # *** These are native commands that write progress to stderr. In
    #     PowerShell 5.1 that becomes a NativeCommandError and, under
    #     ErrorActionPreference=Stop, kills the run over ordinary output.
    #     Exit codes are checked explicitly instead, which is the real signal.
    $priorEap = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    Push-Location $web
    try {
        # The symlink step needs Developer Mode; the cross-compile above it
        # succeeds, so we copy the artefact ourselves and run Gradle directly.
        & npx tauri android build --apk --target aarch64 --debug | Out-Null
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
    $ErrorActionPreference = $priorEap
    $installedVersion = (& $adb shell dumpsys package online.vyybandasky.sustena.mycelium |
        Select-String 'versionName=' | Select-Object -First 1) -replace '.*versionName=',''
    Ok "phone now on $($installedVersion.Trim())"
    if ($installedVersion.Trim() -ne $version) {
        Die "the phone reports $($installedVersion.Trim()) but VERSION is $version."
    }
}

Say 'Deployed'
if ($phoneSkipped) {
    Ok "Mycelium on this machine now shows:  $expected"
    Note 'The phone was not attached, so it is untouched and still on whatever it had.'
} else {
    Ok "both apps should now show:  $expected"
}
Note 'If a header shows anything else, the app is stale -- that is the whole point of the stamp.'
