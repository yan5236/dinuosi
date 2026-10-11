# 临时移走或改写当前项目的发行缓存，验证缺失源码/许可、哈希不符和过期审计均拒绝打包。
# 每个场景在 finally 中恢复缓存；需使用已准备的独立开发环境，不与其他打包任务并行运行。
$ErrorActionPreference='Stop'
$root=Split-Path $PSScriptRoot -Parent
$work=Join-Path $root '.local/distribution'
$negative=Join-Path $work "qa/rejected-package-$([guid]::NewGuid().ToString('N'))"

# expected 为预期错误消息的正则；运行打包并断言匹配该错误，且独占输出目录未生成 EXE。
# 成功返回无值；意外成功、错误原因不符或产生安装程序时抛错，可能保留失败的测试输出。
function Assert-Rejected([string]$expected) {
    $failure=$null
    try { & "$PSScriptRoot/package.ps1" -OutputDirectory $negative }
    catch { $failure=$_.Exception.Message }
    if(!$failure -or $failure -notmatch $expected){throw "错误拒绝原因：$failure；预期：$expected"}
    if(Test-Path $negative){
        if(Get-ChildItem $negative -Filter '*.exe'){throw '拒绝打包后仍生成安装程序'}
    }
}

$source=Join-Path $work 'source-bundle/ffmpeg-8.0.3.tar.xz'
Move-Item -LiteralPath $source -Destination "$source.withheld"
try { Assert-Rejected 'ffmpeg-8\.0\.3\.tar\.xz' }
finally { Move-Item -LiteralPath "$source.withheld" -Destination $source }

# 新编码器必须同时交付版权许可与专利授权；分别缺失时拒绝生成任何安装程序。
foreach ($codecNotice in 'AOM-LICENSE.txt','VPX-PATENTS.txt') {
    $source=Join-Path $work "source-bundle/$codecNotice"
    Move-Item -LiteralPath $source -Destination "$source.withheld"
    try { Assert-Rejected ([regex]::Escape($codecNotice)) }
    finally { Move-Item -LiteralPath "$source.withheld" -Destination $source }
}

$dependencies=Get-Content "$work/licenses/rust-dependencies.json" -Raw|ConvertFrom-Json
$notice=Join-Path $work "licenses/$($dependencies[0].notices[0].file)"
Move-Item -LiteralPath $notice -Destination "$notice.withheld"
try { Assert-Rejected ([regex]::Escape($dependencies[0].notices[0].file)) }
finally { Move-Item -LiteralPath "$notice.withheld" -Destination $notice }

$manifest=Join-Path $work 'runtime-manifest.json'
$original=[IO.File]::ReadAllText($manifest)
try {
    $data=$original|ConvertFrom-Json
    $data.files[0].sha256='0'*64
    [IO.File]::WriteAllText($manifest,($data|ConvertTo-Json -Depth 6),[Text.UTF8Encoding]::new($false))
    Assert-Rejected '运行时已变更'
} finally { [IO.File]::WriteAllText($manifest,$original,[Text.UTF8Encoding]::new($false)) }

$audit=Join-Path $work 'license-audit.json'
$original=[IO.File]::ReadAllText($audit)
try {
    $data=$original|ConvertFrom-Json
    $data.inputs[0].sha256='0'*64
    [IO.File]::WriteAllText($audit,($data|ConvertTo-Json -Depth 6),[Text.UTF8Encoding]::new($false))
    Assert-Rejected 'Cargo 依赖输入已变更'
} finally { [IO.File]::WriteAllText($audit,$original,[Text.UTF8Encoding]::new($false)) }
Write-Output '缺失源码/许可/编码库声明、运行时校验失败与过期审计六项拒绝边界通过。'
