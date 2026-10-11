#!/usr/bin/env bash
# 在 MSVC x64 环境构建锁定的 BSD 编码库，只写项目缓存；失败立即终止，不获取系统安装库。
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
work="${DINUOSI_BUILD_ROOT:-$root/.local/distribution}"
mkdir -p "$work/build/tmp"
export TMPDIR="$work/build/tmp"
vs=$(cygpath -u "$VSINSTALLDIR")
export PATH="$work/build/nasm/nasm-2.16.03:$vs/Common7/IDE/CommonExtensions/Microsoft/CMake/CMake/bin:$vs/Common7/IDE/CommonExtensions/Microsoft/CMake/Ninja:$PATH"
mkdir -p "$work/build/aom" "$work/build/vpx" "$work/build/codecs/include" "$work/build/codecs/pkgconfig"
cmake -S "$work/sources/aom-3.13.1" -B "$work/build/aom" -G Ninja \
  -DCMAKE_BUILD_TYPE=Release -DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded \
  -DENABLE_NASM=ON -DENABLE_TESTS=OFF -DENABLE_EXAMPLES=OFF -DENABLE_TOOLS=OFF \
  -DENABLE_DOCS=OFF -DBUILD_SHARED_LIBS=OFF
cmake --build "$work/build/aom" --parallel 2
cp "$work/build/aom/aom.lib" "$work/build/codecs/aom.lib"
cp -r "$work/sources/aom-3.13.1/aom" "$work/build/codecs/include/"
cd "$work/build/vpx"
"$work/sources/libvpx-1.15.2/configure" --target=x86_64-win64-vs17 --as=nasm \
  --enable-static-msvcrt --disable-examples --disable-tools --disable-unit-tests --disable-docs
make -r -j 2 vpx.vcxproj
"$vs/MSBuild/Current/Bin/MSBuild.exe" vpx.vcxproj -p:Configuration=Release -p:Platform=x64 -m:2 -v:quiet
cp x64/Release/vpxmt.lib "$work/build/codecs/vpx.lib"
cp -r "$work/sources/libvpx-1.15.2/vpx" "$work/build/codecs/include/"
# FFmpeg 的 AOM 检测需要 pkg-config；只描述本次构建产物，不搜索系统编码库。
cat > "$work/build/codecs/pkgconfig/aom.pc" <<EOF
prefix=$(cygpath -m "$work/build/codecs")
Name: aom
Description: Locked Alliance for Open Media codec
Version: 3.13.1
Libs: -L\${prefix} -laom
Cflags: -I\${prefix}/include
EOF
