# SideKit installer for Windows — no Rust toolchain or admin rights required.
#
#   irm https://raw.githubusercontent.com/pratik-chakravorty/sidekit/master/install.ps1 | iex
#
# Options via environment variables (set them before running the line above):
#   $env:SIDEKIT_VERSION = "v0.1.0"   install a specific release (default: latest)
#   $env:SIDEKIT_UNINSTALL = "1"      remove SideKit
#   $env:SIDEKIT_ARCHIVE = "C:\...\sidekit-windows-x86_64.zip"   install from a local zip
#   $env:SIDEKIT_NO_PATH = "1"        don't add SideKit to your user PATH
# The same options work as parameters when the script is saved and run as a file:
#   .\install.ps1 -Version v0.1.0 | -Uninstall | -Archive <zip> | -NoPath

& {
  param(
    [string]$Version = $(if ($env:SIDEKIT_VERSION) { $env:SIDEKIT_VERSION } else { "latest" }),
    [switch]$Uninstall = ($env:SIDEKIT_UNINSTALL -eq "1"),
    [string]$Archive = $env:SIDEKIT_ARCHIVE,
    [switch]$NoPath = ($env:SIDEKIT_NO_PATH -eq "1")
  )

  $ErrorActionPreference = "Stop"
  $ProgressPreference = "SilentlyContinue"   # Invoke-WebRequest is much faster without the progress bar
  [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

  $Repo = "pratik-chakravorty/sidekit"
  $Asset = "sidekit-windows-x86_64.zip"      # also runs on Windows on ARM through x64 emulation
  $InstallDir = Join-Path $env:LOCALAPPDATA "Programs\SideKit"
  $Exe = Join-Path $InstallDir "sidekit.exe"
  $Shortcut = Join-Path ([Environment]::GetFolderPath("Programs")) "SideKit.lnk"
  $UninstallKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\SideKit"

  function Say($msg) { Write-Host "==> $msg" -ForegroundColor Cyan }

  function Set-UserPath([scriptblock]$edit) {
    $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey("Environment", $true)
    # Read the raw value so %VARIABLES% in the user PATH are preserved.
    $current = $key.GetValue("Path", "", [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
    $parts = @($current -split ";" | Where-Object { $_ -ne "" })
    $updated = (& $edit $parts) -join ";"
    if ($updated -ne $current) {
      # Keep the value's existing registry type (normally REG_EXPAND_SZ).
      $kind = if ($key.GetValueNames() -contains "Path") { $key.GetValueKind("Path") } else { [Microsoft.Win32.RegistryValueKind]::ExpandString }
      $key.SetValue("Path", $updated, $kind)
      # Tell Explorer and new terminals about the change.
      [Environment]::SetEnvironmentVariable("SIDEKIT_PATH_REFRESH", "1", "User")
      [Environment]::SetEnvironmentVariable("SIDEKIT_PATH_REFRESH", $null, "User")
    }
    $key.Close()
  }

  function Stop-SideKit {
    Get-Process sidekit -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $Exe } | ForEach-Object {
      Say "Closing the running SideKit"
      $_.CloseMainWindow() | Out-Null
      if (-not $_.WaitForExit(3000)) { $_ | Stop-Process -Force }
    }
  }

  function Remove-SideKit {
    Stop-SideKit
    if (Test-Path $Shortcut) { Remove-Item $Shortcut -Force }
    if (Test-Path $UninstallKey) { Remove-Item $UninstallKey -Recurse -Force }
    Set-UserPath { param($p) $p | Where-Object { $_.TrimEnd("\") -ne $InstallDir } }
    if (Test-Path $InstallDir) { Remove-Item $InstallDir -Recurse -Force }
    Say "SideKit removed. Settings are kept in $env:APPDATA\SideKit (delete it to reset)."
  }

  if ($Uninstall) { Remove-SideKit; return }

  $tmp = Join-Path ([IO.Path]::GetTempPath()) ("sidekit-" + [guid]::NewGuid())
  New-Item -ItemType Directory $tmp | Out-Null
  try {
    $zip = Join-Path $tmp $Asset
    if ($Archive) {
      if (-not (Test-Path $Archive)) { throw "archive not found: $Archive" }
      Copy-Item $Archive $zip
      Say "Installing SideKit from $Archive"
    } else {
      if ($Version -eq "latest") {
        $base = "https://github.com/$Repo/releases/latest/download"
      } else {
        if (-not $Version.StartsWith("v")) { $Version = "v$Version" }
        $base = "https://github.com/$Repo/releases/download/$Version"
      }
      Say "Downloading SideKit ($Version)"
      Invoke-WebRequest "$base/$Asset" -OutFile $zip -UseBasicParsing

      try {
        $sums = (Invoke-WebRequest "$base/SHA256SUMS" -UseBasicParsing).Content
        if ($sums -is [byte[]]) { $sums = [Text.Encoding]::UTF8.GetString($sums) }
        $line = ($sums -split "`n") | Where-Object { $_ -match "\s$([regex]::Escape($Asset))\s*$" } | Select-Object -First 1
        if ($line) {
          $expected = ($line -split "\s+")[0].ToLower()
          $actual = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLower()
          if ($expected -ne $actual) { throw "checksum mismatch for $Asset" }
          Say "Checksum verified"
        }
      } catch [System.Net.WebException] { }
    }

    Expand-Archive $zip -DestinationPath (Join-Path $tmp "x") -Force
    $newExe = Get-ChildItem (Join-Path $tmp "x") -Filter sidekit.exe -Recurse | Select-Object -First 1
    if (-not $newExe) { throw "sidekit.exe missing from the archive" }

    Stop-SideKit
    New-Item -ItemType Directory -Force $InstallDir | Out-Null
    Copy-Item $newExe.FullName $Exe -Force

    # Self-contained uninstaller next to the app, used by Settings → Apps.
    $uninstaller = Join-Path $InstallDir "uninstall.ps1"
    @"
# Removes SideKit (written by install.ps1). Settings in %APPDATA%\SideKit are kept.
`$InstallDir = '$InstallDir'
Get-Process sidekit -ErrorAction SilentlyContinue | Where-Object { `$_.Path -eq (Join-Path `$InstallDir 'sidekit.exe') } | Stop-Process -Force
Remove-Item '$Shortcut' -Force -ErrorAction SilentlyContinue
Remove-Item '$UninstallKey' -Recurse -Force -ErrorAction SilentlyContinue
`$key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', `$true)
`$cur = `$key.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
`$new = (@(`$cur -split ';' | Where-Object { `$_ -ne '' -and `$_.TrimEnd('\') -ne `$InstallDir }) -join ';')
if (`$new -ne `$cur) { `$key.SetValue('Path', `$new, `$key.GetValueKind('Path')) }
`$key.Close()
# This script lives inside the folder, so delete the folder from a separate process.
Start-Process powershell -WindowStyle Hidden -ArgumentList '-NoProfile', '-Command', "Start-Sleep 1; Remove-Item -Recurse -Force '`$InstallDir'"
Write-Host 'SideKit removed.'
"@ | Set-Content -Path $uninstaller -Encoding UTF8

    # Start menu shortcut.
    $shell = New-Object -ComObject WScript.Shell
    $lnk = $shell.CreateShortcut($Shortcut)
    $lnk.TargetPath = $Exe
    $lnk.WorkingDirectory = $InstallDir
    $lnk.IconLocation = "$Exe,0"
    $lnk.Description = "SideKit developer toolbox"
    $lnk.Save()

    # Settings → Apps → Installed apps entry.
    $ver = if ($Version -eq "latest" -or $Archive) { (Get-Item $Exe).VersionInfo.ProductVersion } else { $Version.TrimStart("v") }
    New-Item -Path $UninstallKey -Force | Out-Null
    $props = @{
      DisplayName = "SideKit"; DisplayIcon = "$Exe,0"; Publisher = "SideKit"
      DisplayVersion = "$ver"; InstallLocation = $InstallDir; NoModify = 1; NoRepair = 1
      UninstallString = "powershell.exe -NoProfile -ExecutionPolicy Bypass -File `"$uninstaller`""
      EstimatedSize = [int]((Get-Item $Exe).Length / 1KB)
    }
    foreach ($k in $props.Keys) { Set-ItemProperty -Path $UninstallKey -Name $k -Value $props[$k] }

    if (-not $NoPath) {
      Set-UserPath { param($p) if ($p | Where-Object { $_.TrimEnd("\") -eq $InstallDir }) { $p } else { $p + $InstallDir } }
    }

    Say "Installed SideKit to $InstallDir"
    Say "Launch it from the Start menu, or run 'sidekit' in a new terminal."
  } finally {
    Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
  }
} @args
