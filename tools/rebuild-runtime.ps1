# 从交付源码包的原始归档与补丁重建媒体工具，不复用第一次构建的对象文件。
param([Parameter(Mandatory)][string]$SourceBundle,[Parameter(Mandatory)][string]$MsysRoot,[Parameter(Mandatory)][string]$VcVars,[Parameter(Mandatory)][string]$NasmDirectory,[Parameter(Mandatory)][string]$BuildDirectory)
$ErrorActionPreference='Stop'
$source=[IO.Path]::GetFullPath($SourceBundle)
$work=[IO.Path]::GetFullPath($BuildDirectory)
if((Test-Path -LiteralPath $work) -and (Get-ChildItem -LiteralPath $work -Force | Select-Object -First 1)){
    throw '重建目录必须为空，防止复用旧对象文件'
}
$lock=Get-Content "$source/inputs.lock.json" -Raw|ConvertFrom-Json
New-Item -ItemType Directory -Force "$work/sources","$work/build/nasm/nasm-2.16.03" | Out-Null
foreach($key in 'ffmpeg','zlib','lame','aom','vpx'){
    $inputRecord=$lock.inputs.$key
    $archive=Join-Path $source $inputRecord.file
    if((Get-FileHash $archive).Hash.ToLowerInvariant() -ne $inputRecord.sha256){throw "源码校验失败：$key"}
    $archivePosix = & "$MsysRoot/usr/bin/cygpath.exe" -u $archive
    $sourcesPosix = & "$MsysRoot/usr/bin/cygpath.exe" -u "$work/sources"
    if ($key -eq 'aom') {
        New-Item -ItemType Directory -Force "$work/sources/aom-3.13.1" | Out-Null
        $sourcesPosix += '/aom-3.13.1'
    }
    & "$MsysRoot/usr/bin/bash.exe" --noprofile --norc -c "export PATH=/usr/bin:`$PATH; tar -xf '$archivePosix' -C '$sourcesPosix'"
    if($LASTEXITCODE -ne 0){throw "源码提取失败：$key"}
}
Copy-Item "$NasmDirectory/*" "$work/build/nasm/nasm-2.16.03" -Recurse -Force
$bash="$MsysRoot/usr/bin/bash.exe"
$posixWork=& "$MsysRoot/usr/bin/cygpath.exe" -u $work
$posixSource=& "$MsysRoot/usr/bin/cygpath.exe" -u $source
if($posixWork.Contains("'") -or $posixSource.Contains("'")){throw '构建路径不能包含单引号'}
& $bash --noprofile --norc -c "export PATH=/usr/bin:`$PATH; cd '$posixWork/sources/ffmpeg-8.0.3'; patch -p1 < '$posixSource/ffmpeg-msvc-locale.patch'; cd ../zlib-1.3.1; patch -p1 < '$posixSource/zlib-msvc.patch'"
if($LASTEXITCODE -ne 0){throw '源码补丁应用失败'}
$batch=@"
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
set DINUOSI_BUILD_ROOT=$work
"$bash" --noprofile --norc "$posixSource/build-ffmpeg.sh"
"@
[IO.File]::WriteAllText("$work/build/rebuild.cmd",$batch,[Text.Encoding]::Default)
& cmd.exe /c "$work/build/rebuild.cmd"
if($LASTEXITCODE -ne 0){throw '源码包重建失败'}
Write-Output "已从独立源码目录重建：$work/runtime/ffmpeg/bin"
