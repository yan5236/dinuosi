# 对已生成的内部验收包执行真实安装、升级、运行占用保护、卸载和 ZIP 一致性检查。
# 仅使用项目内独占测试目录；已有正式安装或快捷方式时拒绝改动，不触碰用户项目。
param([string]$Artifacts, [ValidateSet('English','SimpChinese')][string]$Language = 'English')
$ErrorActionPreference='Stop'
$root=Split-Path $PSScriptRoot -Parent
if(!$Artifacts){$Artifacts=Join-Path $root 'target/distribution'}
$qa=Join-Path $root ".local/distribution/qa/package-$([guid]::NewGuid().ToString('N'))"
$installation=Join-Path $qa '安装 中文 目录'
$portable=Join-Path $qa '便携 中文 目录'
$data=Join-Path $qa 'user-data'
$knownKey='HKCU:/Software/Dinuosi/Installer'
if(Test-Path $knownKey){throw '存在安装登记，拒绝覆盖可能的正式安装'}
$shortcut=Join-Path ([Environment]::GetFolderPath('Programs')) 'Dinuosi'
if(Test-Path $shortcut){throw '存在开始菜单目录，拒绝覆盖已有快捷方式'}
$desktopShortcut=Join-Path ([Environment]::GetFolderPath('Desktop')) 'Dinuosi.lnk'
if(Test-Path -LiteralPath $desktopShortcut){throw '存在桌面快捷方式，拒绝改动已有安装'}
New-Item -ItemType Directory -Force $qa,$data | Out-Null
$setup=(Get-ChildItem $Artifacts -Filter '*-setup.exe').FullName
$zip=(Get-ChildItem $Artifacts -Filter '*-windows-x64.zip').FullName
if(!$setup -or !$zip){throw '缺少验收安装包或便携包'}
$languageId = if ($Language -eq 'SimpChinese') { '2052' } else { '1033' }
$uninstallShortcut = if ($Language -eq 'SimpChinese') { '卸载.lnk' } else { 'Uninstall.lnk' }
$record=[ordered]@{qa=$qa; installation=$installation; portable=$portable; language=$Language; checks=@(); code_signed=$false}

# 使用脚本选定的安装器及独立目标目录静默安装，返回实际退出码供调用方断言。
# 会写当前用户安装登记与快捷方式；等待超时抛错，不结束安装器或正在使用的主程序。
function Install-TestPackage {
    $process=Start-Process -FilePath $setup -ArgumentList "/S /D=$installation" -WindowStyle Hidden -PassThru
    if(!$process.WaitForExit(60000)){throw '安装器未在一分钟内退出'}
    return $process.ExitCode
}

# directory 为已安装或提取的应用目录；逐项核对 package-files.json 记录的 SHA-256。
# 逐项检查必要动态库与许可材料、拒绝测试 CLI 并核对全部文件哈希；不限制用户额外文件。
# 成功返回无值；清单/必要文件缺失、混入 CLI 或哈希不符时抛错。
function Assert-Inventory($directory) {
    foreach ($path in 'LICENSE', 'licenses/THIRD_PARTY_NOTICES.txt', 'runtime/ffmpeg/dinuosi_media.dll',
        'runtime/ffmpeg/avcodec-62.dll', 'runtime/ffmpeg/avformat-62.dll', 'runtime/ffmpeg/avfilter-11.dll',
        'runtime/ffmpeg/avutil-60.dll', 'runtime/ffmpeg/swresample-6.dll', 'runtime/ffmpeg/swscale-9.dll') {
        if (!(Test-Path -LiteralPath (Join-Path $directory $path) -PathType Leaf)) { throw "包内缺少必要文件：$path" }
    }
    foreach ($name in 'ffmpeg.exe', 'ffprobe.exe') {
        if (Test-Path -LiteralPath (Join-Path $directory "runtime/ffmpeg/$name")) { throw "包内不应包含测试 CLI：$name" }
    }
    foreach($file in Get-Content (Join-Path $directory 'package-files.json') -Raw|ConvertFrom-Json){
        $path=Join-Path $directory $file.path
        if((Get-FileHash -LiteralPath $path).Hash.ToLowerInvariant() -ne $file.sha256){throw "安装内容不一致：$($file.path)"}
    }
}

# 在已确认不存在正式登记的独占验收中预设语言，检验静默安装及卸载复用 MUI 的语言登记。
New-Item -Path $knownKey -Force | Out-Null
New-ItemProperty -Path $knownKey -Name 'Language' -Value $languageId -PropertyType String -Force | Out-Null
if((Install-TestPackage) -ne 0){throw '首次安装失败'}
if(Test-Path -LiteralPath $desktopShortcut){throw '静默安装不应触发完成页桌面快捷方式选项'}
Assert-Inventory $installation
$record.checks+='install_inventory'
if(!(Test-Path (Join-Path $shortcut 'Dinuosi.lnk'))){throw '开始菜单快捷方式缺失'}
if (!(Test-Path (Join-Path $shortcut $uninstallShortcut)) -or
    (Get-ItemPropertyValue -Path $knownKey -Name 'Language') -ne $languageId) { throw '安装语言或本地化卸载快捷方式不符' }
$record.checks+='selected_language_and_localized_uninstall_shortcut'
if(!(Test-Path 'HKCU:/Software/Microsoft/Windows/CurrentVersion/Uninstall/Dinuosi')){throw '卸载登记缺失'}
$record.checks+='per_user_shortcut_and_uninstall_registration'
$unknown=Join-Path $installation '用户保留.txt'
[IO.File]::WriteAllText($unknown,'必须在升级和卸载后保留',[Text.UTF8Encoding]::new($false))
$project=Join-Path $qa '独立项目.txt'
[IO.File]::WriteAllText($project,'外部项目不能删除',[Text.UTF8Encoding]::new($false))
$projectHash=(Get-FileHash $project).Hash
# 隔离只作用于本轮验收进程；无论成功或异常，都恢复调用者环境，不能破坏后续开发命令。
$environment=@{}
foreach($name in 'PATH','DINUOSI_DATA_DIR','HTTP_PROXY','HTTPS_PROXY'){
    $environment[$name]=[Environment]::GetEnvironmentVariable($name,'Process')
}
try {
$env:DINUOSI_DATA_DIR=$data
$env:PATH="$env:SystemRoot\System32;$env:SystemRoot"
$env:HTTP_PROXY='http://127.0.0.1:9'
$env:HTTPS_PROXY='http://127.0.0.1:9'
$program=Start-Process -FilePath (Join-Path $installation 'dinuosi.exe') -WorkingDirectory $qa -WindowStyle Normal -PassThru
Start-Sleep -Seconds 3
if($program.HasExited){throw '已安装程序启动即退出'}
if((Install-TestPackage) -ne 2){throw '未阻止运行中升级'}
$record.checks+='running_upgrade_rejected_without_killing_application'
if($program.HasExited){throw '升级器结束了主程序'}
if(!$program.CloseMainWindow() -or !$program.WaitForExit(15000)){throw '验收程序正常关闭失败'}
if((Install-TestPackage) -ne 0){throw '关闭后的升级失败'}
Assert-Inventory $installation
if(!(Test-Path $unknown)){throw '升级删除了未知文件'}
$record.checks+='upgrade_inventory_and_unknown_file_preservation'
Expand-Archive -LiteralPath $zip -DestinationPath $portable
Assert-Inventory $portable
$record.checks+='portable_matches_installer_payload'
$program=Start-Process -FilePath (Join-Path $portable 'dinuosi.exe') -WorkingDirectory $qa -WindowStyle Normal -PassThru
Start-Sleep -Seconds 3
if($program.HasExited){throw '便携程序启动即退出'}
if(!$program.CloseMainWindow() -or !$program.WaitForExit(15000)){throw '便携程序正常关闭失败'}
$record.checks+='portable_runs_without_external_path'
$uninstaller=Start-Process -FilePath (Join-Path $installation 'uninstall.exe') -ArgumentList '/S' -WindowStyle Hidden -PassThru
if(!$uninstaller.WaitForExit(60000)){throw '卸载器未退出'}
# NSIS 卸载器可能复制到临时目录执行，等待原目录主程序确实移除。
$deadline=[DateTime]::UtcNow.AddSeconds(20)
while(((Test-Path (Join-Path $installation 'dinuosi.exe')) -or (Test-Path $knownKey)) -and [DateTime]::UtcNow -lt $deadline){Start-Sleep -Milliseconds 100}
if(Test-Path (Join-Path $installation 'dinuosi.exe')){throw '卸载未移除主程序'}
if(!(Test-Path $unknown) -or (Get-FileHash $project).Hash -ne $projectHash){throw '卸载破坏了未知文件或外部项目'}
if(!(Test-Path $data)){throw '卸载删除了用户数据目录'}
if(Test-Path $knownKey){throw '卸载未清理安装登记'}
$record.checks+='uninstall_preserves_unknown_files_projects_and_user_data'
[IO.File]::WriteAllText((Join-Path $root ".local/distribution/qa/package-verification-$Language.json"),($record|ConvertTo-Json -Depth 4),[Text.UTF8Encoding]::new($false))
Write-Output ($record|ConvertTo-Json -Depth 4)
} finally {
    foreach($name in $environment.Keys){[Environment]::SetEnvironmentVariable($name,$environment[$name],'Process')}
}
