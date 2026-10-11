# 下载、构建和部署经锁定的媒体工具；只改变项目缓存、忽略的音源及 target 目录。
param([switch]$Build, [string]$MsysRoot, [string]$VcVars)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$work = Join-Path $root '.local/distribution'
$lock = Get-Content (Join-Path $root 'distribution/inputs.lock.json') -Raw | ConvertFrom-Json
New-Item -ItemType Directory -Force "$work/downloads", "$work/sources", "$work/build" | Out-Null

# 按输入记录的 file、url、sha256 获取固定文件，校验后返回缓存路径。
# 只写项目下载缓存；下载或哈希失败时抛错，已有缓存不匹配也拒绝使用，不覆盖已有文件。
function Get-LockedInput($inputRecord) {
    $path = Join-Path "$work/downloads" $inputRecord.file
    if (!(Test-Path -LiteralPath $path)) {
        & curl.exe --fail --location --max-time 600 --output "$path.pending" $inputRecord.url
        if ($LASTEXITCODE -ne 0) { throw "下载失败：$($inputRecord.file)" }
        if ((Get-FileHash "$path.pending").Hash.ToLowerInvariant() -ne $inputRecord.sha256) {
            throw "下载校验失败：$($inputRecord.file)"
        }
        Move-Item -LiteralPath "$path.pending" -Destination $path
    }
    if ((Get-FileHash -LiteralPath $path).Hash.ToLowerInvariant() -ne $inputRecord.sha256) {
        throw "缓存校验失败：$($inputRecord.file)"
    }
    return $path
}

foreach ($entry in $lock.inputs.PSObject.Properties) { Get-LockedInput $entry.Value | Out-Null }
if (!$MsysRoot) {
    $gcc = (Get-Command gcc.exe -ErrorAction Stop).Source
    $MsysRoot = Split-Path (Split-Path (Split-Path $gcc -Parent) -Parent) -Parent
}
$bash = Join-Path $MsysRoot 'usr/bin/bash.exe'
$cygpath = Join-Path $MsysRoot 'usr/bin/cygpath.exe'
if (!(Test-Path $bash)) { throw '需要 MSYS2 构建工具；不属于用户安装依赖' }
$posixWork = & $cygpath -u $work
$posixRoot = & $cygpath -u $root
if ($posixWork.Contains("'")) { throw '构建目录不能包含单引号' }

& $bash --noprofile --norc -c "export PATH=/usr/bin:`$PATH; cd '$posixWork/sources'; mkdir -p fluid; cd fluid; ar x '../../downloads/$($lock.inputs.soundfont.file)'; tar -xf data.tar.xz"
if ($LASTEXITCODE -ne 0) { throw '音源包提取失败' }
$font = "$work/sources/fluid/usr/share/sounds/sf2/FluidR3_GM.sf2"
if ((Get-FileHash $font).Hash.ToLowerInvariant() -ne $lock.soundfontSha256) { throw '音源内容校验失败' }
Copy-Item -LiteralPath $font -Destination "$root/assets/audio/FluidR3_GM.sf2" -Force
Expand-Archive "$work/downloads/$($lock.inputs.nsis.file)" "$work/build/nsis" -Force
Expand-Archive "$work/downloads/$($lock.inputs.nasm.file)" "$work/build/nasm" -Force

if ($Build) {
    if (!$VcVars -or !(Test-Path -LiteralPath $VcVars)) { throw '-Build 需要 -VcVars 指定 MSVC x64 环境脚本' }
    # AOM 的官方归档没有顶层目录；编码库固定提取到对应版本目录后再由共享构建脚本使用。
    & $bash --noprofile --norc -c "export PATH=/usr/bin:`$PATH; mkdir -p '$posixWork/sources/aom-3.13.1'; tar -xf '$posixWork/downloads/$($lock.inputs.aom.file)' -C '$posixWork/sources/aom-3.13.1'; tar -xf '$posixWork/downloads/$($lock.inputs.vpx.file)' -C '$posixWork/sources'"
    if ($LASTEXITCODE -ne 0) { throw 'AV1/VP9 源码提取失败' }
    & $bash --noprofile --norc -c "export PATH=/usr/bin:`$PATH; cd '$posixWork/sources'; tar -xf '../downloads/$($lock.inputs.ffmpeg.file)'; tar -xf '../downloads/$($lock.inputs.zlib.file)'; tar -xf '../downloads/$($lock.inputs.lame.file)'; cd ffmpeg-8.0.3; patch -p1 < '$posixRoot/distribution/ffmpeg-msvc-locale.patch'; cd ../zlib-1.3.1; patch -p1 < '$posixRoot/distribution/zlib-msvc.patch'"
    if ($LASTEXITCODE -ne 0) { throw '源码提取或构建补丁失败' }
    New-Item -ItemType Directory -Force "$work/gpg" | Out-Null
    & $bash --noprofile --norc -c "export PATH=/usr/bin:`$PATH; gpg --homedir '$posixWork/gpg' --import '$posixWork/downloads/$($lock.inputs.ffmpegKey.file)'; gpg --homedir '$posixWork/gpg' --verify '$posixWork/downloads/$($lock.inputs.ffmpegSignature.file)' '$posixWork/downloads/$($lock.inputs.ffmpeg.file)'"
    if ($LASTEXITCODE -ne 0) { throw 'FFmpeg 官方签名验证失败' }
    $batch = @"
@echo off
set VSLANG=1033
call "$VcVars"
if errorlevel 1 exit /b %errorlevel%
cd /d "$work\sources\zlib-1.3.1"
nmake -f win32\Makefile.msc zlib.lib "CFLAGS=-nologo -MT -W3 -O2 -D_CRT_SECURE_NO_WARNINGS"
if errorlevel 1 exit /b %errorlevel%
cd /d "$work\sources\lame-3.100"
nmake -f Makefile.MSVC MSVCVER=Win64 ASM=NO CPU=P1 MACHINE=/machine:X64 LIB_OPTS="/nologo /machine:X64" config.h libmp3lame-static.lib
if errorlevel 1 exit /b %errorlevel%
set PATH=%VCToolsInstallDir%bin\Hostx64\x64;$MsysRoot\usr\bin;%PATH%
"$bash" --noprofile --norc "$posixRoot/tools/build-ffmpeg.sh"
"@
    [IO.File]::WriteAllText("$work/build/build.cmd", $batch, [Text.Encoding]::Default)
    & cmd.exe /c "$work/build/build.cmd"
    if ($LASTEXITCODE -ne 0) { throw 'FFmpeg 构建失败' }
    & (Join-Path $PSScriptRoot 'seal-runtime.ps1') -MsysRoot $MsysRoot -VcVars $VcVars
}

if (!(Test-Path "$work/runtime/ffmpeg/bin/ffmpeg.exe")) { throw '首次准备请加 -Build 构建受控运行时' }
foreach ($profile in 'debug', 'debug/deps', 'release', 'release/deps') {
    $destination = Join-Path $root "target/$profile/runtime/ffmpeg"
    New-Item -ItemType Directory -Force $destination | Out-Null
    Get-ChildItem "$work/runtime/ffmpeg/bin" -File | Where-Object { $_.Extension -in '.exe', '.dll' } |
        Copy-Item -Destination $destination -Force
}
Write-Output '锁定音源、媒体运行时和安装工具准备完成。'
