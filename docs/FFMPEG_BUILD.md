# FFmpeg in the Windows release

Sonitrelix runs the bundled `ffmpeg.exe` as a separate process. This x64 build includes FFmpeg 9.0.2 and LAME 3.100 under LGPL-2.1-or-later and Opus 1.6.1 under its BSD-style license. The portable ZIP includes their license texts. You can replace `ffmpeg.exe` beside Sonitrelix with a compatible build.

Download `FFmpeg-corresponding-source.zip` from the same [Sonitrelix release](https://github.com/AnimalMetal/sonitrelix/releases). It contains the exact upstream source archives and the build script. The sources are unmodified.

| Source | Upstream archive |
| --- | --- |
| FFmpeg | [ffmpeg-9.0.2.tar.xz](https://ffmpeg.org/releases/ffmpeg-9.0.2.tar.xz) |
| LAME | [lame_3.100.orig.tar.gz](https://deb.debian.org/debian/pool/main/l/lame/lame_3.100.orig.tar.gz) |
| Opus | [opus-1.6.1.tar.gz](https://downloads.xiph.org/releases/opus/opus-1.6.1.tar.gz) |

```text
8c3850283eb25fa026482078a04051e0be17347b09ef81a0849bec15a96e002e  ffmpeg-9.0.2.tar.xz
ddfe36cab873794038ae2c1210557ad34857a4b6bdc515785d1da9e175b1da1e  lame_3.100.orig.tar.gz
6ffcb593207be92584df15b32466ed64bbec99109f007c82205f0194572411a1  opus-1.6.1.tar.gz
```

Extract the source ZIP, open Git Bash in the extracted directory, and use MinGW-w64 GCC 13.2.0, NASM, CMake, mingw32-make, MSYS GNU Make 4.4.1 and pkgconf 2.4.3 on PATH. Run the following commands in that directory.

```sh
tar -xf ffmpeg-9.0.2.tar.xz
tar -xf lame_3.100.orig.tar.gz
tar -xf opus-1.6.1.tar.gz
bash build-ffmpeg.sh lame
bash build-ffmpeg.sh opus
bash build-ffmpeg.sh ffmpeg
./ffmpeg-9.0.2/ffmpeg.exe -version
```

The build includes audio codecs and filters for Sonitrelix. Video import extracts audio. GPL, nonfree and network features are disabled.
