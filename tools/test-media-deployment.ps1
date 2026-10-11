# 使用隔离缓存验证 DLL、MIT 与第三方声明部署；应用不带 CLI，缺库或许可材料时拒绝写入。
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$temporary = [IO.Path]::GetFullPath((Join-Path $root ".local/media-deployment-$([guid]::NewGuid().ToString('N'))"))
$allowed = [IO.Path]::GetFullPath((Join-Path $root '.local')) + [IO.Path]::DirectorySeparatorChar
if (!$temporary.StartsWith($allowed, [StringComparison]::OrdinalIgnoreCase)) { throw '测试目录越界' }
$source = Join-Path $temporary '.local/distribution/runtime/ffmpeg/bin'
New-Item -ItemType Directory -Force $source, "$temporary/tools" | Out-Null
Copy-Item -LiteralPath "$PSScriptRoot/prepare-media.ps1" -Destination "$temporary/tools"
Copy-Item -LiteralPath "$PSScriptRoot/write-license-notices.ps1" -Destination "$temporary/tools"
$shell = (Get-Process -Id $PID).Path

# 在独立 PowerShell 进程执行真实准备脚本，避免脚本 exit 结束测试。
# 返回全部输出；实际退出码必须符合 success，失败信息由调用者断言，不下载或访问真实缓存。
function Invoke-Deployment([bool]$success, [switch]$Build) {
    $arguments = @('-NoProfile', '-File', "$temporary/tools/prepare-media.ps1", '-DeployOnly')
    if ($Build) { $arguments += '-Build' }
    $output = (& $shell @arguments 2>&1 | Out-String)
    if (($LASTEXITCODE -eq 0) -ne $success) { throw "部署退出码不符合预期：$output" }
    return $output
}

try {
    $work = "$temporary/.local/distribution"
    New-Item -ItemType Directory -Force "$work/licenses", "$work/source-bundle", "$temporary/distribution", "$temporary/assets/audio" | Out-Null
    Copy-Item -LiteralPath "$root/LICENSE" -Destination "$temporary/LICENSE"
    [IO.File]::WriteAllText("$temporary/Cargo.toml", 'version = "0.1.0"')
    [IO.File]::WriteAllText("$temporary/distribution/inputs.lock.json", '{"ffmpegVersion":"8.0.3","zlibVersion":"1.3.1","nsisVersion":"3.13"}')
    foreach ($path in 'sources/ffmpeg-8.0.3/COPYING.LGPLv2.1', 'sources/lame-3.100/COPYING',
        'sources/lame-3.100/LICENSE', 'sources/zlib-1.3.1/README', 'build/nsis/nsis-3.13/COPYING',
        'source-bundle/AOM-LICENSE.txt', 'source-bundle/AOM-PATENTS.txt', 'source-bundle/AOM-fastfeat-LICENSE.txt',
        'source-bundle/AOM-vector-LICENSE.txt', 'source-bundle/AOM-x86inc-LICENSE.txt', 'source-bundle/VPX-LICENSE.txt',
        'source-bundle/VPX-PATENTS.txt', 'source-bundle/VPX-x86inc-ISC.txt') {
        New-Item -ItemType Directory -Force (Split-Path "$work/$path" -Parent) | Out-Null
        [IO.File]::WriteAllText("$work/$path", "native-notice:$path")
    }
    [IO.File]::WriteAllText("$temporary/assets/audio/FluidR3-LICENSE.txt", 'soundfont-notice')
    [IO.File]::WriteAllText("$work/licenses/fixture-MIT.txt", 'upstream-original-notice')
    $dependencies = @(@{name='fixture'; version='1.0'; selected='MIT'; repository='https://example.invalid'; notices=@(
        @{file='fixture-MIT.txt'; sha256=(Get-FileHash "$work/licenses/fixture-MIT.txt").Hash.ToLowerInvariant()}
    )})
    [IO.File]::WriteAllText("$work/licenses/rust-dependencies.json", (ConvertTo-Json -InputObject $dependencies -Depth 5))
    $audit = @{inputs=@(@{path='Cargo.toml'; sha256=(Get-FileHash "$temporary/Cargo.toml").Hash.ToLowerInvariant()});
        inventory_sha256=(Get-FileHash "$work/licenses/rust-dependencies.json").Hash.ToLowerInvariant()}
    [IO.File]::WriteAllText("$work/license-audit.json", ($audit | ConvertTo-Json -Depth 5))
    $names = @('dinuosi_media.dll', 'avcodec-62.dll', 'avformat-62.dll', 'avfilter-11.dll',
        'avutil-60.dll', 'swresample-6.dll', 'swscale-9.dll', 'ffmpeg.exe', 'ffprobe.exe')
    foreach ($name in $names) { [IO.File]::WriteAllText((Join-Path $source $name), "fixture:$name") }
    foreach ($profile in 'debug', 'release') {
        $destination = "$temporary/target/$profile/runtime/ffmpeg"
        New-Item -ItemType Directory -Force $destination | Out-Null
        foreach ($name in 'ffmpeg.exe', 'ffprobe.exe') { [IO.File]::WriteAllText("$destination/$name", 'stale-cli') }
    }
    Invoke-Deployment $true | Out-Null
    foreach ($profile in 'debug', 'debug/deps', 'release', 'release/deps') {
        $notices = Get-Content -LiteralPath "$temporary/target/$profile/licenses/THIRD_PARTY_NOTICES.txt" -Raw -Encoding UTF8
        if (!$notices.StartsWith("Dinuosi 0.1.0") -or !$notices.Contains('Dinuosi 主程序采用 MIT') -or
            !$notices.Contains('upstream-original-notice') -or !$notices.Contains('native-notice:sources/ffmpeg-8.0.3/COPYING.LGPLv2.1') -or
            !$notices.Contains((Get-Content "$root/LICENSE" -Raw -Encoding UTF8))) { throw "声明内容不完整：$profile" }
        if ((Get-FileHash "$temporary/target/$profile/LICENSE").Hash -ne (Get-FileHash "$root/LICENSE").Hash) {
            throw "主程序 MIT 原文不同：$profile"
        }
        foreach ($name in $names) {
            $destination = "$temporary/target/$profile/runtime/ffmpeg/$name"
            if ($name.EndsWith('.exe') -and !$profile.EndsWith('/deps')) {
                if (Test-Path -LiteralPath $destination) { throw "应用目录仍有 CLI：$destination" }
            } elseif ((Get-FileHash -LiteralPath $destination).Hash -ne (Get-FileHash -LiteralPath "$source/$name").Hash) {
                throw "部署内容不同：$destination"
            }
        }
    }
    & $shell -NoProfile -File "$temporary/tools/write-license-notices.ps1" -OutputFile "$temporary/package/licenses/THIRD_PARTY_NOTICES.txt"
    if ($LASTEXITCODE -ne 0 -or (Get-FileHash "$temporary/package/licenses/THIRD_PARTY_NOTICES.txt").Hash -ne
        (Get-FileHash "$temporary/target/debug/licenses/THIRD_PARTY_NOTICES.txt").Hash) { throw '开发与打包声明不同' }
    if ((Invoke-Deployment $false -Build) -notmatch '不能同时使用') { throw '冲突参数没有正确拒绝' }
    [IO.File]::WriteAllText("$source/avutil-60.dll", 'must-not-deploy')
    [IO.File]::WriteAllText("$temporary/Cargo.toml", 'version = "0.2.0"')
    if ((Invoke-Deployment $false) -notmatch 'Cargo 依赖输入已变更') { throw '过期许可审计没有正确拒绝' }
    [IO.File]::WriteAllText("$temporary/Cargo.toml", 'version = "0.1.0"')
    Remove-Item -LiteralPath "$work/licenses/fixture-MIT.txt"
    if ((Invoke-Deployment $false) -notmatch 'fixture-MIT.txt') { throw '缺许可原文没有正确拒绝' }
    if ([IO.File]::ReadAllText("$temporary/target/debug/runtime/ffmpeg/avutil-60.dll") -ne 'fixture:avutil-60.dll') {
        throw '许可失败仍部署了部分运行库'
    }
    Remove-Item -LiteralPath "$source/dinuosi_media.dll"
    [IO.File]::WriteAllText("$source/avutil-60.dll", 'must-not-deploy')
    if ((Invoke-Deployment $false) -notmatch 'dinuosi_media.dll') { throw '缺桥接库没有正确拒绝' }
    if ([IO.File]::ReadAllText("$temporary/target/debug/runtime/ffmpeg/avutil-60.dll") -ne 'fixture:avutil-60.dll') {
        throw '缺库失败仍写入了部分运行库'
    }
    Write-Output 'MIT/第三方原文部署、开发与打包声明一致、缺失或过期许可拒绝、CLI 隔离及缺库拒绝通过。'
} finally {
    Remove-Item -LiteralPath $temporary -Recurse -Force
}
