param(
    [switch]$Release = $true
)

$ErrorActionPreference = "Stop"

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw "Rust toolchain is required. Install from https://rustup.rs/"
}

if (-not (Get-Command candle.exe -ErrorAction SilentlyContinue)) {
    $wixBins = @(
        "${env:ProgramFiles(x86)}\WiX Toolset v3.14\bin",
        "${env:ProgramFiles(x86)}\WiX Toolset v3.11\bin"
    )
    foreach ($wixBin in $wixBins) {
        if (Test-Path $wixBin) {
            $env:Path = "$wixBin;$env:Path"
            if (-not $env:WIX) {
                $env:WIX = [System.IO.Path]::GetDirectoryName($wixBin)
            }
            break
        }
    }
}

if (-not (Get-Command candle.exe -ErrorAction SilentlyContinue)) {
    throw "WiX Toolset is required. Install with: choco install wixtoolset -y"
}

if (-not (Get-Command cargo-wix -ErrorAction SilentlyContinue)) {
    cargo install cargo-wix --locked
}

$mode = if ($Release) { "" } else { "--debug-build" }

Write-Host "Building Windows MSI installer..."
Push-Location (Resolve-Path (Join-Path $PSScriptRoot ".."))
if (-not (Test-Path .\wix\main.wxs)) {
    cargo wix init
}
cargo wix $mode --nocapture
Pop-Location

Write-Host "Installer created in target\wix."
