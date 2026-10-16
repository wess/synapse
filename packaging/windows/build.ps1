[CmdletBinding()]
param([switch]$NoBuild)
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path "$PSScriptRoot\..\..").Path
$target = 'x86_64-pc-windows-msvc'
$version = (Select-String -Path "$root\crates\synapse\Cargo.toml" -Pattern '^version = "([^"]+)"').Matches[0].Groups[1].Value
$dist = Join-Path $root "dist\$version"
$stage = Join-Path $dist 'stage'
$targetdir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $root 'target' }
if (-not $NoBuild) {
    $previous = @{}
    $defaults = @{
        CARGO_PROFILE_RELEASE_LTO = 'thin'
        CARGO_PROFILE_RELEASE_CODEGEN_UNITS = '1'
        CARGO_PROFILE_RELEASE_STRIP = 'symbols'
    }
    foreach ($name in @('RUSTFLAGS') + @($defaults.Keys)) {
        $previous[$name] = [Environment]::GetEnvironmentVariable($name)
    }
    try {
        if ($env:RUSTFLAGS -notlike '*target-feature=+crt-static*') {
            $env:RUSTFLAGS = "$env:RUSTFLAGS -C target-feature=+crt-static".Trim()
        }
        foreach ($name in $defaults.Keys) {
            if (-not [Environment]::GetEnvironmentVariable($name)) {
                [Environment]::SetEnvironmentVariable($name, $defaults[$name])
            }
        }
        & cargo build --release --locked --target $target --manifest-path "$root\Cargo.toml"
        if ($LASTEXITCODE -ne 0) { throw 'Desktop build failed' }
        & cargo build --release --locked --target $target --manifest-path "$root\crates\synapsecore\Cargo.toml"
        if ($LASTEXITCODE -ne 0) { throw 'CLI build failed' }
    } finally {
        foreach ($name in $previous.Keys) {
            [Environment]::SetEnvironmentVariable($name, $previous[$name])
        }
    }
}
New-Item -ItemType Directory -Force $stage | Out-Null
Copy-Item "$targetdir\$target\release\synapse.exe" "$stage\synapsedesktop.exe" -Force
# The core workspace uses its own target directory unless CARGO_TARGET_DIR is set.
$coretarget = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { "$root\crates\synapsecore\target" }
Copy-Item "$coretarget\$target\release\synapse-cli.exe" "$stage\synapse.exe" -Force
$iscc = (Get-Command ISCC.exe -ErrorAction SilentlyContinue).Source
if (-not $iscc) { $iscc = "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe" }
if (-not (Test-Path $iscc)) { throw 'Install Inno Setup 6, or put ISCC.exe on PATH' }
& $iscc "/DAppVersion=$version" "/DStage=$stage" "/DOutput=$dist" "$PSScriptRoot\synapse.iss"
if ($LASTEXITCODE -ne 0) { throw 'Installer compilation failed' }
$setup = Join-Path $dist "synapse.$version.windows.x64.exe"
$hash = (Get-FileHash $setup -Algorithm SHA256).Hash.ToLowerInvariant()
[IO.File]::WriteAllText((Join-Path $dist 'sha256sums.txt'), "$hash  $(Split-Path $setup -Leaf)`n", [Text.Encoding]::ASCII)
Write-Host $setup
