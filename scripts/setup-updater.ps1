<#
.SYNOPSIS
  Switch on the desktop app's built-in updater. NOT YET RUN -- read this first.

.DESCRIPTION
  WHY THIS IS A SCRIPT AND NOT ALREADY DONE.

  Tauri's updater needs two things that are decisions, not defaults:

    1. A URL where the app looks for "is there a newer version?"
    2. A public signing key, whose private half signs each release.

  Both get baked into every installer. A half-configured updater is worse than
  none: the app would either check a URL that does not exist, or refuse every
  update because the key it was built with is not the key that signed them. So
  the wiring is written and parameterised here, and applied in one step once
  the two answers exist.

  WHY THE SIGNING KEY WAS NOT GENERATED FOR YOU. Generating it takes twenty
  seconds and asks for a password. If a key were generated here, either it
  would have no password, or it would have a password chosen by someone other
  than its owner -- and it would have passed through a session log on the way.
  A signing key should be created by the person who keeps it. The command is
  below; it is one line.

  THE POINT OF NO RETURN. Right now, regenerating the key costs nothing. Once
  an installer built with a public key is in someone's hands, that key is the
  only one their copy will ever trust. Losing the private half at that point
  means no installed copy can ever be updated again -- they would each have to
  be uninstalled and reinstalled by hand. Back it up somewhere real before the
  first release goes out.

.PARAMETER Endpoint
  Where the update manifest lives. For GitHub Releases this is the "latest"
  download URL of a latest.json asset, e.g.

    https://github.com/bonniegachiengu/sustena/releases/latest/download/latest.json

  Tauri substitutes {{target}}, {{arch}} and {{current_version}} if present.

.PARAMETER PublicKey
  The contents of the .pub file printed by `npx tauri signer generate`.
  This one is PUBLIC and belongs in the repository. Its partner does not.

.PARAMETER WhatIf
  Show every change that would be made, change nothing.

.EXAMPLE
  # Step 1 -- Bonnie runs this once, and keeps the private key safe:
  #   cd apps\mycelium
  #   npx tauri signer generate -w $HOME\.sustena\updater.key
  #
  # It prints a public key and writes the private one to that path.
  # $HOME\.sustena\ is OUTSIDE the repository on purpose.
  #
  # Step 2 -- wire it:
  .\scripts\setup-updater.ps1 `
      -Endpoint "https://github.com/bonniegachiengu/sustena/releases/latest/download/latest.json" `
      -PublicKey "dW50cnVzdGVkIGNvbW1lbnQ6..."
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$Endpoint,
    [Parameter(Mandatory = $true)][string]$PublicKey,
    [switch]$WhatIf
)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot

if ($Endpoint -notmatch '^https://') {
    throw "The update endpoint must be https. An update channel over plain http is a way to install someone else's software."
}

Write-Host "This will:" -ForegroundColor Cyan
Write-Host "  1. add tauri-plugin-updater to apps/mycelium/src-tauri/Cargo.toml"
Write-Host "  2. add @tauri-apps/plugin-updater + plugin-process to apps/mycelium"
Write-Host "  3. register the plugin in src-tauri/src/lib.rs"
Write-Host "  4. add plugins.updater to tauri.conf.json (endpoint + pubkey)"
Write-Host "  5. add the updater capability permission"
Write-Host "  6. print what still has to happen at release time"
Write-Host ""
Write-Host "  endpoint : $Endpoint"
Write-Host "  pubkey   : $($PublicKey.Substring(0, [Math]::Min(24, $PublicKey.Length)))..."
Write-Host ""

if ($WhatIf) { Write-Host "-WhatIf: nothing changed." -ForegroundColor Yellow; exit 0 }

# --- 1. Rust dependency -----------------------------------------------------
Push-Location (Join-Path $repo 'apps/mycelium/src-tauri')
cargo add tauri-plugin-updater
if ($LASTEXITCODE -ne 0) { Pop-Location; throw 'cargo add tauri-plugin-updater failed' }
Pop-Location

# --- 2. JS dependencies -----------------------------------------------------
Push-Location (Join-Path $repo 'apps/mycelium')
npm install @tauri-apps/plugin-updater @tauri-apps/plugin-process
if ($LASTEXITCODE -ne 0) { Pop-Location; throw 'npm install of updater plugins failed' }
Pop-Location

# --- 3. Register the plugin -------------------------------------------------
$libPath = Join-Path $repo 'apps/mycelium/src-tauri/src/lib.rs'
$lib = [System.IO.File]::ReadAllText($libPath)
if ($lib -notmatch 'plugin_updater') {
    $anchor = '    tauri::Builder::default()'
    if ($lib -notmatch [regex]::Escape($anchor)) { throw "Could not find the Tauri builder in lib.rs to register the plugin against." }
    $withPlugin = @"
    tauri::Builder::default()
        // The updater only acts when the app calls it. Registering it does not
        // make the app phone home on its own.
        .plugin(tauri_plugin_updater::Builder::new().build())
"@
    $lib = $lib.Replace($anchor, $withPlugin.TrimEnd())
    [System.IO.File]::WriteAllText($libPath, $lib)
    Write-Host "registered the plugin in lib.rs" -ForegroundColor Green
}
else { Write-Host "lib.rs already registers the plugin -- left alone" -ForegroundColor DarkGray }

# --- 4. tauri.conf.json -----------------------------------------------------
$confPath = Join-Path $repo 'apps/mycelium/src-tauri/tauri.conf.json'
$conf = Get-Content $confPath -Raw | ConvertFrom-Json

if (-not $conf.PSObject.Properties.Name.Contains('plugins')) {
    $conf | Add-Member -NotePropertyName plugins -NotePropertyValue ([pscustomobject]@{})
}
$conf.plugins | Add-Member -NotePropertyName updater -NotePropertyValue ([pscustomobject]@{
        endpoints = @($Endpoint)
        pubkey    = $PublicKey
    }) -Force

# ConvertTo-Json reformats the file. That is acceptable here because this runs
# once; the alternative is a fragile regex against nested JSON.
$conf | ConvertTo-Json -Depth 32 | Set-Content $confPath -Encoding utf8
Write-Host "wrote plugins.updater into tauri.conf.json" -ForegroundColor Green

# --- 5/6. what remains ------------------------------------------------------
Write-Host ""
Write-Host "WIRED. What still has to happen, each release:" -ForegroundColor Cyan
Write-Host @"

  1. Sign the installer. Set the private key in the environment before
     building, and Tauri signs the bundle and emits a .sig next to it:

         `$env:TAURI_SIGNING_PRIVATE_KEY = "`$HOME\.sustena\updater.key"
         `$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = "<the password you chose>"

     Set these in your own shell, never in a file in the repository.

  2. Publish a latest.json next to the installer at the endpoint above:

         {
           "version": "0.3.0",
           "notes": "What changed.",
           "pub_date": "2026-08-21T00:00:00Z",
           "platforms": {
             "windows-x86_64": {
               "signature": "<contents of the .sig file>",
               "url": "https://github.com/bonniegachiengu/sustena/releases/download/v0.3.0/Mycelium-Sustena-0.3.0-x64-setup.exe"
             }
           }
         }

  3. Add a "check for updates" control to the app. Nothing checks on its own --
     the plugin acts only when called:

         import { check } from '@tauri-apps/plugin-updater'
         const update = await check()
         if (update) { await update.downloadAndInstall() }

  Then re-run the release script and the two steps above, and an installed copy
  can update itself.

"@ -ForegroundColor Gray
