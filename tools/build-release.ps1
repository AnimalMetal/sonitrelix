param([string]$FastPlugsClap)

$ErrorActionPreference = 'Stop'
$workspace = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if ($FastPlugsClap) {
    $FastPlugsClap = (Resolve-Path -LiteralPath $FastPlugsClap).Path
}
Push-Location $workspace
$previous = @{}
foreach ($name in @('OPENSSL_DIR','OPENSSL_STATIC','OPENSSL_NO_VENDOR','CARGO_ENCODED_RUSTFLAGS','SONITRELIX_FASTPLUGS_CLAP')) { $previous[$name] = [Environment]::GetEnvironmentVariable($name,'Process') }
try {
    if ($FastPlugsClap) {
        $env:SONITRELIX_FASTPLUGS_CLAP = $FastPlugsClap
    }
    $lock = [IO.File]::ReadAllText((Join-Path $workspace 'Cargo.lock'))
    $match = [regex]::Match($lock, 'name = "openssl-src"\r?\nversion = "[^+]+\+([^"]+)"')
    if (-not $match.Success) { throw 'Cannot identify the locked native OpenSSL version.' }
    $expected = $match.Groups[1].Value
    $cargoTarget = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { 'target' }
    function Find-NativeLibrary {
        Get-ChildItem (Join-Path $cargoTarget 'debug/build/openssl-sys-*/out/openssl-build/install/lib/libcrypto.lib') -ErrorAction SilentlyContinue |
            Where-Object {
                $header = Join-Path $_.Directory.Parent.FullName 'include/openssl/opensslv.h'
                (Test-Path -LiteralPath $header) -and ([IO.File]::ReadAllText($header) -match ('OPENSSL_VERSION_STR\s+"' + [regex]::Escape($expected) + '"'))
            } | Sort-Object LastWriteTime -Descending | Select-Object -First 1
    }
    $library = Find-NativeLibrary
    if (-not $library) {
        Remove-Item Env:OPENSSL_DIR,Env:OPENSSL_STATIC,Env:OPENSSL_NO_VENDOR -ErrorAction SilentlyContinue
        cargo check -p sonitrelix-updater --locked
        if ($LASTEXITCODE) { exit $LASTEXITCODE }
        $library = Find-NativeLibrary
    }
    if (-not $library) { throw 'The locked static OpenSSL library was not built.' }
    $env:OPENSSL_DIR = $library.Directory.Parent.FullName
    $env:OPENSSL_STATIC = '1'
    $env:OPENSSL_NO_VENDOR = '1'
    $rustFlags = @(
        "--remap-path-prefix=$workspace=/sonitrelix"
        "--remap-path-prefix=$($env:USERPROFILE)=/build-user"
    )
    $separator = [string][char]31
    if ($env:CARGO_ENCODED_RUSTFLAGS) { $rustFlags = @($env:CARGO_ENCODED_RUSTFLAGS.Split([char]31)) + $rustFlags }
    elseif ($env:RUSTFLAGS) { $rustFlags = @($env:RUSTFLAGS -split '\s+' | Where-Object { $_ }) + $rustFlags }
    $env:CARGO_ENCODED_RUSTFLAGS = $rustFlags -join $separator
    cargo build --release --workspace --locked
    if ($LASTEXITCODE) { exit $LASTEXITCODE }
} finally {
    foreach ($name in $previous.Keys) { [Environment]::SetEnvironmentVariable($name,$previous[$name],'Process') }
    Pop-Location
}
