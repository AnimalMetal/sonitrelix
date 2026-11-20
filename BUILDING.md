# Building on Windows

Target: Windows 11 x64; Windows 10 x64 (1607 or later API baseline) is intended but not separately verified.

Install the stable Rust MSVC toolchain and Visual Studio Build Tools with Desktop development with C++ and a Windows SDK. No Python, wxPython, CMake, or audio SDK is needed for this build. The initial verified compiler was rustc 1.99.1, Cargo 1.99.1, x86_64-pc-windows-msvc. Use that compiler and the committed Cargo.lock to reproduce the dependency graph; byte-identical binaries are not guaranteed.

From the repository directory:

```powershell
rustup default stable-x86_64-pc-windows-msvc
cargo fetch --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo build --release --workspace --locked
.\target\release\Sonitrelix.exe
```

For development:

```powershell
cargo run --locked -p sonitrelix-windows-ui --bin Sonitrelix
```

The executable is `target\release\Sonitrelix.exe`. JSON language resources are embedded; there are no separate runtime assets. Version 0.4 also stores captured PCM16 WAV assets in data/recordings; preserve these files with your projects. The application creates `data\preferences.json` and `data\session.jsonl` beside its executable. Run from a writable portable folder. Projects are saved wherever the user chooses.

Preferences now retain `preferences.json.bak` and timestamped corrupt copies when recovery is necessary. Existing 0.1 preferences migrate without changing their path. The native tabbed dialog and read-only Core Audio discovery use system DLLs; no extra runtime or codec is installed.

The regular tests use temporary settings and synthetic device lists. A separate optional read-only check exercises the local Windows audio service without opening a stream:

```powershell
cargo test -p sonitrelix-devices --locked -- --ignored --nocapture
```

Version 0.3 adds WASAPI playback through the Microsoft windows-rs bindings and an original bounded WAV decoder. No CPAL, Symphonia, Hound, Rubato, ASIO SDK or external codec binary is required by this build. The engine integration checks open a real output with short silent fixtures and test a nonexistent input endpoint without microphone capture:

```powershell
cargo test -p sonitrelix-engine --locked -- --ignored --nocapture
```

After building, Ctrl+I imports a WAV; Ctrl+O opens a project. Supported formats and manual tests are documented in docs/EDITING_RECORDING.md and docs/SHORTCUTS.md. A release build overwrites target\release\Sonitrelix.exe; close a running instance first so Windows can replace the file.

This environment has Cargo configured offline and its network proxy is unreachable. All dependencies in Cargo.lock were available in the local cache. On a connected machine with the same offline setting, use `cargo --config net.offline=false fetch --locked` first. Do not change system proxy settings to build.

Before redistributing a binary, include LICENSE, THIRD_PARTY_NOTICES.md, the third-party-license directory, the matching source and Cargo.lock. An installer and code signing have not been implemented.
