#!/usr/bin/env bash
# Run with Git for Windows Bash and a MinGW-w64 toolchain on PATH.
set -euo pipefail

jobs=${JOBS:-8}
root=$(pwd)
prefix=$(cygpath -m "$root/prefix")
make_command=${MAKE:-make}

case "${1:-}" in
  lame)
    cd lame-3.100
    ./configure --prefix="$prefix" --host=x86_64-w64-mingw32 \
      --build=x86_64-w64-mingw32 --disable-shared --enable-static --disable-frontend
    "$make_command" -j"$jobs"
    "$make_command" install
    ;;
  opus)
    opus_source=$(cygpath -m "$root/opus-1.6.1")
    opus_source_native=$(cygpath -w "$root/opus-1.6.1")
    cmake -S opus-1.6.1 -B opus-build -G "MinGW Makefiles" \
      -DCMAKE_C_COMPILER=gcc -DCMAKE_INSTALL_PREFIX="$prefix" \
      "-DCMAKE_C_FLAGS:STRING=-ffile-prefix-map=\"$opus_source\"=opus-1.6.1 -ffile-prefix-map=\"$opus_source_native\"=opus-1.6.1" \
      -DCMAKE_BUILD_TYPE=Release -DOPUS_BUILD_TESTING=OFF -DOPUS_BUILD_PROGRAMS=OFF
    cmake --build opus-build --parallel "$jobs"
    cmake --install opus-build
    ;;
  ffmpeg)
    cd ffmpeg-9.0.2
    export PKG_CONFIG_PATH="../prefix/lib/pkgconfig"
    # Keep paths relative so the version output can be distributed unchanged.
    ./configure --target-os=mingw32 --arch=x86_64 --cc=gcc \
      --disable-autodetect --disable-network --disable-debug --disable-doc \
      --disable-ffplay --disable-ffprobe --disable-avdevice --disable-swscale --pkg-config=pkgconf \
      --enable-static --disable-shared --disable-gpl --disable-nonfree \
      --enable-libmp3lame --enable-libopus --enable-small \
      --disable-encoders --enable-encoder=aac,flac,libmp3lame,libopus,pcm_f32le,pcm_s16le,pcm_s24le,pcm_s32le \
      --disable-decoders --enable-decoder=aac,aac_fixed,aac_latm,alac,ape,flac,mp2,mp2float,mp3,mp3float,opus,vorbis,tta,wavpack,wmav1,wmav2,wmapro,wmalossless,pcm_* \
      --extra-cflags=-I../prefix/include --extra-ldflags="-L../prefix/lib -static-libgcc"
    "$make_command" -j"$jobs"
    ;;
  *)
    printf 'Usage: bash build-ffmpeg.sh {lame|opus|ffmpeg}\n' >&2
    exit 2
    ;;
esac
