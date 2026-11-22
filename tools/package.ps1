param(
    [string]$Executable = 'target/release/Sonitrelix.exe',
    [string]$FFmpegBuild,
    [string]$WinpthreadsRuntime
)
$ErrorActionPreference = 'Stop'
$workspace = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$binary = (Resolve-Path (Join-Path $workspace $Executable)).Path
if (-not $binary.StartsWith($workspace + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Executable must be inside this workspace.' }
$dist = Join-Path $workspace 'dist'
New-Item -ItemType Directory -Path $dist -Force | Out-Null
$stage = Join-Path $dist ('package-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $stage | Out-Null
$program = Join-Path $stage 'Sonitrelix'
New-Item -ItemType Directory -Path $program | Out-Null
Copy-Item -LiteralPath $binary -Destination (Join-Path $program 'Sonitrelix.exe')
foreach ($name in @('LICENSE','THIRD_PARTY_NOTICES.md','README.md','CHANGELOG.md','BUILDING.md','Cargo.lock')) {
    Copy-Item -LiteralPath (Join-Path $workspace $name) -Destination $program
}
foreach ($folder in @('docs','third-party-licenses')) {
    $to = Join-Path $program $folder
    Copy-Item -LiteralPath (Join-Path $workspace $folder) -Destination $to -Recurse
}
if ($FFmpegBuild) {
    $ffmpeg = (Resolve-Path -LiteralPath $FFmpegBuild).Path
    if (-not $WinpthreadsRuntime) { throw 'Specify the Winpthreads runtime that was used to build FFmpeg.' }
    $runtime = (Resolve-Path -LiteralPath $WinpthreadsRuntime).Path
    if ((Get-FileHash -LiteralPath $runtime -Algorithm SHA256).Hash -ne '4bde68fbb2c6a9a64535928fc505056ebf6f5dc44f82ea2eaac3c714dc22ce79') {
        throw 'Unexpected Winpthreads runtime. See docs/FFMPEG_BUILD.md.'
    }
    Copy-Item -LiteralPath (Join-Path $ffmpeg 'ffmpeg-9.0.2/ffmpeg.exe') -Destination $program
    Copy-Item -LiteralPath $runtime -Destination (Join-Path $program 'libwinpthread-1.dll')
    $licenses = @{
        'FFmpeg-LICENSE.md'   = 'ffmpeg-9.0.2/LICENSE.md'
        'FFmpeg-LGPL-2.1.txt' = 'ffmpeg-9.0.2/COPYING.LGPLv2.1'
        'LAME-LICENSE.txt'    = 'lame-3.100/LICENSE'
        'LAME-COPYING.txt'    = 'lame-3.100/COPYING'
        'Opus-COPYING.txt'    = 'opus-1.6.1/COPYING'
        'Winpthreads-COPYING.txt' = 'mingw-w64-11.0.1/mingw-w64-libraries/winpthreads/COPYING'
    }
    foreach ($name in $licenses.Keys) {
        Copy-Item -LiteralPath (Join-Path $ffmpeg $licenses[$name]) -Destination (Join-Path $program "third-party-licenses/$name")
    }
    $sourceFiles = @(
        (Join-Path $workspace 'tools/build-ffmpeg.sh')
        (Join-Path $workspace 'docs/FFMPEG_BUILD.md')
    )
    $sources = Get-Content -LiteralPath (Join-Path $workspace 'docs/FFMPEG_BUILD.md') |
        Select-String '^([0-9a-f]{64})  ([a-zA-Z0-9._-]+)$'
    if ($sources.Count -ne 4) { throw 'Expected four corresponding source archives.' }
    foreach ($source in $sources) {
        $path = Join-Path $ffmpeg $source.Matches[0].Groups[2].Value
        $expected = $source.Matches[0].Groups[1].Value
        if ((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash -ne $expected) { throw 'FFmpeg source archive hash mismatch.' }
        $sourceFiles += $path
    }
    Compress-Archive -LiteralPath $sourceFiles -DestinationPath (Join-Path $dist 'FFmpeg-corresponding-source.zip') -Force
}
# Only the release CRT from Visual Studio's redistributable list, never debug_nonredist.
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio/Installer/vswhere.exe'
if (-not (Test-Path -LiteralPath $vswhere)) { throw 'Visual Studio redistributable lookup tool was not found.' }
$runtimes = & $vswhere -latest -products '*' -find 'VC\Redist\MSVC\*\x64\Microsoft.VC*.CRT\vcruntime140*.dll'
if ($LASTEXITCODE -ne 0 -or -not $runtimes) { throw 'Install the x64 Visual C++ release redistributable build component before packaging.' }
foreach ($runtime in $runtimes) {
    if ($runtime -match 'debug_nonredist') { throw 'Debug runtime distribution is forbidden.' }
    Copy-Item -LiteralPath $runtime -Destination $program -Force
}
[IO.File]::WriteAllText((Join-Path $program '.sonitrelix-portable'), 'Sonitrelix portable format 1')
$unsafe = Get-ChildItem -LiteralPath $program -Recurse -File | Where-Object { $_.Extension -in @('.pem','.key') -or $_.FullName -match '[\\/]data[\\/]' }
if ($unsafe) { throw 'Refusing to package private keys or user data.' }
# Cargo license files can carry Unix-epoch dates outside ZIP's supported range.
# Normalize only the fresh distribution copies, preserving original source files.
Get-ChildItem -LiteralPath $program -Recurse -File -Force | ForEach-Object { $_.LastWriteTime = [datetime]'2000-01-01T12:00:00' }
$archive = Join-Path $dist 'Sonitrelix-windows-x64.zip'
# The stage is fresh and contains only allowlisted program assets, never an existing data/ folder.
Compress-Archive -LiteralPath $program -DestinationPath $archive -Force
$hash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
[IO.File]::WriteAllText((Join-Path $dist 'SHA256SUMS.txt'), "$hash  Sonitrelix-windows-x64.zip" + [Environment]::NewLine)
Write-Output "Portable program: $program"
Write-Output "Archive: $archive"
