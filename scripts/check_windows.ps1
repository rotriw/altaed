param(
    [string]$ZedSource,
    [ValidateSet('x86_64', 'aarch64')][string]$Architecture = 'x86_64',
    [ValidateSet('all', 'prepare', 'check', 'test', 'build', 'clippy', 'probe', 'release')][string]$Stage = 'all',
    [switch]$Release,
    [switch]$Offline
)
$ErrorActionPreference = 'Stop'
if ($env:OS -ne 'Windows_NT') { throw 'Run this validation on a Windows host.' }

# Each CI stage starts a fresh shell. Discover MSVC instead of depending on the
# caller's shell or a development-machine installation directory.
if (-not (Get-Command cl.exe -ErrorAction SilentlyContinue)) {
    $vswhere = Join-Path ([Environment]::GetFolderPath('ProgramFilesX86')) 'Microsoft Visual Studio/Installer/vswhere.exe'
    if (-not (Test-Path -LiteralPath $vswhere)) { throw 'Install MSVC Build Tools with the Windows SDK first.' }
    $installation = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
    if ($LASTEXITCODE -ne 0 -or -not $installation) { throw 'MSVC tools are unavailable.' }
    Import-Module (Join-Path $installation 'Common7/Tools/Microsoft.VisualStudio.DevShell.dll')
    $targetArch = if ($Architecture -eq 'x86_64') { 'x64' } else { 'arm64' }
    Enter-VsDevShell -VsInstallPath $installation -SkipAutomaticLocation -DevCmdArguments "-arch=$targetArch -host_arch=x64"
}
$llvmBin = Join-Path ([Environment]::GetFolderPath('ProgramFiles')) 'LLVM/bin'
if (Test-Path -LiteralPath $llvmBin) { $env:PATH = $llvmBin + ';' + $env:PATH }

$projectRoot = Split-Path -Parent $PSScriptRoot
Push-Location $projectRoot
try {
    foreach ($tool in @('cl.exe', 'link.exe', 'lib.exe', 'cmake.exe', 'git.exe', 'python.exe', 'cargo.exe')) {
        if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) { throw "Missing $tool in the Windows build environment." }
    }
    $evidence = Join-Path $projectRoot 'evidence/windows'
    New-Item -ItemType Directory -Force $evidence | Out-Null
    if ($Stage -in @('all', 'prepare')) {
        $prepareArgs = @('scripts/prepare_app.py')
        if ($ZedSource) { $prepareArgs += @('--source', $ZedSource) }
        & python -X utf8 @prepareArgs
        if ($LASTEXITCODE -ne 0) { throw 'Pinned editor preparation failed.' }
        & rustc +1.93 -vV | Tee-Object -FilePath (Join-Path $evidence 'host.txt')
        if ($LASTEXITCODE -ne 0) { throw 'Install Rust 1.93 and its MSVC standard library.' }
    }
    $target = "$Architecture-pc-windows-msvc"
    $cargoArgs = @('--manifest-path', (Join-Path $projectRoot 'Cargo.toml'), '--locked', '--target', $target)
    if ($Offline) { $cargoArgs += '--offline' }
    foreach ($check in @('check', 'test', 'build', 'clippy')) {
        if ($Stage -notin @('all', $check)) { continue }
        Write-Host "::group::cargo $check ($target)"
        $commandArgs = @('+1.93', $check, '--workspace') + $cargoArgs
        if ($check -eq 'clippy') { $commandArgs += @('--all-targets', '--no-deps', '--', '-D', 'warnings') }
        & cargo @commandArgs 2>&1 | Tee-Object -FilePath (Join-Path $evidence "$check.log")
        $exitCode = $LASTEXITCODE
        Write-Host '::endgroup::'
        if ($exitCode -ne 0) { throw "$check failed; see evidence/windows/$check.log" }
    }
    if ($Stage -in @('all', 'probe')) {
        & python -X utf8 scripts/verify_clangd.py --output (Join-Path $evidence 'clangd.json')
        if ($LASTEXITCODE -ne 0) { throw 'Real clangd probe failed.' }
    }
    if ($Stage -eq 'release' -or ($Stage -eq 'all' -and $Release)) {
        & cargo +1.93 build -p altaed @cargoArgs --release 2>&1 | Tee-Object -FilePath (Join-Path $evidence 'release.log')
        if ($LASTEXITCODE -ne 0) { throw 'Release failed; inspect release.log and Windows SDK fxc.exe / GPUI_FXC_PATH.' }
    }
} finally {
    Pop-Location
}
