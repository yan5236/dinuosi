# 使用隔离缓存验证 Windows 部署：应用只带 DLL、测试保留 CLI、缺库时拒绝写入。
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$temporary = [IO.Path]::GetFullPath((Join-Path $root ".local/media-deployment-$([guid]::NewGuid().ToString('N'))"))
$allowed = [IO.Path]::GetFullPath((Join-Path $root '.local')) + [IO.Path]::DirectorySeparatorChar
if (!$temporary.StartsWith($allowed, [StringComparison]::OrdinalIgnoreCase)) { throw '测试目录越界' }
$source = Join-Path $temporary '.local/distribution/runtime/ffmpeg/bin'
New-Item -ItemType Directory -Force $source, "$temporary/tools" | Out-Null
Copy-Item -LiteralPath "$PSScriptRoot/prepare-media.ps1" -Destination "$temporary/tools"
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
        foreach ($name in $names) {
            $destination = "$temporary/target/$profile/runtime/ffmpeg/$name"
            if ($name.EndsWith('.exe') -and !$profile.EndsWith('/deps')) {
                if (Test-Path -LiteralPath $destination) { throw "应用目录仍有 CLI：$destination" }
            } elseif ((Get-FileHash -LiteralPath $destination).Hash -ne (Get-FileHash -LiteralPath "$source/$name").Hash) {
                throw "部署内容不同：$destination"
            }
        }
    }
    if ((Invoke-Deployment $false -Build) -notmatch '不能同时使用') { throw '冲突参数没有正确拒绝' }
    Remove-Item -LiteralPath "$source/dinuosi_media.dll"
    [IO.File]::WriteAllText("$source/avutil-60.dll", 'must-not-deploy')
    if ((Invoke-Deployment $false) -notmatch 'dinuosi_media.dll') { throw '缺桥接库没有正确拒绝' }
    if ([IO.File]::ReadAllText("$temporary/target/debug/runtime/ffmpeg/avutil-60.dll") -ne 'fixture:avutil-60.dll') {
        throw '缺库失败仍写入了部分运行库'
    }
    Write-Output '离线部署、应用无 CLI、测试工具隔离、旧 CLI 清理、冲突参数及缺库拒绝通过。'
} finally {
    Remove-Item -LiteralPath $temporary -Recurse -Force
}
