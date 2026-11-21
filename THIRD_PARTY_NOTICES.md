# Third-party notices

Sonitrelix original code is MIT licensed. Dependency licenses were reviewed against the actual resolved Cargo metadata and retained license files before choosing MIT for this foundation. These permissive licenses allow the original application to remain MIT; dependency notices and Unicode data terms remain applicable.

| Locked package | Version | Declared license |
| --- | --- | --- |
| serde | 1.0.229 | MIT OR Apache-2.0 |
| serde_core | 1.0.229 | MIT OR Apache-2.0 |
| serde_derive | 1.0.229 | MIT OR Apache-2.0 |
| serde_json | 1.0.154 | MIT OR Apache-2.0 |
| itoa | 1.0.18 | MIT OR Apache-2.0 |
| memchr | 2.8.3 | Unlicense OR MIT |
| zmij | 1.0.23 | MIT |
| proc-macro2 | 1.0.107 | MIT OR Apache-2.0 |
| quote | 1.0.47 | MIT OR Apache-2.0 |
| syn | 3.0.9 | MIT OR Apache-2.0 |
| syn | 2.0.119 | MIT OR Apache-2.0 |
| unicode-ident | 1.0.27 | (MIT OR Apache-2.0) AND Unicode-3.0 |
| windows | 0.62.2 | MIT OR Apache-2.0 |
| windows-collections | 0.3.2 | MIT OR Apache-2.0 |
| windows-core | 0.62.2 | MIT OR Apache-2.0 |
| windows-future | 0.3.2 | MIT OR Apache-2.0 |
| windows-implement | 0.60.2 | MIT OR Apache-2.0 |
| windows-interface | 0.59.3 | MIT OR Apache-2.0 |
| windows-link | 0.2.1 | MIT OR Apache-2.0 |
| windows-numerics | 0.3.1 | MIT OR Apache-2.0 |
| windows-result | 0.4.1 | MIT OR Apache-2.0 |
| windows-strings | 0.5.1 | MIT OR Apache-2.0 |
| windows-threading | 0.2.1 | MIT OR Apache-2.0 |
| cc | 1.9.0 | MIT OR Apache-2.0 |
| find-msvc-tools | 0.1.15 | MIT OR Apache-2.0 |
| libc | 0.2.191 | MIT OR Apache-2.0 |
| openssl-src | 300.6.1+3.6.3 | MIT OR Apache-2.0 wrapper; Apache-2.0 OpenSSL |
| openssl-sys | 0.9.117 | MIT |
| pkg-config | 0.3.34 | MIT OR Apache-2.0 |
| shlex | 2.0.1 | MIT OR Apache-2.0 |
| vcpkg | 0.2.15 | MIT OR Apache-2.0 |

Select the MIT option where offered, retaining the Unicode-3.0 terms for unicode-ident's data. Full supplied notices are retained in `third-party-licenses/<name>-<version>/`; `inventory.json` records Cargo metadata. Some packages are build-time proc-macro dependencies; their notices are retained as well. Rebuild this inventory when Cargo.lock changes. Standard Windows system DLLs are linked through original ABI declarations, not redistributed.

The Microsoft [windows-rs](https://github.com/microsoft/windows-rs) bindings are now linked for WASAPI and Windows threading. The current graph contains 31 external packages; full supplied notices and metadata were retained. The WAV reader and SPSC queue are original Sonitrelix code. CPAL, Symphonia, Hound and Rubato remain candidates, not linked dependencies; their exact graphs must be audited if added.

OpenSSL 3.6.3 is statically linked for Ed25519, SHA-256 and random bytes. Its Apache-2.0 license is retained in third-party-licenses/OpenSSL-3.6.3-LICENSE.txt; build/wrapper licenses are separate.

Portable packages include unmodified release vcruntime140 DLLs from Visual Studio designated x64 redistributables under Microsoft terms, not MIT. See https://learn.microsoft.com/en-us/visualstudio/releases/2022/redistribution and https://learn.microsoft.com/en-us/cpp/windows/redistributing-visual-cpp-files. Only appropriately licensed redistributables may be packaged. Other Windows DLLs and Media Foundation codecs are not bundled. Original PCM WAV/FLAC code needs no external codec library; FLAC follows RFC 9639.

Official release ZIPs include a separate FFmpeg executable built with LAME and Opus. See [FFmpeg build and corresponding source](docs/FFMPEG_BUILD.md) for license details, source downloads and build instructions.
