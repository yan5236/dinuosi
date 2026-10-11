# 根据当前许可审计生成统一的第三方声明；OutputFile 指定输出，默认写入项目许可缓存。
# 开头包含主程序 MIT 原文，随后保留原生库和 Rust 依赖原始许可；缺失或过期时抛错，不下载材料。
param([string]$OutputFile)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$work = Join-Path $root '.local/distribution'
if (!$OutputFile) { $OutputFile = "$work/licenses/THIRD_PARTY_NOTICES.txt" }
if (!(Test-Path -LiteralPath "$work/license-audit.json")) {
    throw '缺少第三方许可材料，请先运行 python tools/collect-licenses.py'
}
$audit = Get-Content -LiteralPath "$work/license-audit.json" -Raw -Encoding UTF8 | ConvertFrom-Json
foreach ($inputRecord in $audit.inputs) {
    if ((Get-FileHash -LiteralPath (Join-Path $root $inputRecord.path)).Hash.ToLowerInvariant() -ne $inputRecord.sha256) {
        throw 'Cargo 依赖输入已变更，请重新运行 python tools/collect-licenses.py'
    }
}
$inventory = "$work/licenses/rust-dependencies.json"
if ((Get-FileHash -LiteralPath $inventory).Hash.ToLowerInvariant() -ne $audit.inventory_sha256) {
    throw '第三方许可清单与审计结果不对应'
}
$dependencies = @(Get-Content -LiteralPath $inventory -Raw -Encoding UTF8 | ConvertFrom-Json)
if (!$dependencies.Count) { throw '没有经过审核的 Rust 依赖清单' }
$lock = Get-Content -LiteralPath "$root/distribution/inputs.lock.json" -Raw -Encoding UTF8 | ConvertFrom-Json
$version = (Select-String -Path "$root/Cargo.toml" -Pattern '^version = "([^"]+)"').Matches.Groups[1].Value
$sourceName = "Dinuosi-$version-ffmpeg-$($lock.ffmpegVersion)-sources.zip"
$notes = @"
Dinuosi $version

Dinuosi 主程序采用 MIT 许可证，版权归原作者所有。完整 MIT 原文见下文及程序目录中的 LICENSE。
MIT 允许使用、复制、修改、分发及商业使用，须保留其版权声明和许可文本。
第三方组件分别遵循各自许可证，不因主程序采用 MIT 而改变。

本程序使用 FFmpeg $($lock.ffmpegVersion) 的动态库，遵循 LGPL-2.1-or-later。
FFmpeg: https://ffmpeg.org/；LGPL: https://www.gnu.org/licenses/old-licenses/lgpl-2.1.html
用户可以修改这些库，并在退出程序后替换 runtime/ffmpeg 中接口兼容的版本；允许为调试这些修改进行逆向工程。
FFmpeg、zlib、LAME、libaom、libvpx 的对应源码、补丁和构建说明应随发行版本提供：$sourceName。
分发二进制时须从同一下载位置提供准确对应的源码包；本文件不代替源码交付。

LAME 3.100：LGPL-2.0-or-later，https://lame.sourceforge.io/
zlib $($lock.zlibVersion)：zlib License，https://zlib.net/
libaom 3.13.1：BSD-2-Clause；libvpx 1.15.2：BSD-3-Clause。
This software is based in part on the work of the Independent JPEG Group.
FluidR3 GM：MIT，Copyright 2000-2002, 2008 Frank Wen and 2008 Toby Smithe。
RustySynth：MIT。安装程序使用 NSIS，遵循其 zlib/libpng License。
以下为原始许可文本和当前 Windows Rust 运行依赖声明；MPL 及已记录上游许可文件缺口的对应源码保留在 sources/。

===== Dinuosi — MIT =====
"@
$notes += "`n" + (Get-Content -LiteralPath "$root/LICENSE" -Raw -Encoding UTF8)
$nativeNotices = @(
    @{name='FFmpeg — LGPL-2.1'; path="$work/sources/ffmpeg-$($lock.ffmpegVersion)/COPYING.LGPLv2.1"},
    @{name='LAME — LGPL-2.0'; path="$work/sources/lame-3.100/COPYING"},
    @{name='LAME — usage'; path="$work/sources/lame-3.100/LICENSE"},
    @{name='zlib'; path="$work/sources/zlib-$($lock.zlibVersion)/README"},
    @{name='FluidR3 GM'; path="$root/assets/audio/FluidR3-LICENSE.txt"},
    @{name='NSIS'; path="$work/build/nsis/nsis-$($lock.nsisVersion)/COPYING"}
)
foreach ($name in 'AOM-LICENSE', 'AOM-PATENTS', 'AOM-fastfeat-LICENSE', 'AOM-vector-LICENSE',
    'AOM-x86inc-LICENSE', 'VPX-LICENSE', 'VPX-PATENTS', 'VPX-x86inc-ISC') {
    $nativeNotices += @{name=$name; path="$work/source-bundle/$name.txt"}
}
foreach ($notice in $nativeNotices) {
    $notes += "`n`n===== $($notice.name) =====`n" + (Get-Content -LiteralPath $notice.path -Raw -Encoding UTF8)
}
foreach ($entry in $dependencies) {
    if ($entry.selected -match 'GPL|AGPL' -or !$entry.notices.Count) { throw "依赖许可材料不符合要求：$($entry.name)" }
    $notes += "`n`n===== $($entry.name) $($entry.version) — $($entry.selected) =====`n$($entry.repository)`n"
    foreach ($notice in $entry.notices) {
        $path = Join-Path "$work/licenses" $notice.file
        if ((Get-FileHash -LiteralPath $path).Hash.ToLowerInvariant() -ne $notice.sha256) {
            throw "许可原文已变更：$($entry.name)"
        }
        $notes += "`n" + (Get-Content -LiteralPath $path -Raw -Encoding UTF8)
    }
}
New-Item -ItemType Directory -Force (Split-Path $OutputFile -Parent) | Out-Null
[IO.File]::WriteAllText($OutputFile, $notes, [Text.UTF8Encoding]::new($false))
