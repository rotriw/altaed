param([Parameter(Mandatory)][string]$Executable)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$evidence = Join-Path $projectRoot 'evidence/windows'
New-Item -ItemType Directory -Force $evidence | Out-Null
$previousData = $env:ALTAED_DATA_DIR
$env:ALTAED_DATA_DIR = Join-Path ([IO.Path]::GetTempPath()) ('altaed-startup-' + [guid]::NewGuid())
$process = $null
try {
    $process = Start-Process -FilePath (Resolve-Path $Executable) -PassThru -RedirectStandardError (Join-Path $evidence 'portable-startup.log')
    Start-Sleep -Seconds 5
    $process.Refresh()
    if ($process.HasExited) { throw "Portable GUI exited during startup with code $($process.ExitCode)." }
    @{
        status = 'process-alive-after-startup'
        scope = 'Hosted Windows startup only; no desktop interaction validation'
        dataDirectory = 'isolated temporary directory'
    } | ConvertTo-Json | Set-Content -Encoding utf8 (Join-Path $evidence 'portable-startup.json')
} finally {
    # This is only the process created above, with a new empty data directory on
    # the disposable CI runner. No user's IDE process or documents are touched.
    if ($process -and -not $process.HasExited) { Stop-Process -Id $process.Id -Force }
    $env:ALTAED_DATA_DIR = $previousData
}
