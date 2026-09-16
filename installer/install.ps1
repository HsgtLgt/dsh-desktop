# 日志
Start-Transcript -Path (Join-Path $PSScriptRoot 'install-transcript.log') -Force | Out-Null
# DeepSeek Harness 安装脚本（由 setup.exe 在解压后调用；本文件与 win-unpacked 同级）
$ErrorActionPreference = 'Stop'
$Root = $PSScriptRoot
$Source = Join-Path $Root 'win-unpacked'
if (-not (Test-Path $Source)) { throw "安装包内容缺失: $Source" }

$InstallDir = Join-Path $env:LOCALAPPDATA 'Programs\DeepSeek Harness'
Write-Host ''
Write-Host '安装目录: ' $InstallDir

# 退出正在运行的实例；旧文件交给 robocopy /MIR 处理（cmd rmdir 遇含空格路径会失败）
Get-Process 'DeepSeek Harness' -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 2
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null

# 用 robocopy 复制：原生支持超长路径、带重试、不弹进度
Write-Host '正在写入安装目录（约 930MB，需要几分钟）...'
# 路径可能含空格：Start-Process 的 ArgumentList 需要显式加引号
# 记录 robocopy 完整日志，便于诊断漏拷
$copyLog = Join-Path $Root 'copy.log'
$rc = Start-Process -FilePath 'robocopy.exe' -ArgumentList @('"' + $Source + '"', '"' + $InstallDir + '"', '/E', '/R:1', '/W:1', '/NP', '/LOG:' + $copyLog) -Wait -PassThru -NoNewWindow
Write-Host ('robocopy 退出码=' + $rc.ExitCode)
if (Test-Path $copyLog) { Write-Host '--- copy.log 尾部 ---'; Get-Content $copyLog -Tail 25 | ForEach-Object { Write-Host $_ } }
if ($rc.ExitCode -ge 8) { throw "复制失败，robocopy 退出码 $($rc.ExitCode)" }

# 卸载脚本放在应用目录之外的包根，需单独复制
Copy-Item -Path (Join-Path $Root 'uninstall.cmd') -Destination $InstallDir -Force

$Exe = Join-Path $InstallDir 'DeepSeek Harness.exe'
if (-not (Test-Path $Exe)) { throw '安装失败：主程序未就位' }

# 桌面 + 开始菜单快捷方式
$shell = New-Object -ComObject WScript.Shell
$desktop = [Environment]::GetFolderPath('Desktop')
$startMenu = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs'
foreach ($dir in @($desktop, $startMenu)) {
  $lnk = $shell.CreateShortcut((Join-Path $dir 'DeepSeek Harness.lnk'))
  $lnk.TargetPath = $Exe
  $lnk.WorkingDirectory = $InstallDir
  $lnk.IconLocation = $Exe
  $lnk.Description = 'DeepSeek Harness 桌面版'
  $lnk.Save()
}

# 注册到"应用和功能"
$key = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\DeepSeekHarness'
New-Item -Path $key -Force | Out-Null
Set-ItemProperty -Path $key -Name DisplayName -Value 'DeepSeek Harness'
Set-ItemProperty -Path $key -Name DisplayVersion -Value '0.1.6-alpha.1'
Set-ItemProperty -Path $key -Name Publisher -Value 'DeepSeek'
Set-ItemProperty -Path $key -Name InstallLocation -Value $InstallDir
Set-ItemProperty -Path $key -Name UninstallString -Value ('"' + (Join-Path $InstallDir 'uninstall.cmd') + '"')
Set-ItemProperty -Path $key -Name NoModify -Value 1 -Type DWord
Set-ItemProperty -Path $key -Name NoRepair -Value 1 -Type DWord

Write-Host ''
Write-Host '安装完成，正在启动 DeepSeek Harness ...'
Start-Process -FilePath $Exe -WorkingDirectory $InstallDir

Stop-Transcript | Out-Null
