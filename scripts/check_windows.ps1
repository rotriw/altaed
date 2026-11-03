param(
    [string]$ZedSource,
    [ValidateSet('x86_64', 'aarch64')][string]$Architecture = 'x86_64',
    [switch]$Release,
    [switch]$Offline
)
$ErrorActionPreference = 'Stop'
if ($env:OS -ne 'Windows_NT') { throw 'Run this validation on a Windows host.' }
$projectRoot = Split-Path -Parent $PSScriptRoot
Push-Location $projectRoot
try {
    foreach ($tool in @('cl.exe', 'link.exe', 'lib.exe', 'cmake.exe', 'git.exe', 'python.exe', 'cargo.exe')) {
        if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) {
            throw "Missing $tool. Run from a Visual Studio Developer PowerShell with C++ tools, Windows SDK, CMake, Python and Rust available."
        }
    }
    $prepareArgs = @('scripts/prepare_app.py')
    if ($ZedSource) { $prepareArgs += @('--source', $ZedSource) }
    $manifest = & python -X utf8 @prepareArgs
    if ($LASTEXITCODE -ne 0) { throw 'Pinned editor preparation failed.' }
    $manifest = ($manifest | Select-Object -Last 1).Trim()
    $evidence = Join-Path $projectRoot 'evidence/windows'
    New-Item -ItemType Directory -Force $evidence | Out-Null
    & rustc +1.93 -vV | Out-File (Join-Path $evidence 'host.txt') -Encoding utf8
    if ($LASTEXITCODE -ne 0) { throw 'Install Rust 1.93 and the MSVC build tools first.' }
    $target = "$Architecture-pc-windows-msvc"
    $cargoArgs = @('--manifest-path', $manifest, '--locked', '--target', $target)
    if ($Offline) { $cargoArgs += '--offline' }
    # The IDE uses MSVC; user C++ solutions use the selected clang++/MinGW g++.
    # Tests require a usable C++20 compiler and clangd on this host.
    foreach ($check in @('check', 'test', 'build')) {
        & cargo +1.93 $check --workspace @cargoArgs *> (Join-Path $evidence "$check.log")
        if ($LASTEXITCODE -ne 0) { throw "$check failed; see evidence/windows/$check.log" }
    }
    & cargo +1.93 clippy --workspace @cargoArgs --all-targets --no-deps -- -D warnings *> (Join-Path $evidence 'clippy.log')
    if ($LASTEXITCODE -ne 0) { throw 'Clippy failed; see evidence/windows/clippy.log' }
    & python -X utf8 scripts/verify_clangd.py --output (Join-Path $evidence 'clangd.json')
    if ($LASTEXITCODE -ne 0) { throw 'Real clangd probe failed.' }
    if ($Release) {
        & cargo +1.93 build -p altaed @cargoArgs --release *> (Join-Path $evidence 'release.log')
        if ($LASTEXITCODE -ne 0) { throw 'Release failed; check Windows SDK fxc.exe / GPUI_FXC_PATH and evidence/windows/release.log.' }
    }
} finally {
    Pop-Location
}
