# FFmpeg in the Windows release

Sonitrelix runs the bundled `ffmpeg.exe` as a separate process. This x64 build includes FFmpeg 9.0.2 and LAME 3.100 under LGPL-2.1-or-later and Opus 1.6.1 under its BSD-style license. The portable ZIP includes their license texts. It also includes the Winpthreads runtime DLL and its license. You can replace `ffmpeg.exe` beside Sonitrelix with a compatible build.

Download `FFmpeg-corresponding-source.zip` from the same [Sonitrelix release](https://github.com/AnimalMetal/sonitrelix/releases). It contains the exact upstream source archives and the build script. The sources are unmodified.

| Source | Upstream archive |
| --- | --- |
| FFmpeg | [ffmpeg-9.0.2.tar.xz](https://ffmpeg.org/releases/ffmpeg-9.0.2.tar.xz) |
| LAME | [lame_3.100.orig.tar.gz](https://deb.debian.org/debian/pool/main/l/lame/lame_3.100.orig.tar.gz) |
| Opus | [opus-1.6.1.tar.gz](https://downloads.xiph.org/releases/opus/opus-1.6.1.tar.gz) |
| MinGW-w64, including Winpthreads | [mingw-w64-11.0.1.tar.gz](https://codeload.github.com/mingw-w64/mingw-w64/tar.gz/refs/tags/v11.0.1) |

```text
8c3850283eb25fa026482078a04051e0be17347b09ef81a0849bec15a96e002e  ffmpeg-9.0.2.tar.xz
ddfe36cab873794038ae2c1210557ad34857a4b6bdc515785d1da9e175b1da1e  lame_3.100.orig.tar.gz
6ffcb593207be92584df15b32466ed64bbec99109f007c82205f0194572411a1  opus-1.6.1.tar.gz
9c82a58713786c95b0594443a1c6190cd986401ed2b03677a90acc470140af28  mingw-w64-11.0.1.tar.gz
```

Extract the source ZIP, open Git Bash in the extracted directory, and use the WinLibs GCC 13.2.0 / MinGW-w64 UCRT 11.0.1 r8 toolchain (x64, POSIX threads) with its NASM, CMake and mingw32-make. Put MSYS GNU Make 4.4.1 and pkgconf 2.4.3 on PATH as well. Run the following commands in that directory.

```sh
tar -xf ffmpeg-9.0.2.tar.xz
tar -xf lame_3.100.orig.tar.gz
tar -xf opus-1.6.1.tar.gz
tar -xf mingw-w64-11.0.1.tar.gz
bash build-ffmpeg.sh lame
bash build-ffmpeg.sh opus
bash build-ffmpeg.sh ffmpeg
./ffmpeg-9.0.2/ffmpeg.exe -version
```

The build includes audio codecs and filters for Sonitrelix. Video import extracts audio. GPL, nonfree and network features are disabled.

For packaging, pass the toolchain's `bin/libwinpthread-1.dll` as `-WinpthreadsRuntime`. Its SHA-256 is `4bde68fbb2c6a9a64535928fc505056ebf6f5dc44f82ea2eaac3c714dc22ce79`.
