# 生成本机验收安装包、同内容 ZIP 和准确对应的媒体源码包；不会上传或公开发布。
param([string]$OutputDirectory)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$work = Join-Path $root '.local/distribution'
if (!$OutputDirectory) { $OutputDirectory = Join-Path $root 'target/distribution' }
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
$manifest = Get-Content "$work/runtime-manifest.json" -Raw | ConvertFrom-Json
$lock = Get-Content "$root/distribution/inputs.lock.json" -Raw | ConvertFrom-Json
if (!($manifest.files.name -contains 'dinuosi_media.dll') -or !$manifest.bridgeInputs.Count) { throw '运行时未包含已封存的媒体动态库，请重新构建并封存' }
foreach ($input in $manifest.bridgeInputs) {
    if ((Get-FileHash (Join-Path $root $input.path)).Hash.ToLowerInvariant() -ne $input.sha256) { throw "媒体桥接源码已变更：$($input.path)" }
    $bundled = if ($input.path -like 'crates/media/native/*') { "$work/source-bundle/media-native/$(Split-Path $input.path -Leaf)" } else { "$work/source-bundle/build-media-library.py" }
    if ((Get-FileHash $bundled).Hash.ToLowerInvariant() -ne $input.sha256) { throw "媒体桥接对应源码不符：$bundled" }
}
foreach ($flag in '--disable-gpl','--disable-nonfree','--disable-version3','--enable-libaom','--enable-libvpx') {
    if (!$manifest.configuration.Contains($flag)) { throw "运行时构建约束不符：$flag" }
}
if ((Get-FileHash "$root/assets/audio/FluidR3_GM.sf2").Hash.ToLowerInvariant() -ne $lock.soundfontSha256) {
    throw '内嵌音源与锁定原始内容不一致'
}
$blockers = @(Get-Content "$work/license-blockers.json" -Raw | ConvertFrom-Json)
if ($blockers.Count) { throw "依赖许可原文未齐全：$($blockers.name -join ', ')" }
$audit = Get-Content "$work/license-audit.json" -Raw | ConvertFrom-Json
foreach ($input in $audit.inputs) {
    if ((Get-FileHash (Join-Path $root $input.path)).Hash.ToLowerInvariant() -ne $input.sha256) {
        throw 'Cargo 依赖输入已变更，请重新收集许可'
    }
}
if ((Get-FileHash "$work/licenses/rust-dependencies.json").Hash.ToLowerInvariant() -ne $audit.inventory_sha256) {
    throw '许可依赖清单与审计结果不对应'
}
$dependencies = @(Get-Content "$work/licenses/rust-dependencies.json" -Raw | ConvertFrom-Json)
if (!$dependencies.Count) { throw '没有经过审核的 Rust 依赖清单' }
foreach ($entry in $dependencies) {
    if ($entry.selected -match 'GPL|AGPL') { throw "不允许的运行依赖：$($entry.name)" }
    if (!$entry.notices.Count) { throw "缺少许可原文：$($entry.name)" }
    foreach ($notice in $entry.notices) {
        if ((Get-FileHash "$work/licenses/$($notice.file)").Hash.ToLowerInvariant() -ne $notice.sha256) {
            throw "许可原文已变更：$($entry.name)"
        }
    }
}
foreach ($file in $manifest.files) {
    if ((Get-FileHash "$work/runtime/ffmpeg/bin/$($file.name)").Hash.ToLowerInvariant() -ne $file.sha256) {
        throw "运行时已变更：$($file.name)"
    }
}
foreach ($key in 'ffmpeg','ffmpegSignature','ffmpegKey','zlib','lame','aom','vpx') {
    $entry = $lock.inputs.$key
    if ((Get-FileHash "$work/source-bundle/$($entry.file)").Hash.ToLowerInvariant() -ne $entry.sha256) {
        throw "对应源码缺失或改变：$key"
    }
}
foreach ($codec in @{directory='aom-3.13.1';name='AOM'}, @{directory='libvpx-1.15.2';name='VPX'}) {
    foreach ($notice in 'LICENSE','PATENTS') {
        $name = "$($codec.name)-$notice.txt"
        if ((Get-FileHash "$work/source-bundle/$name").Hash -ne (Get-FileHash "$work/sources/$($codec.directory)/$notice").Hash) {
            throw "编码库许可原文缺失或改变：$name"
        }
    }
}
foreach ($dependency in 'fastfeat','vector','x86inc') {
    if ((Get-FileHash "$work/source-bundle/AOM-$dependency-LICENSE.txt").Hash -ne (Get-FileHash "$work/sources/aom-3.13.1/third_party/$dependency/LICENSE").Hash) {
        throw "编码库许可原文缺失或改变：AOM-$dependency"
    }
}
if ((Get-FileHash "$work/source-bundle/VPX-x86inc-ISC.txt").Hash -ne (Get-FileHash "$work/sources/libvpx-1.15.2/third_party/x86inc/x86inc.asm").Hash) {
    throw '编码库许可原文缺失或改变：VPX-x86inc'
}
foreach ($name in 'ffmpeg-msvc-locale.patch','zlib-msvc.patch') {
    if ((Get-FileHash "$work/source-bundle/$name").Hash -ne (Get-FileHash "$root/distribution/$name").Hash) {
        throw "构建补丁与源码包不对应：$name"
    }
}
$exe = "$root/target/release/dinuosi.exe"
if (!(Test-Path -LiteralPath $exe)) { throw '请先构建 desktop release' }
$version = (Select-String -Path "$root/Cargo.toml" -Pattern '^version = "([^"]+)"').Matches.Groups[1].Value
$stage = "$work/package-$([guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory -Force $stage, "$stage/runtime/ffmpeg", "$stage/licenses", $OutputDirectory | Out-Null
Copy-Item -LiteralPath $exe -Destination "$stage/dinuosi.exe"
# 主程序的 MIT 原文随应用分发，并进入文件清单与卸载文件列表。
Copy-Item -LiteralPath "$root/LICENSE" -Destination "$stage/LICENSE"
foreach ($file in $manifest.files) { Copy-Item -LiteralPath "$work/runtime/ffmpeg/bin/$($file.name)" -Destination "$stage/runtime/ffmpeg" }
Copy-Item -Path "$work/licenses/*" -Destination "$stage/licenses" -Recurse
Copy-Item -LiteralPath "$root/assets/audio/FluidR3-LICENSE.txt" -Destination "$stage/licenses"
Copy-Item -LiteralPath "$work/sources/ffmpeg-8.0.3/COPYING.LGPLv2.1" -Destination "$stage/licenses/FFmpeg-LGPL-2.1.txt"
Copy-Item -LiteralPath "$work/sources/lame-3.100/COPYING" -Destination "$stage/licenses/LAME-LGPL-2.0.txt"
Copy-Item -LiteralPath "$work/sources/lame-3.100/LICENSE" -Destination "$stage/licenses/LAME-usage.txt"
Copy-Item -LiteralPath "$work/sources/zlib-1.3.1/README" -Destination "$stage/licenses/zlib.txt"
Copy-Item "$work/source-bundle/AOM-*.txt", "$work/source-bundle/VPX-*.txt" -Destination "$stage/licenses"
Copy-Item -LiteralPath "$work/build/nsis/nsis-3.13/COPYING" -Destination "$stage/licenses/NSIS.txt"
Copy-Item -LiteralPath "$work/runtime-manifest.json", "$work/license-audit.json" -Destination "$stage/licenses"
$sourceName = "Dinuosi-$version-ffmpeg-8.0.3-sources.zip"
& "$PSScriptRoot/write-license-notices.ps1" -OutputFile "$stage/licenses/THIRD_PARTY_NOTICES.txt"
[IO.File]::WriteAllText("$stage/licenses/SOURCE.txt","Corresponding FFmpeg/zlib/LAME/libaom/libvpx source: $sourceName (delivered alongside this package).`n",[Text.UTF8Encoding]::new($false))
$files = Get-ChildItem $stage -File -Recurse
$inventory = @($files | ForEach-Object { @{path=[IO.Path]::GetRelativePath($stage,$_.FullName).Replace('\','/'); sha256=(Get-FileHash $_.FullName).Hash.ToLowerInvariant();bytes=$_.Length} })
[IO.File]::WriteAllText("$stage/package-files.json",($inventory|ConvertTo-Json -Depth 4),[Text.UTF8Encoding]::new($false))
$deletions = @($files | ForEach-Object { 'Delete "$INSTDIR\' + [IO.Path]::GetRelativePath($stage,$_.FullName).Replace('/','\') + '"' })
$deletions += 'Delete "$INSTDIR\package-files.json"'
$deletions += @(Get-ChildItem $stage -Directory -Recurse | Sort-Object {$_.FullName.Length} -Descending | ForEach-Object { 'RMDir "$INSTDIR\' + [IO.Path]::GetRelativePath($stage,$_.FullName).Replace('/','\') + '"' })
$uninstall = [IO.Path]::GetFullPath("$work/build/uninstall-files.nsh").Replace('/' , '\')
[IO.File]::WriteAllLines($uninstall,$deletions,[Text.UTF8Encoding]::new($false))
$output = "$OutputDirectory/Dinuosi-$version-windows-x64-setup.exe"
& "$work/build/nsis/nsis-3.13/makensis.exe" /INPUTCHARSET UTF8 /V2 "/DSTAGE=$stage" "/DOUTPUT=$output" "/DVERSION=$version" "/DREMOVAL_LIST=$uninstall" "$root/distribution/installer.nsi"
if ($LASTEXITCODE -ne 0) { throw '安装包编译失败' }
Compress-Archive -Path "$stage/*" -DestinationPath "$OutputDirectory/Dinuosi-$version-windows-x64.zip" -Force
Compress-Archive -Path "$work/source-bundle/*" -DestinationPath "$OutputDirectory/$sourceName" -Force
Copy-Item -LiteralPath "$stage/licenses/THIRD_PARTY_NOTICES.txt" -Destination "$OutputDirectory/THIRD_PARTY_NOTICES.txt" -Force
[IO.File]::WriteAllText("$OutputDirectory/build-artifacts.json",(@{stage=$stage;version=$version;outputs=@(Get-ChildItem $OutputDirectory -File | Where-Object {$_.Extension -in '.exe','.zip'} | ForEach-Object {@{name=$_.Name;sha256=(Get-FileHash $_.FullName).Hash.ToLowerInvariant();bytes=$_.Length}})}|ConvertTo-Json -Depth 5),[Text.UTF8Encoding]::new($false))
# 让本地开发程序的关于窗口使用同一许可文本，避免另维护一个会漂移的清单。
foreach ($profile in 'debug','debug/deps','release','release/deps') {
    $destination = "$root/target/$profile/licenses"
    New-Item -ItemType Directory -Force $destination | Out-Null
    Copy-Item -LiteralPath "$stage/licenses/THIRD_PARTY_NOTICES.txt" -Destination $destination -Force
}
Write-Output "已生成内部验收产物：$OutputDirectory"
