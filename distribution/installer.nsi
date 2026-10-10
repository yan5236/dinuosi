Unicode true
!include "MUI2.nsh"
!include "LogicLib.nsh"
Name "Dinuosi"
OutFile "${OUTPUT}"
InstallDir "$LOCALAPPDATA\Programs\Dinuosi"
InstallDirRegKey HKCU "Software\Dinuosi\Installer" "InstallDir"
RequestExecutionLevel user
SetCompressor /SOLID zlib
ShowInstDetails show
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "SimpChinese"

; 在写入前检查现有主程序能否写入；正在运行时提示关闭，不结束用户进程。
!macro RequireClosed
    IfFileExists "$INSTDIR\dinuosi.exe" 0 ready
    System::Call 'kernel32::CreateFileW(w "$INSTDIR\dinuosi.exe", i 0x40000000, i 3, p 0, i 3, i 0, p 0) p.r0'
    ${If} $0 == -1
        MessageBox MB_OK|MB_ICONEXCLAMATION "请先关闭 Dinuosi，再重新运行安装程序。" /SD IDOK
        SetErrorLevel 2
        Abort
    ${EndIf}
    System::Call 'kernel32::CloseHandle(p r0)'
    ready:
!macroend

; 安装前检查当前目标，错误时不结束用户程序。
Function CheckApplicationClosed
    !insertmacro RequireClosed
FunctionEnd

; 卸载前同样检查文件占用，防止留下部分安装内容。
Function un.onInit
    !insertmacro RequireClosed
FunctionEnd

Section
    Call CheckApplicationClosed
    SetOutPath "$INSTDIR"
    File /r "${STAGE}\*"
    WriteUninstaller "$INSTDIR\uninstall.exe"
    CreateDirectory "$SMPROGRAMS\Dinuosi"
    CreateShortCut "$SMPROGRAMS\Dinuosi\Dinuosi.lnk" "$INSTDIR\dinuosi.exe"
    CreateShortCut "$SMPROGRAMS\Dinuosi\卸载.lnk" "$INSTDIR\uninstall.exe"
    WriteRegStr HKCU "Software\Dinuosi\Installer" "InstallDir" "$INSTDIR"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Dinuosi" "DisplayName" "Dinuosi"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Dinuosi" "DisplayVersion" "${VERSION}"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Dinuosi" "UninstallString" '"$INSTDIR\uninstall.exe"'
SectionEnd

; 仅删除本包清单中的文件；目录采用非递归删除，未知文件、项目和用户数据保留。
Section "Uninstall"
    !include "${REMOVAL_LIST}"
    Delete "$INSTDIR\uninstall.exe"
    Delete "$SMPROGRAMS\Dinuosi\Dinuosi.lnk"
    Delete "$SMPROGRAMS\Dinuosi\卸载.lnk"
    RMDir "$SMPROGRAMS\Dinuosi"
    DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Dinuosi"
    DeleteRegKey HKCU "Software\Dinuosi\Installer"
    RMDir "$INSTDIR"
SectionEnd