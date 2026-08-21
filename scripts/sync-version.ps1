<#
.SYNOPSIS
  Propagate the version in /VERSION into every place a version number lives.

.DESCRIPTION
  ONE SOURCE OF TRUTH. The file /VERSION at the repo root holds a bare
  MAJOR.MINOR.PATCH line and nothing else. Everything else is downstream of it.

  There are four real fields, and one that only LOOKS like a fifth:

    1. sustena-core/Cargo.toml            version = "..."
    2. apps/mycelium/src-tauri/Cargo.toml version = "..."
    3. apps/mycelium/src-tauri/tauri.conf.json   "version": "..."
    4. apps/mycelium/package.json                "version": "..."

  The Android versionCode and versionName are NOT stored anywhere you edit.
  gen/android/app/build.gradle.kts reads them out of gen/android/app/
  tauri.properties, which the Tauri CLI regenerates from tauri.conf.json on
  every android build, using versionCode = major*1000000 + minor*1000 + patch.
  So 0.1.0 becomes 1000 and 0.2.0 becomes 2000, automatically. Setting field 3
  above IS setting the Android version -- there is nothing further to sync, and
  hand-editing tauri.properties would be overwritten on the next build.

.PARAMETER Version
  Optional. Write this version to /VERSION first, then propagate. Without it,
  /VERSION is read as-is and propagated.

.PARAMETER Check
  Verify only. Reports whether every field already agrees with /VERSION and
  exits non-zero if any disagree. Changes nothing. This is what CI runs.

.EXAMPLE
  .\scripts\sync-version.ps1
  .\scripts\sync-version.ps1 -Version 0.2.0
  .\scripts\sync-version.ps1 -Check
#>
[CmdletBinding()]
param(
    [string]$Version,
    [switch]$Check
)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$versionFile = Join-Path $repo 'VERSION'

# --- resolve the version ---------------------------------------------------
if ($Version) {
    if ($Check) { throw "-Version and -Check are mutually exclusive: one writes, the other refuses to." }
    if ($Version -notmatch '^\d+\.\d+\.\d+$') {
        throw "Version must be MAJOR.MINOR.PATCH (got '$Version')."
    }
    Set-Content -Path $versionFile -Value $Version -Encoding utf8 -NoNewline
    Add-Content -Path $versionFile -Value "" -Encoding utf8
}

if (-not (Test-Path $versionFile)) { throw "No VERSION file at $versionFile" }
$v = (Get-Content $versionFile -Raw).Trim()
if ($v -notmatch '^\d+\.\d+\.\d+$') {
    throw "VERSION must contain a bare MAJOR.MINOR.PATCH line (got '$v')."
}

# The Android versionCode Tauri will derive. Reported so it is never a mystery.
$parts = $v.Split('.')
$androidCode = ([int]$parts[0] * 1000000) + ([int]$parts[1] * 1000) + [int]$parts[2]

Write-Host "VERSION = $v   (android versionCode will be $androidCode)" -ForegroundColor Cyan

# --- the four fields -------------------------------------------------------
# Each entry: path, the regex that matches ONLY that file's own version line,
# and the replacement. Every pattern was checked to match exactly once; the
# script re-checks that below rather than trusting it, because a dependency
# line that started with `version = ` would silently corrupt a manifest.
$targets = @(
    @{ Path = 'sustena-core/Cargo.toml';                Pattern = '(?m)^version = "[^"]*"';   Replace = "version = `"$v`"" }
    @{ Path = 'apps/mycelium/src-tauri/Cargo.toml';     Pattern = '(?m)^version = "[^"]*"';   Replace = "version = `"$v`"" }
    @{ Path = 'apps/mycelium/src-tauri/tauri.conf.json'; Pattern = '"version": "[^"]*"';       Replace = "`"version`": `"$v`"" }
    @{ Path = 'apps/mycelium/package.json';             Pattern = '"version": "[^"]*"';       Replace = "`"version`": `"$v`"" }
)

$drift = @()
foreach ($t in $targets) {
    $full = Join-Path $repo $t.Path
    if (-not (Test-Path $full)) { throw "Missing version target: $($t.Path)" }

    $raw = Get-Content $full -Raw
    $found = [regex]::Matches($raw, $t.Pattern)

    # A pattern that stops matching exactly once means the file was restructured.
    # Refuse rather than guess which one is the package's own version.
    if ($found.Count -ne 1) {
        throw "$($t.Path): expected exactly 1 version field, found $($found.Count). Refusing to edit -- the file shape changed and this script needs updating."
    }

    $current = $found[0].Value
    $wanted = $t.Replace

    if ($current -eq $wanted) {
        Write-Host "  ok      $($t.Path)" -ForegroundColor DarkGray
        continue
    }

    if ($Check) {
        Write-Host "  DRIFT   $($t.Path): has [$current], VERSION says [$wanted]" -ForegroundColor Red
        $drift += $t.Path
        continue
    }

    # Preserve the file's existing line endings -- these manifests are CRLF on
    # this machine and a silent conversion would show up as a whole-file diff.
    $updated = [regex]::Replace($raw, $t.Pattern, { param($m) $wanted }, 1)
    [System.IO.File]::WriteAllText($full, $updated)
    Write-Host "  set     $($t.Path)  ->  $v" -ForegroundColor Green
}

if ($Check) {
    if ($drift.Count -gt 0) {
        Write-Host ""
        Write-Host "Version drift in $($drift.Count) file(s). Run: .\scripts\sync-version.ps1" -ForegroundColor Red
        exit 1
    }
    Write-Host "All version fields agree with VERSION ($v)." -ForegroundColor Green
    exit 0
}

Write-Host ""
Write-Host "Synced to $v." -ForegroundColor Green
