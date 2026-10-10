#!/usr/bin/env bash
# 在已加载 MSVC x64 环境的 MSYS2 中编译固定源码；产物只写入项目构建缓存。
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
work="${DINUOSI_BUILD_ROOT:-$root/.local/distribution}"
export PATH="$work/build/nasm/nasm-2.16.03:$PATH"
mkdir -p "$work/build/ffmpeg" "$work/runtime/ffmpeg"
mkdir -p "$work/build/lame/include/lame"
cp "$work/sources/lame-3.100/include/lame.h" "$work/build/lame/include/lame/lame.h"
cp "$work/sources/lame-3.100/output/libmp3lame-static.lib" "$work/build/lame/mp3lame.lib"
cd "$work/build/ffmpeg"
# 中文 MSVC 的上游依赖扫描会漏掉头文件；重新配置前清除旧对象以避免混入旧功能。
if [ -f ffbuild/config.mak ]; then make -r clean; fi
"$work/sources/ffmpeg-8.0.3/configure" \
  --toolchain=msvc --cc=cl.exe --cxx=cl.exe --arch=x86_64 --target-os=win32 \
  --disable-autodetect --disable-gpl --disable-nonfree --disable-version3 \
  --enable-shared --disable-static --disable-ffplay --disable-doc --disable-debug \
  --enable-zlib --enable-libmp3lame --disable-network \
  --extra-cflags="-I$(cygpath -m "$work/sources/zlib-1.3.1") -I$(cygpath -m "$work/build/lame/include")" \
  --extra-ldflags="-LIBPATH:$(cygpath -m "$work/sources/zlib-1.3.1") -LIBPATH:$(cygpath -m "$work/build/lame")" \
  --prefix="$work/runtime/ffmpeg"
make -r -j 4
make -r install
