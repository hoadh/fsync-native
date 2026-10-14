# Generates the C# bindings for fsync-ffi (uniffi-bindgen-cs, library mode) into the
# Fsync.CoreBindings project.
#
#   scripts/gen-bindings-cs.ps1 [-OutDir <dir>]
#
# Needs: rustup (MSVC), uniffi-bindgen-cs v0.10.0+v0.29.4 on PATH (ADR 008):
#   cargo install uniffi-bindgen-cs --git https://github.com/NordSecurity/uniffi-bindgen-cs --tag v0.10.0+v0.29.4
# Honors CARGO_TARGET_DIR.
param(
    [string]$OutDir = (Join-Path $PSScriptRoot '..\apps\windows\src\Fsync.CoreBindings\Generated')
)
# Native tools write warnings to stderr; failures are detected via $LASTEXITCODE, and
# cmdlets below use -ErrorAction Stop (PS 5.1 would turn redirected stderr into errors).
$ErrorActionPreference = 'Continue'

$Root = Resolve-Path (Join-Path $PSScriptRoot '..')
$BuildDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $Root 'target' }
$Expected = '0.10.0+v0.29.4'

$version = (& uniffi-bindgen-cs --version) -join ' '
if ($LASTEXITCODE -ne 0) { throw 'uniffi-bindgen-cs not found on PATH' }
if ($version -notmatch [regex]::Escape($Expected)) {
    throw "uniffi-bindgen-cs $Expected required (UniFFI 0.29.4 pin), found: $version"
}

Push-Location $Root
try {
    # The DLL carries the UniFFI metadata that library mode reads.
    cargo build -p fsync-ffi --release
    if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }

    $Lib = Join-Path $BuildDir 'release\fsync_ffi.dll'
    if (-not (Test-Path $Lib)) { throw "missing $Lib" }

    $Tmp = Join-Path ([IO.Path]::GetTempPath()) ([Guid]::NewGuid())
    New-Item -ItemType Directory $Tmp -ErrorAction Stop | Out-Null
    try {
        # [bindings.csharp]: namespace Fsync.Core, public access, FsyncFfi for free functions.
        $Config = Join-Path $Root 'core\fsync-ffi\uniffi.toml'
        & uniffi-bindgen-cs --library $Lib --config $Config --no-format --out-dir $Tmp
        if ($LASTEXITCODE -ne 0) { throw 'uniffi-bindgen-cs failed' }
        New-Item -ItemType Directory -Force $OutDir -ErrorAction Stop | Out-Null
        Copy-Item (Join-Path $Tmp 'fsync_ffi.cs') (Join-Path $OutDir 'fsync_ffi.cs') -Force -ErrorAction Stop
    } finally {
        Remove-Item -Recurse -Force $Tmp
    }
    Write-Host "csharp: $(Join-Path $OutDir 'fsync_ffi.cs')"
} finally {
    Pop-Location
}
