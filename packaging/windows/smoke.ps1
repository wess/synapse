[CmdletBinding()]
param([Parameter(Mandatory=$true)][string]$Cli)
$ErrorActionPreference = 'Stop'
$Cli = (Resolve-Path $Cli).Path
$root = Join-Path ([IO.Path]::GetTempPath()) ('synapse' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory $root | Out-Null
$previous = @{}
foreach ($name in @('SYNAPSE_DATA', 'SYNAPSE_HOME', 'SYNAPSE_BIN')) {
    $previous[$name] = [Environment]::GetEnvironmentVariable($name)
}
function Invoke-Cli {
    $output = & $Cli @args
    if ($LASTEXITCODE -ne 0) { throw "CLI failed: $args" }
    return $output
}
try {
    $env:SYNAPSE_DATA = Join-Path $root 'data'
    $env:SYNAPSE_HOME = Join-Path $root 'home'
    $env:SYNAPSE_BIN = Join-Path $root 'bin\synapse.exe'
    Invoke-Cli version
    $check = Invoke-Cli data check --json | ConvertFrom-Json
    if ($check.integrity -ne 'ok') { throw 'Database integrity failed' }
    'Windows installer smoke memory' | & $Cli memory add windows --global | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'Memory write failed' }
    if ((Invoke-Cli memory list 'installer smoke') -notmatch 'Windows installer smoke memory') { throw 'Memory recall failed' }
    Invoke-Cli vault create smoke | Out-Null
    'samplevalue' | & $Cli secret set smoke token SYNAPSE_SMOKE --global | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'Vault write failed' }
    $value = Invoke-Cli run -- cmd.exe /D /C 'echo %SYNAPSE_SMOKE%'
    if ($value.Trim() -ne 'samplevalue') { throw 'Vault injection failed' }
    Invoke-Cli data export (Join-Path $root 'backup.db') | Out-Null
    Invoke-Cli data restore (Join-Path $root 'backup.db') | Out-Null
    Invoke-Cli install | Out-Null
    Invoke-Cli install | Out-Null
    & $env:SYNAPSE_BIN version
    if ($LASTEXITCODE -ne 0) { throw 'Installed CLI failed' }
    Invoke-Cli doctor --json
    Write-Host 'Windows smoke passed: memory, vault injection, export, restore, CLI installation.'
} finally {
    foreach ($name in $previous.Keys) { [Environment]::SetEnvironmentVariable($name, $previous[$name]) }
    # Keep the isolated test directory for diagnosis; it contains only synthetic data.
    Write-Host "Smoke data: $root"
}
