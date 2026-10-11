# 对本次 FFmpeg 构建封存文件校验值、配置和必要源码；打包阶段只能使用这一清单。
param([string]$MsysRoot, [string]$VcVars)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$work = Join-Path $root '.local/distribution'
$ffmpeg = "$work/runtime/ffmpeg/bin/ffmpeg.exe"
$configuration = (& $ffmpeg -buildconf 2>&1 | Out-String)
if ($LASTEXITCODE -ne 0) { throw 'FFmpeg 无法运行' }
foreach ($flag in '--disable-gpl', '--disable-nonfree', '--disable-version3', '--disable-autodetect', '--enable-libmp3lame', '--enable-mediafoundation') {
    if (!$configuration.Contains($flag)) { throw "缺少受控运行时构建约束：$flag" }
}
$license = (& $ffmpeg -L 2>&1 | Out-String)
if (!$license.Contains('Lesser General Public License')) { throw '产物不是 LGPL FFmpeg' }
$source = "$work/source-bundle"
New-Item -ItemType Directory -Force $source | Out-Null
$lock = Get-Content "$root/distribution/inputs.lock.json" -Raw | ConvertFrom-Json
foreach ($key in 'ffmpeg','ffmpegSignature','ffmpegKey','zlib','lame') {
    $entry = $lock.inputs.$key
    Copy-Item -LiteralPath "$work/downloads/$($entry.file)" -Destination $source -Force
}
Copy-Item "$root/distribution/ffmpeg-msvc-locale.patch", "$root/distribution/zlib-msvc.patch", "$root/distribution/inputs.lock.json", "$root/tools/build-ffmpeg.sh", "$root/tools/prepare-media.ps1", "$root/tools/seal-runtime.ps1", "$root/tools/rebuild-runtime.ps1" -Destination $source -Force
$vcRoot = Split-Path (Split-Path (Split-Path $VcVars -Parent) -Parent) -Parent
$compilerRoot = Get-ChildItem (Join-Path $vcRoot 'Tools/MSVC') -Directory | Sort-Object Name -Descending | Select-Object -First 1
$record = [ordered]@{
    ffmpegVersion = $lock.ffmpegVersion
    license = 'LGPL-2.1-or-later'
    configuration = $configuration
    compiler = (Get-Item (Join-Path $compilerRoot.FullName 'bin/Hostx64/x64/cl.exe')).VersionInfo.FileVersion
    msvcEnvironment = $VcVars
    msysVersion = (& "$MsysRoot/usr/bin/bash.exe" --version | Select-Object -First 1)
    os = (Get-CimInstance Win32_OperatingSystem).Version
    files = @()
}
foreach ($file in Get-ChildItem "$work/runtime/ffmpeg/bin" -File | Where-Object {$_.Extension -in '.exe','.dll'}) {
    $record.files += @{name=$file.Name; sha256=(Get-FileHash $file.FullName).Hash.ToLowerInvariant();bytes=$file.Length}
}
if (!$record.files.Count) { throw '没有可封存的运行时文件' }
[IO.File]::WriteAllText("$work/runtime-manifest.json", ($record | ConvertTo-Json -Depth 6), [Text.UTF8Encoding]::new($false))
Copy-Item "$work/runtime-manifest.json" -Destination $source -Force
$instructions = @"
Dinuosi FFmpeg 8.0.3 corresponding source

FFmpeg is LGPL-2.1-or-later; zlib is zlib licensed; libmp3lame is LGPL-2.0-or-later. LAME is unmodified and only its static encoding/decoding library is linked, not its frontend programs. The FFmpeg patch fixes MSVC locale/UTF-8 handling
and includes the D3D11 type declarations required by Media Foundation even
when D3D11 decoding autodetection is disabled. A zlib MSVC header compatibility
patch is also included. Both original archives and patches are included.
The patch does not change media algorithms or enable GPL functionality.

Use tools/prepare-media.ps1 -Build with MSYS2, MSVC x64 and NASM 2.16.03.
This bundle's scripts retain their original repository paths: reconstruct a
repository with tools/ and distribution/ subdirectories, restore the included
inputs.lock.json and patch, and place source archives in .local/distribution/downloads.
The locked full input list also identifies the MIT soundfont and NSIS tooling.
For a media-only reconstruction use rebuild-runtime.ps1 -SourceBundle <this-directory>
-MsysRoot <MSYS2> -VcVars <vcvars64.bat> -NasmDirectory <NASM-2.16.03>
-BuildDirectory <an-empty-directory>. It extracts fresh sources and rebuilds all objects.
All downloaded inputs are checked before use. Windows users do not need these
build tools to use Dinuosi. The recorded toolchain and configure line are in
runtime-manifest.json. Build from sources again; matching version text alone
is not accepted as reconstruction evidence.

No object code is linked from FFmpeg into the Dinuosi application.
FFmpeg is a replaceable standalone program, using replaceable libav DLLs.
H.264 Media Foundation encoding calls the codecs installed with Windows or
its hardware drivers. Windows codec DLLs and GPL encoders such as libx264
are not copied into the package. Codec availability is checked on the host;
this build configuration does not grant codec patent or commercial licenses.
"@
[IO.File]::WriteAllText("$source/BUILDING.txt", $instructions, [Text.UTF8Encoding]::new($false))
Copy-Item "$work/sources/ffmpeg-8.0.3/COPYING.LGPLv2.1", "$work/sources/ffmpeg-8.0.3/LICENSE.md" -Destination $source -Force
Copy-Item "$work/sources/zlib-1.3.1/README" -Destination "$source/zlib-README.txt" -Force
Copy-Item "$work/sources/lame-3.100/COPYING" -Destination "$source/LAME-LGPL-2.0.txt" -Force
Copy-Item "$work/sources/lame-3.100/LICENSE" -Destination "$source/LAME-usage.txt" -Force
Write-Output 'LGPL 运行时已封存。'
