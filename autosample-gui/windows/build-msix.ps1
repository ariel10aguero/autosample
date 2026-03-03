param(
    [string]$Configuration = "Release",
    [string]$Version = "",
    [string]$Publisher = "CN=Autosample"
)

$ErrorActionPreference = "Stop"

function Require-Command([string]$Name, [string]$InstallHint) {
    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        throw "$Name not found. $InstallHint"
    }
}

Require-Command cargo "Install Rust from https://rustup.rs/"
Require-Command makeappx.exe "Install Windows SDK (MakeAppx)."

$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$guiRoot = Resolve-Path (Join-Path $scriptDir "..")
$repoRoot = Resolve-Path (Join-Path $guiRoot "..")

Push-Location $repoRoot
try {
    $cargoMode = if ($Configuration -ieq "Release") { "--release" } else { "" }
    Write-Host "Building autosample-gui ($Configuration)..."
    cargo build -p autosample-gui $cargoMode

    $binDir = if ($Configuration -ieq "Release") {
        Join-Path $repoRoot "target\release"
    } else {
        Join-Path $repoRoot "target\debug"
    }
    $exePath = Join-Path $binDir "autosample-gui.exe"
    if (-not (Test-Path $exePath)) {
        throw "Could not find built executable at $exePath"
    }

    if ([string]::IsNullOrWhiteSpace($Version)) {
        $cargoToml = Get-Content (Join-Path $guiRoot "Cargo.toml") -Raw
        if ($cargoToml -match '(?m)^\s*version(?:\.workspace)?\s*=\s*"([^"]+)"\s*$') {
            $Version = $Matches[1]
        } else {
            $rootCargoToml = Get-Content (Join-Path $repoRoot "Cargo.toml") -Raw
            if ($rootCargoToml -match '(?m)^\s*version\s*=\s*"([^"]+)"\s*$') {
                $Version = $Matches[1]
            }
        }
    }
    if ([string]::IsNullOrWhiteSpace($Version)) {
        throw "Could not resolve package version. Pass -Version explicitly (example: 1.2.3.0)."
    }

    # MSIX version format must be Major.Minor.Build.Revision (all numeric)
    if ($Version -match '^\d+\.\d+\.\d+$') {
        $Version = "$Version.0"
    }
    if (-not ($Version -match '^\d+\.\d+\.\d+\.\d+$')) {
        throw "Invalid MSIX version '$Version'. Use numeric form like 1.2.3.0."
    }

    $stageRoot = Join-Path $guiRoot "target\msix\staging"
    $assetsDir = Join-Path $stageRoot "Assets"
    $vfsDir = Join-Path $stageRoot "VFS\ProgramFilesX64\Autosample"

    if (Test-Path $stageRoot) {
        Remove-Item $stageRoot -Recurse -Force
    }

    New-Item -ItemType Directory -Force -Path $assetsDir | Out-Null
    New-Item -ItemType Directory -Force -Path $vfsDir | Out-Null

    Copy-Item $exePath (Join-Path $vfsDir "autosample-gui.exe") -Force

    $logoSource = Join-Path $guiRoot "assets\logo.png"
    if (-not (Test-Path $logoSource)) {
        throw "Missing logo source at $logoSource"
    }

    # Use the same source logo for required MSIX icon slots.
    Copy-Item $logoSource (Join-Path $assetsDir "StoreLogo.png") -Force
    Copy-Item $logoSource (Join-Path $assetsDir "Square44x44Logo.png") -Force
    Copy-Item $logoSource (Join-Path $assetsDir "Square150x150Logo.png") -Force
    Copy-Item $logoSource (Join-Path $assetsDir "Wide310x150Logo.png") -Force

    $manifestTemplatePath = Join-Path $scriptDir "msix\AppxManifest.xml.template"
    if (-not (Test-Path $manifestTemplatePath)) {
        throw "Missing manifest template at $manifestTemplatePath"
    }
    $manifestOutPath = Join-Path $stageRoot "AppxManifest.xml"
    $manifest = Get-Content $manifestTemplatePath -Raw
    $manifest = $manifest.Replace("__VERSION__", $Version)
    $manifest = $manifest.Replace('Publisher="CN=Autosample"', "Publisher=`"$Publisher`"")
    Set-Content -Path $manifestOutPath -Value $manifest -Encoding UTF8

    $packageRoot = Join-Path $guiRoot "target\msix"
    New-Item -ItemType Directory -Force -Path $packageRoot | Out-Null
    $msixPath = Join-Path $packageRoot "Autosample-$Version.msix"

    if (Test-Path $msixPath) {
        Remove-Item $msixPath -Force
    }

    Write-Host "Packing MSIX -> $msixPath"
    makeappx.exe pack /d $stageRoot /p $msixPath /o | Out-Host

    Write-Host ""
    Write-Host "MSIX package created:"
    Write-Host "  $msixPath"
    Write-Host ""
    Write-Host "Note: package signing is not performed by this script."
}
finally {
    Pop-Location
}
