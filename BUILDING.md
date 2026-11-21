# Building on Windows

Install stable Rust MSVC, Visual Studio C++ Build Tools with Windows SDK/release CRT redistributables, and Perl on PATH for vendored OpenSSL. Python 3.11+ runs the documentation generator/check; cryptography is only needed by the offline release-signing owner. Neither is needed to run Sonitrelix or read its guide. Windows 11 x64 is the target; Windows 10 remains unverified. Verified compiler: Rust/Cargo 1.99.1. Use Cargo.lock and --locked; byte-identical output is not guaranteed. Initial OpenSSL compilation can be lengthy.

```powershell
cargo fetch --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
.\tools\build-release.ps1
.\target\release\Sonitrelix.exe
.\tools\package.ps1
```

The release helper reuses a matching locked static OpenSSL build from debug validation. Conventional cargo build --release --workspace --locked also works but may rebuild native OpenSSL. Close the executable before rebuilding.

For public releases set `CARGO_TARGET_DIR` to a directory whose absolute path contains no personal information. The helper remaps Rust source paths, while native OpenSSL can embed its build directory. Check both binaries for personal paths before packaging. See [GitHub publication](docs/GITHUB.md) and [building the bundled FFmpeg](docs/FFMPEG_BUILD.md).

For development use cargo run --locked -p sonitrelix-windows-ui --bin Sonitrelix. Resources are embedded. A writable portable folder holds data beside the executable. Preserve imports/recordings with projects. Packaging creates a fresh allowlisted folder under dist, excludes data/private keys and includes only release vcruntime140 DLLs from the Microsoft redistributable directory under Microsoft terms. The portable marker enables EXE installation. Future runtime/asset changes require a full package.

Optional hardware checks (endpoint discovery, output silence and invalid input; no microphone capture):

```powershell
cargo test -p sonitrelix-devices -p sonitrelix-engine --locked -- --ignored --nocapture
```

Media Foundation codecs require Windows media components, including Media Feature Pack on relevant N editions. No wxPython, ASIO SDK or external codec binary is bundled. Include LICENSE, notices and dependency licenses. See docs/UPDATES.md for owner publication. Authenticode signing and an installer remain unavailable.

This environment uses cached dependencies with an unreachable proxy. On a connected machine, cargo --config net.offline=false fetch --locked can populate the cache; do not change system proxy settings.
