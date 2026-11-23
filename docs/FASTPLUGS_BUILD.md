# Build a Windows package with FastPlugs

The release asset `FastPlugs-corresponding-source.zip` contains the Sonitrelix workspace and locked Rust dependency sources, FastPlugs and the Windows sources used by its build. Extract it to a short writable path. Install Visual Studio 2022 x64 C++ Build Tools with the Windows SDK, CMake 3.24 or later, stable Rust MSVC and Perl.

From the extracted source folder in PowerShell:

```powershell
$src = (Resolve-Path .).Path
cmake -S FastPlugs -B build -G 'Visual Studio 17 2022' -A x64 `
    "-DFASTROUTE_SOURCE_DIR=$src/deps/fastroute" `
    "-DFASTPLAY_SOURCE_DIR=$src/deps/fastplay" `
    "-DFETCHCONTENT_SOURCE_DIR_RNNOISE=$src/deps/rnnoise" `
    "-DFETCHCONTENT_SOURCE_DIR_SPEEXDSP=$src/deps/speexdsp" `
    "-DFETCHCONTENT_SOURCE_DIR_SIGNALSMITH_STRETCH=$src/deps/signalsmith_stretch" `
    "-DFETCHCONTENT_SOURCE_DIR_CLAP-WRAPPER=$src/deps/clap-wrapper" `
    "-DCLAP_SDK_ROOT=$src/deps/clap" "-DVST3_SDK_ROOT=$src/deps/vst3sdk"
cmake --build build --config Release --target fastplugs_clap fastplugs_smoke --parallel
ctest --test-dir build -C Release --output-on-failure
Set-Location Sonitrelix
.\tools\build-release.ps1 -FastPlugsClap ../build/out/CLAP/Release/FastPlugs.clap
.\target\release\Sonitrelix.exe
```

The `-FastPlugsClap` argument also produces a preview executable with the bundle. Open **Effects > FastPlugs** to test it. Sonitrelix stores a copy of its embedded plug-in under `data/effects` when used. The signed EXE updater delivers the same bundle. A build without the argument can use an independently installed CLAP bundle.

To prepare a source asset for publication, configure and build the upstream [FastPlugs revision](https://github.com/masonasons/FastPlugs/tree/11154c19e3d33ac92d36f88fc36826ff3980e78d) with its default pinned Windows dependencies and the CLAP and smoke targets above. From the committed Sonitrelix workspace, run `python tools/package-fastplugs-source.py <FastPlugs checkout> <CMake build folder> <source ZIP>`. Pass that ZIP to `tools/package.ps1 -FastPlugsSource <source ZIP>` and publish it with the Windows ZIP and standalone EXE. See [third-party notices](THIRD_PARTY_NOTICES.md).
