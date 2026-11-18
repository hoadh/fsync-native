# Builds the downloadable Windows release: fresh fsync_ffi.dll (x64 + ARM64), the Windows tests, then
# one self-contained zip per RID with SHA-256 hashes.
#
#   scripts/build-windows-release.ps1 [-OutDir <dir>] [-SkipTests]
#
# Output: <OutDir>\fsync-windows-win-x64.zip and fsync-windows-win-arm64.zip (default OutDir:
# <repo>\dist\windows). Needs everything scripts/build-windows-dll.ps1 needs, plus the .NET 8 SDK.
# Run it from a clean checkout of the commit you release. Works over ssh (Windows PowerShell 5.1).
param(
    [string]$OutDir = (Join-Path $PSScriptRoot '..\dist\windows'),
    [switch]$SkipTests
)
$ErrorActionPreference = 'Continue'
$ProgressPreference = 'SilentlyContinue'
$Root = Resolve-Path (Join-Path $PSScriptRoot '..')
Set-Location $Root

if (git status --porcelain) { throw 'the tree is dirty; release from a clean checkout' }
"building $(git rev-parse --short HEAD)"

# A failed cargo run inside build-windows-dll.ps1 leaves the old DLL in place and the publish would
# still succeed, so require both DLLs to be newer than this run.
$start = Get-Date
& (Join-Path $PSScriptRoot 'build-windows-dll.ps1')
$dlls = Get-ChildItem (Join-Path $Root 'apps\windows\src\Fsync.CoreBindings\runtimes\*\native\fsync_ffi.dll')
if (@($dlls | Where-Object { $_.LastWriteTime -gt $start }).Count -ne 2) {
    throw 'fsync_ffi.dll was not rebuilt (stale DLL); see the cargo output above'
}

# Regenerated bindings may differ only in line endings; a content change means the committed C# is stale.
if (git diff --ignore-cr-at-eol --name-only -- apps/windows/src/Fsync.CoreBindings/Generated) {
    throw 'Generated/fsync_ffi.cs differs from the commit; regenerate and commit it first'
}

if (-not $SkipTests) {
    foreach ($t in @('Fsync.CoreBindings.Tests', 'Fsync.App.Tests')) {
        dotnet test "apps\windows\tests\$t" -c Release
        if ($LASTEXITCODE -ne 0) { throw "$t failed" }
    }
}

Remove-Item -Recurse -Force $OutDir -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $OutDir -ErrorAction Stop | Out-Null
foreach ($p in @(@('x64', 'win-x64'), @('ARM64', 'win-arm64'))) {
    $dir = Join-Path $OutDir "fsync-windows-$($p[1])"
    dotnet publish apps\windows\src\Fsync.App -c Release "-p:Platform=$($p[0])" -r $p[1] --self-contained true -o $dir
    if ($LASTEXITCODE -ne 0) { throw "publish $($p[1]) failed" }
    Compress-Archive -Path "$dir\*" -DestinationPath "$dir.zip" -Force -ErrorAction Stop
    Remove-Item -Recurse -Force $dir
}
Get-ChildItem $OutDir -Filter *.zip | ForEach-Object {
    '{0}  {1}' -f (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLower(), $_.Name
}
