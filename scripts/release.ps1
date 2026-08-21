<#
.SYNOPSIS
  Cut a release: gate, bump, build both apps, tag, stage installers, changelog.

.DESCRIPTION
  ONE COMMAND. It refuses more readily than it proceeds, because a release that
  half-happened is worse than one that did not start.

  It will REFUSE, before touching anything, if:
    * the working tree has uncommitted changes to tracked files
      (a release ships a COMMIT, not whatever happens to be lying around)
    * you are not on `main` (tags belong on the stable line; -AllowBranch to override)
    * the tag for the resulting version already exists (that release was already cut)
    * any part of the gate is red -- tests, clippy, tsc, or the frontend build

  Then, in order:
    1. bump VERSION and propagate it (scripts/sync-version.ps1)
    2. build the desktop app  -> MSI + NSIS
    3. build the Android app  -> arm64 debug APK
    4. write CHANGELOG.md
    5. commit the bump, tag vX.Y.Z
    6. copy both installers into sustena-installers/ named with the version

  If any BUILD step fails, every file it touched is reverted and no tag is
  written -- so a failed release leaves the repo exactly as it found it.

  MEMORY CEILING. Everything Rust runs at -j 2 / CARGO_BUILD_JOBS=2. This is
  not caution for its own sake: on 2026-08-21 a cold full-parallelism build on
  this machine crashed rustc with STATUS_STACK_BUFFER_OVERRUN and the linker
  with LNK1102 (out of memory), and left corrupted `core` metadata behind that
  looked like a toolchain fault and was not. -j 2 was clean end to end. This is
  a property of THIS machine; CI runners use full parallelism.

.PARAMETER Bump
  patch | minor | major. Required unless -DryRun with no bump intended.

.PARAMETER Notes
  A line (or lines) to head this version's changelog entry. Commit subjects
  since the last tag are appended underneath automatically.

.PARAMETER DryRun
  Run the gate and report exactly what WOULD change. Writes nothing, builds
  nothing, tags nothing.

.PARAMETER SkipApps
  Gate, bump, changelog and tag, but do not build installers. For a version cut
  that does not need artifacts. Named honestly in the changelog entry.

.PARAMETER AllowBranch
  Permit cutting from a branch other than main. For rehearsal only.

.EXAMPLE
  .\scripts\release.ps1 -Bump minor -Notes "Peer transport, quorum writes, trust."
  .\scripts\release.ps1 -Bump patch -DryRun
#>
[CmdletBinding()]
param(
    [ValidateSet('patch', 'minor', 'major')]
    [string]$Bump,
    [string[]]$Notes,
    [switch]$DryRun,
    [switch]$SkipApps,
    [switch]$AllowBranch
)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
Set-Location $repo

function Say($msg, $colour = 'Cyan') { Write-Host "`n=== $msg" -ForegroundColor $colour }
function Ok($msg) { Write-Host "    $msg" -ForegroundColor Green }
function Note($msg) { Write-Host "    $msg" -ForegroundColor DarkGray }
function Die($msg) { Write-Host "`nREFUSED: $msg" -ForegroundColor Red; exit 1 }

# ---------------------------------------------------------------------------
# Toolchain environment. Scoped to this process -- nothing here touches the
# machine's own PATH or environment variables.
# ---------------------------------------------------------------------------
$env:CARGO_HOME = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { 'C:\Users\DELL\Rust\cargo' }
$env:RUSTUP_HOME = if ($env:RUSTUP_HOME) { $env:RUSTUP_HOME } else { 'C:\Users\DELL\Rust\rustup' }
$env:PATH = "$($env:CARGO_HOME)\bin;$($env:PATH)"
$env:CARGO_BUILD_JOBS = '2'

if (-not $env:JAVA_HOME) { $env:JAVA_HOME = 'C:\Program Files\Eclipse Adoptium\jdk-21.0.6.7-hotspot' }
if (-not $env:ANDROID_HOME) { $env:ANDROID_HOME = 'C:\Users\DELL\Android\sdk' }
if (-not $env:NDK_HOME) { $env:NDK_HOME = 'E:\Android\ndk-root\ndk\27.2.12479018' }
$env:ANDROID_NDK_ROOT = $env:NDK_HOME

# ===========================================================================
# 0. REFUSALS -- all of them, before anything is touched
# ===========================================================================
Say 'Pre-flight'

$branch = (git rev-parse --abbrev-ref HEAD).Trim()
if ($branch -ne 'main' -and -not $AllowBranch) {
    Die "on branch '$branch', not 'main'. A tag belongs on the stable line. Merge first, or pass -AllowBranch to rehearse."
}
Ok "branch: $branch"

$dirty = git status --porcelain --untracked-files=no
if ($dirty) {
    Write-Host $dirty -ForegroundColor Yellow
    Die "the working tree has uncommitted changes. A release ships a commit, not the working tree. Commit or stash first."
}
Ok 'working tree clean'

$versionFile = Join-Path $repo 'VERSION'
$current = (Get-Content $versionFile -Raw).Trim()
if ($current -notmatch '^(\d+)\.(\d+)\.(\d+)$') { Die "VERSION is not MAJOR.MINOR.PATCH (got '$current')." }
$maj = [int]$Matches[1]; $min = [int]$Matches[2]; $pat = [int]$Matches[3]

if ($Bump) {
    switch ($Bump) {
        'major' { $maj++; $min = 0; $pat = 0 }
        'minor' { $min++; $pat = 0 }
        'patch' { $pat++ }
    }
}
$next = "$maj.$min.$pat"
$tag = "v$next"
$androidCode = ($maj * 1000000) + ($min * 1000) + $pat
Ok "version: $current  ->  $next   (tag $tag, android versionCode $androidCode)"

if (-not $Bump -and -not $DryRun) { Die "no -Bump given. Pass patch, minor or major." }

$existing = git tag -l $tag
if ($existing) { Die "tag $tag already exists. That release was already cut -- bump further, or delete the tag deliberately." }
Ok "tag $tag is free"

# ===========================================================================
# 1. THE GATE -- red anywhere means no release
# ===========================================================================
# ---------------------------------------------------------------------------
# Run a native tool and return its EXIT CODE, which is the only thing that
# actually says whether it worked.
#
# WHY THIS EXISTS. cargo, npx and gradle all write ordinary progress to stderr.
# Under PowerShell 5.1, when a native command's stderr is redirected, each line
# comes back wrapped in a NativeCommandError record -- and with
# $ErrorActionPreference = 'Stop' that is TERMINATING. The script would abort on
# a tool that exited 0 and printed "Compiling ...". It cost one failed release
# run to find: clippy produced no output and the script died, while the same
# clippy passed fine when run unredirected.
#
# So: exit code is the truth, stderr text is not. EAP is lowered for the call
# and restored immediately after.
# ---------------------------------------------------------------------------
function Native([scriptblock]$cmd) {
    $prev = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        & $cmd
        return $LASTEXITCODE
    }
    finally { $ErrorActionPreference = $prev }
}

function Gate($label, $dir, [scriptblock]$cmd) {
    Say "Gate: $label"
    Push-Location $dir
    try {
        $code = Native $cmd
        if ($code -ne 0) { Die "$label failed (exit $code). Nothing has been changed." }
        Ok "$label green"
    }
    finally { Pop-Location }
}

Gate 'sustena-core tests' 'sustena-core' { cargo test -j 2 --quiet }
Gate 'sustena-core clippy' 'sustena-core' { cargo clippy --all-targets -j 2 -- -D warnings }
Gate 'mycelium host tests' 'apps/mycelium/src-tauri' { cargo test -j 2 --quiet }
Gate 'mycelium host clippy' 'apps/mycelium/src-tauri' { cargo clippy --all-targets -j 2 -- -D warnings }
Gate 'typescript (strict)' 'apps/mycelium' { npx tsc --noEmit }
Gate 'frontend build' 'apps/mycelium' { npm run build }

if ($DryRun) {
    Say 'DRY RUN -- gate is green, nothing was changed' 'Yellow'
    Note "would set VERSION to $next and propagate to 4 files"
    Note "would build: desktop MSI + NSIS, android arm64 debug APK"
    Note "would tag $tag and stage installers into sustena-installers/"
    exit 0
}

# ===========================================================================
# 2. BUMP -- from here on, failures revert
# ===========================================================================
$touched = @(
    'VERSION'
    'CHANGELOG.md'
    'sustena-core/Cargo.toml'
    'sustena-core/Cargo.lock'
    'apps/mycelium/src-tauri/Cargo.toml'
    'apps/mycelium/src-tauri/Cargo.lock'
    'apps/mycelium/src-tauri/tauri.conf.json'
    'apps/mycelium/package.json'
    'apps/mycelium/package-lock.json'
)

function RevertAll($why) {
    Write-Host "`nBUILD FAILED: $why" -ForegroundColor Red
    Write-Host "Reverting every file this run touched, so the repo is as it was..." -ForegroundColor Yellow
    foreach ($f in $touched) {
        if (Test-Path (Join-Path $repo $f)) { Native { git checkout -- $f } | Out-Null }
    }
    Write-Host "Reverted. No tag written. Nothing shipped." -ForegroundColor Yellow
    exit 1
}

Say "Bumping to $next"
# No $LASTEXITCODE check here on purpose: sync-version.ps1 is a PowerShell
# script, not a native command, so on success it leaves $LASTEXITCODE holding
# whatever the last native tool set -- checking it would be reading a stale
# value. It THROWS on every failure, and $ErrorActionPreference is Stop, so a
# failure propagates on its own.
& (Join-Path $PSScriptRoot 'sync-version.ps1') -Version $next

# ===========================================================================
# 3. BUILD BOTH APPS
# ===========================================================================
if (-not $SkipApps) {

    Say 'Building desktop (MSI + NSIS)'
    Push-Location 'apps/mycelium'
    $code = Native { npx tauri build }
    Pop-Location
    if ($code -ne 0) { RevertAll "desktop build failed (exit $code)" }
    Ok 'desktop built'

    Say 'Building Android (arm64 debug APK)'
    # ---------------------------------------------------------------------
    # KNOWN WINDOWS LIMIT, handled rather than worked around blindly.
    # `tauri android build` SYMLINKS the built .so into jniLibs, which needs
    # Developer Mode / SeCreateSymbolicLinkPrivilege. That is a machine
    # security setting and this script does not change machine settings. The
    # cargo cross-compile above it succeeds; only the placement fails. So we
    # let it build, COPY the artifact ourselves (identical result), and run
    # Gradle directly with its rust task excluded -- the .so is already there.
    # Gradle's own rustBuild task is also excluded because it shells out to
    # `npm.bat` and this Node ships `npm.cmd`.
    # ---------------------------------------------------------------------
    Push-Location 'apps/mycelium'
    # Its exit code is deliberately NOT checked: the symlink step always fails
    # here, and that failure is expected and handled below. What matters is
    # whether the cross-compile produced the .so, which is checked directly.
    Native { npx tauri android build --debug --target aarch64 } | Out-Null
    Pop-Location

    $so = Join-Path $repo 'apps/mycelium/src-tauri/target/aarch64-linux-android/debug/libmycelium_lib.so'
    if (-not (Test-Path $so)) { RevertAll 'android cross-compile produced no libmycelium_lib.so' }

    $jni = Join-Path $repo 'apps/mycelium/src-tauri/gen/android/app/src/main/jniLibs/arm64-v8a'
    New-Item -ItemType Directory -Force -Path $jni | Out-Null
    Copy-Item -Force $so (Join-Path $jni 'libmycelium_lib.so')
    Ok 'copied .so into jniLibs (symlink step needs Developer Mode; copy is identical)'

    Push-Location 'apps/mycelium/src-tauri/gen/android'
    $code = Native { & .\gradlew.bat assembleArm64Debug -x rustBuildArm64Debug --console=plain }
    Pop-Location
    if ($code -ne 0) { RevertAll "gradle assembleArm64Debug failed (exit $code)" }
    Ok 'android APK built'
}

# ===========================================================================
# 4. CHANGELOG
# ===========================================================================
Say 'Writing CHANGELOG.md'

# `git describe` writes to stderr when there is no tag at all, which is exactly
# the first-release case -- so it goes through Native like everything else.
$lastTag = $null
$prevEAP = $ErrorActionPreference; $ErrorActionPreference = 'Continue'
$lastTag = (git describe --tags --abbrev=0 2>$null | Select-Object -First 1)
$ErrorActionPreference = $prevEAP
$range = if ($lastTag) { "$lastTag..HEAD" } else { 'HEAD' }
$subjects = git log $range --no-merges --pretty=format:'%s' | Where-Object { $_ -and $_ -notmatch '^chore\(release\)' }

$stamp = (git log -1 --date=format:'%Y-%m-%d' --pretty=format:'%ad')
$entry = New-Object System.Text.StringBuilder
[void]$entry.AppendLine("## $tag - $stamp")
[void]$entry.AppendLine('')
if ($SkipApps) { [void]$entry.AppendLine('_Version cut only -- no installers were built for this release._'); [void]$entry.AppendLine('') }
if ($Notes) { foreach ($n in $Notes) { [void]$entry.AppendLine($n) }; [void]$entry.AppendLine('') }
if ($subjects) {
    foreach ($s in $subjects) { [void]$entry.AppendLine("- $s") }
}
else {
    [void]$entry.AppendLine('- No commits since the previous tag.')
}
[void]$entry.AppendLine('')

$changelogPath = Join-Path $repo 'CHANGELOG.md'
$header = @'
# Changelog

Every released version of Sustena, newest first. Versions are `MAJOR.MINOR.PATCH`
and the single source of truth is the `VERSION` file at the repo root.

'@

if (Test-Path $changelogPath) {
    $existingLog = Get-Content $changelogPath -Raw
    # Insert this entry directly under the header, above the previous newest.
    $marker = "`n## "
    $idx = $existingLog.IndexOf($marker)
    if ($idx -ge 0) {
        $body = $existingLog.Substring(0, $idx + 1) + $entry.ToString() + $existingLog.Substring($idx + 1)
    }
    else {
        $body = $existingLog.TrimEnd() + "`n`n" + $entry.ToString()
    }
}
else {
    $body = $header + $entry.ToString()
}
[System.IO.File]::WriteAllText($changelogPath, $body)
Ok 'changelog updated'

# ===========================================================================
# 5. COMMIT + TAG
# ===========================================================================
Say "Committing and tagging $tag"
# git is a native command too, and prints plenty to stderr on a good day.
foreach ($f in $touched) { if (Test-Path (Join-Path $repo $f)) { Native { git add $f } | Out-Null } }
$code = Native { git commit -m "chore(release): $tag" }
if ($code -ne 0) { Die "commit failed (exit $code)" }
$code = Native { git tag -a $tag -m "Sustena $tag" }
if ($code -ne 0) { Die "tag failed (exit $code)" }
Ok "committed and tagged $tag"

# ===========================================================================
# 6. STAGE THE INSTALLERS
# ===========================================================================
if (-not $SkipApps) {
    Say 'Staging installers'
    $out = 'C:\Users\DELL\dev\sustena-installers'
    New-Item -ItemType Directory -Force -Path $out | Out-Null

    $msi = Get-ChildItem "$repo\apps\mycelium\src-tauri\target\release\bundle\msi\*.msi" -EA SilentlyContinue | Select-Object -First 1
    $nsis = Get-ChildItem "$repo\apps\mycelium\src-tauri\target\release\bundle\nsis\*.exe" -EA SilentlyContinue | Select-Object -First 1
    $apk = "$repo\apps\mycelium\src-tauri\gen\android\app\build\outputs\apk\arm64\debug\app-arm64-debug.apk"

    if ($msi) { Copy-Item -Force $msi.FullName (Join-Path $out "Mycelium-Sustena-$next-x64.msi"); Ok "MSI  -> Mycelium-Sustena-$next-x64.msi" }
    if ($nsis) { Copy-Item -Force $nsis.FullName (Join-Path $out "Mycelium-Sustena-$next-x64-setup.exe"); Ok "NSIS -> Mycelium-Sustena-$next-x64-setup.exe" }
    if (Test-Path $apk) { Copy-Item -Force $apk (Join-Path $out "Mycelium-Orchie-$next-arm64-debug.apk"); Ok "APK  -> Mycelium-Orchie-$next-arm64-debug.apk" }
}

# ===========================================================================
Say "Released $tag" 'Green'
Write-Host @"

  Version   $next        (android versionCode $androidCode)
  Tag       $tag         (local -- not pushed yet)
  Installers in C:\Users\DELL\dev\sustena-installers

  To publish:
      git push origin main
      git push origin $tag

"@ -ForegroundColor Green
