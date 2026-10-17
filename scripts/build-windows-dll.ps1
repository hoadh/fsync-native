# Builds fsync_ffi.dll for each Windows triple and places it, with the generated C#, into the
# Fsync.CoreBindings project (runtimes/<rid>/native/fsync_ffi.dll, picked up by NuGet-style RID
# resolution).
#
#   scripts/build-windows-dll.ps1 [-Rids win-x64,win-arm64]
#
# Needs: rustup, VS 2022 Build Tools (C++ x64 + ARM64), Strawberry Perl + NASM (vendored OpenSSL
# for SQLCipher), clang (ring on ARM64), uniffi-bindgen-cs (see gen-bindings-cs.ps1).
# Honors CARGO_TARGET_DIR.
param(
    [string[]]$Rids = @('win-x64', 'win-arm64')
)
# Native tools write warnings to stderr; failures are detected via $LASTEXITCODE, and
# cmdlets below use -ErrorAction Stop (PS 5.1 would turn redirected stderr into errors).
$ErrorActionPreference = 'Continue'

$Root = Resolve-Path (Join-Path $PSScriptRoot '..')
$Proj = Join-Path $Root 'apps\windows\src\Fsync.CoreBindings'
$BuildDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $Root 'target' }
$Triples = @{ 'win-x64' = 'x86_64-pc-windows-msvc'; 'win-arm64' = 'aarch64-pc-windows-msvc' }

# ring needs clang for aarch64-pc-windows-msvc; the LLVM installer does not add it to PATH.
if ($Rids -contains 'win-arm64' -and -not (Get-Command clang -ErrorAction SilentlyContinue)) {
    $llvm = Join-Path $env:ProgramFiles 'LLVM\bin'
    if (Test-Path (Join-Path $llvm 'clang.exe')) { $env:Path = "$llvm;$env:Path" }
    else { throw 'clang not found (winget install LLVM.LLVM)' }
}

& (Join-Path $PSScriptRoot 'gen-bindings-cs.ps1')
if (-not $?) { throw 'binding generation failed' }

Push-Location $Root
try {
    foreach ($rid in $Rids) {
        $triple = $Triples[$rid]
        if (-not $triple) { throw "unknown RID $rid" }
        rustup target add $triple | Out-Null
        cargo build -p fsync-ffi --release --target $triple
        if ($LASTEXITCODE -ne 0) { throw "cargo build failed for $triple" }
        # Shipped builds must never carry the test-only cargo features. `{f}` prints each crate's
        # enabled features (the default format hides them).
        $tree = cargo tree -e normal,build -p fsync-ffi --target $triple --format '{p} {f}'
        if ($LASTEXITCODE -ne 0) { throw "cargo tree failed for $triple" }
        if ($tree | Select-String -Pattern 'test-relay-ca|test-hooks') { throw "test-only feature enabled in the fsync-ffi release build ($triple)" }

        $dest = Join-Path $Proj "runtimes\$rid\native"
        New-Item -ItemType Directory -Force $dest -ErrorAction Stop | Out-Null
        Copy-Item (Join-Path $BuildDir "$triple\release\fsync_ffi.dll") $dest -Force -ErrorAction Stop
        Write-Host "$rid -> $dest\fsync_ffi.dll"
    }
} finally {
    Pop-Location
}
