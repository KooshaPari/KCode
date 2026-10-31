# Run in a fresh PowerShell process; requires only rustc, not a workspace build.
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
$source = Join-Path $repo 'crates/jcode-build-meta'
$scratch = Join-Path ([IO.Path]::GetTempPath()) ('kcode-build-meta-' + [Guid]::NewGuid())
New-Item -ItemType Directory -Path (Join-Path $scratch 'crates/jcode-build-meta') -Force | Out-Null
[IO.File]::WriteAllText((Join-Path $scratch 'Cargo.toml'), "[package]`nversion = `"0.93.0-k0.1.0`"`n")
$env:CARGO_MANIFEST_DIR = Join-Path $scratch 'crates/jcode-build-meta'
$env:KCODE_BUILD_GIT_HASH = 'testhash'
$env:KCODE_BUILD_GIT_DATE = 'testdate'
$env:KCODE_BUILD_GIT_DIRTY = 'false'
Remove-Item Env:KCODE_BUILD_SEMVER -ErrorAction SilentlyContinue
Remove-Item Env:KCODE_RELEASE_BUILD -ErrorAction SilentlyContinue
$generator = Join-Path $scratch 'build-meta.exe'
rustc --edition=2024 --test (Join-Path $source 'build.rs') -o (Join-Path $scratch 'build-tests.exe')
if ($LASTEXITCODE -ne 0) { throw 'Build-script tests did not compile' }
& (Join-Path $scratch 'build-tests.exe')
if ($LASTEXITCODE -ne 0) { throw 'Build-script tests failed' }
rustc --edition=2024 (Join-Path $source 'build.rs') -o $generator
if ($LASTEXITCODE -ne 0) { throw 'Generator did not compile' }
$lines = & $generator
if ($LASTEXITCODE -ne 0 -or
    $lines -notcontains 'cargo:rustc-env=KCODE_SEMVER=0.93.0-k0.1.0' -or
    $lines -notcontains 'cargo:rustc-env=KCODE_VERSION=v0.93.0-k0.1.0-dev (testhash)') {
    throw 'Default generated fork identity or dev suffix mismatch'
}
$env:KCODE_BUILD_SEMVER = '0.93.1-k0.2.0'
$override = & $generator
if ($LASTEXITCODE -ne 0 -or
    $override -notcontains 'cargo:rustc-env=KCODE_SEMVER=0.93.1-k0.2.0' -or
    $override -notcontains 'cargo:rustc-env=KCODE_UPDATE_SEMVER=0.93.1-k0.2.0') {
    throw 'Explicit fork override or update identity mismatch'
}
foreach ($bad in @('1.2.3', '1.2.3-', '1.2.3-kbad', '0.0.0-k0.1.0')) {
    $env:KCODE_BUILD_SEMVER = $bad
    $null = & $generator 2>$null
    if ($LASTEXITCODE -eq 0) { throw "Accepted invalid build override: $bad" }
}
foreach ($line in $lines) {
    if ($line -match '^cargo:rustc-env=([^=]+)=(.*)$') {
        [Environment]::SetEnvironmentVariable($Matches[1], $Matches[2], 'Process')
    }
}
# PowerShell removes empty environment values; fixtures keep rustc env! inputs present.
$env:KCODE_GIT_TAG = 'testtag'
$env:KCODE_CHANGELOG = 'testlog'
rustc --edition=2024 --test (Join-Path $source 'src/lib.rs') -o (Join-Path $scratch 'runtime-tests.exe')
if ($LASTEXITCODE -ne 0) { throw 'Runtime tests did not compile' }
& (Join-Path $scratch 'runtime-tests.exe')
if ($LASTEXITCODE -ne 0) { throw 'Runtime tests failed' }
Write-Output "PASS generated identity, explicit override, rejection and runtime tests; receipts: $scratch"
