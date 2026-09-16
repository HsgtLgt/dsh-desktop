@echo off
chcp 65001 >nul
setlocal
set "DIR=%LOCALAPPDATA%\Programs\DeepSeek Harness"
echo 正在卸载 DeepSeek Harness ...
taskkill /IM "DeepSeek Harness.exe" /F >nul 2>nul
timeout /t 2 /nobreak >nul
del "%USERPROFILE%\Desktop\DeepSeek Harness.lnk" >nul 2>nul
del "%APPDATA%\Microsoft\Windows\Start Menu\Programs\DeepSeek Harness.lnk" >nul 2>nul
reg delete "HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\DeepSeekHarness" /f >nul 2>nul
if exist "%DIR%" robocopy "%DIR%" "%TEMP%\dsh-uninstall-empty" /MIR /NFL /NDL /NJH /NJS /NP /R:0 /W:0 >nul 2>nul
if exist "%DIR%" rmdir /s /q "%DIR%" >nul 2>nul
rmdir /s /q "%TEMP%\dsh-uninstall-empty" >nul 2>nul
echo 卸载完成。
timeout /t 2 /nobreak >nul
