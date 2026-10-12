Unicode true
!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "nsDialogs.nsh"
!addplugindir /x86-unicode "${__FILEDIR__}\..\.local\distribution\build\installer-ui"
Name "Dinuosi"
OutFile "${OUTPUT}"
InstallDir "$LOCALAPPDATA\Programs\Dinuosi"
InstallDirRegKey HKCU "Software\Dinuosi\Installer" "InstallDir"
RequestExecutionLevel user
SetCompressor /SOLID zlib
ShowInstDetails hide
BrandingText "Dinuosi"
!define MUI_ICON "${__FILEDIR__}\..\assets\brand\dinuosi.ico"
!define MUI_UNICON "${__FILEDIR__}\..\assets\brand\dinuosi.ico"
!define MUI_LANGDLL_REGISTRY_ROOT HKCU
!define MUI_LANGDLL_REGISTRY_KEY "Software\Dinuosi\Installer"
!define MUI_LANGDLL_REGISTRY_VALUENAME "Language"
!define MUI_LANGDLL_ALWAYSSHOW
!define MUI_LANGDLL_ALLLANGUAGES
!define MUI_LANGDLL_WINDOWTITLE "Dinuosi - Language / 语言"
!define MUI_LANGDLL_INFO "Please select a language. / 请选择语言。"
!insertmacro MUI_PAGE_DIRECTORY
!define MUI_PAGE_CUSTOMFUNCTION_SHOW ShowInstallDetails
!define MUI_PAGE_CUSTOMFUNCTION_LEAVE LeaveInstallDetails
!insertmacro MUI_PAGE_INSTFILES
Page custom ShowCompletion CompleteInstallation
!define MUI_PAGE_CUSTOMFUNCTION_SHOW un.HideDirectory
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"
!insertmacro MUI_LANGUAGE "SimpChinese"
LangString CloseApplication ${LANG_ENGLISH} "Please close Dinuosi, then run the installer or uninstaller again."
LangString CloseApplication ${LANG_SIMPCHINESE} "请先关闭 Dinuosi，再重新运行安装或卸载程序。"
LangString UninstallShortcut ${LANG_ENGLISH} "Uninstall"
LangString UninstallShortcut ${LANG_SIMPCHINESE} "卸载"
LangString ShowDetails ${LANG_ENGLISH} "Show details"
LangString ShowDetails ${LANG_SIMPCHINESE} "显示详情"
LangString HideDetails ${LANG_ENGLISH} "Hide details"
LangString HideDetails ${LANG_SIMPCHINESE} "收起详情"
LangString DetailsError ${LANG_ENGLISH} "Unable to initialize installation details."
LangString DetailsError ${LANG_SIMPCHINESE} "无法初始化安装详情。"
LangString CompletionTitle ${LANG_ENGLISH} "Installation complete"
LangString CompletionTitle ${LANG_SIMPCHINESE} "安装完成"
LangString CompletionText ${LANG_ENGLISH} "Dinuosi has been installed."
LangString CompletionText ${LANG_SIMPCHINESE} "Dinuosi 已安装完成。"
LangString DesktopShortcut ${LANG_ENGLISH} "Create a desktop shortcut"
LangString DesktopShortcut ${LANG_SIMPCHINESE} "创建桌面快捷方式"
LangString RunApplication ${LANG_ENGLISH} "Run Dinuosi now"
LangString RunApplication ${LANG_SIMPCHINESE} "立即运行 Dinuosi"
LangString FinishButton ${LANG_ENGLISH} "Finish"
LangString FinishButton ${LANG_SIMPCHINESE} "完成"
LangString ShortcutError ${LANG_ENGLISH} "Unable to create the desktop shortcut."
LangString ShortcutError ${LANG_SIMPCHINESE} "无法创建桌面快捷方式。"
LangString LaunchError ${LANG_ENGLISH} "Unable to start Dinuosi."
LangString LaunchError ${LANG_SIMPCHINESE} "无法启动 Dinuosi。"
Var DesktopShortcutCheckbox
Var RunApplicationCheckbox

; 安装进度页初始化双向详情按钮；插件仅接管该按钮点击，失败时报告错误并中止。
Function ShowInstallDetails
    DinuosiInstaller::Attach $mui.InstFilesPage "$(ShowDetails)" "$(HideDetails)"
    Pop $0
    ${If} $0 != "ok"
        MessageBox MB_OK|MB_ICONSTOP "$(DetailsError)"
        Abort
    ${EndIf}
FunctionEnd

; 离开安装进度页时解除按钮事件挂接，随后保留原有 MUI 页尾及语言登记行为。
Function LeaveInstallDetails
    DinuosiInstaller::Detach
FunctionEnd

; 绘制安装完成页与两个默认勾选项；显示时不创建快捷方式或启动软件，静默安装跳过此页。
Function ShowCompletion
    !insertmacro MUI_HEADER_TEXT "$(CompletionTitle)" ""
    nsDialogs::Create 1018
    Pop $0
    ${If} $0 == "error"
        Abort
    ${EndIf}
    ${NSD_CreateLabel} 0 0 100% 18u "$(CompletionText)"
    Pop $0
    ${NSD_CreateCheckbox} 0 28u 100% 12u "$(DesktopShortcut)"
    Pop $DesktopShortcutCheckbox
    ${NSD_Check} $DesktopShortcutCheckbox
    ${NSD_CreateCheckbox} 0 48u 100% 12u "$(RunApplication)"
    Pop $RunApplicationCheckbox
    ${NSD_Check} $RunApplicationCheckbox
    GetDlgItem $0 $HWNDPARENT 1
    SendMessage $0 ${WM_SETTEXT} 0 "STR:$(FinishButton)"
    nsDialogs::Show
FunctionEnd

; 点击完成后按勾选状态创建当前用户桌面快捷方式并启动程序；操作失败保留完成页供改选或重试。
Function CompleteInstallation
    ${NSD_GetState} $DesktopShortcutCheckbox $0
    ${If} $0 == ${BST_CHECKED}
        ClearErrors
        CreateShortCut "$DESKTOP\Dinuosi.lnk" "$INSTDIR\dinuosi.exe"
        ${If} ${Errors}
            MessageBox MB_OK|MB_ICONSTOP "$(ShortcutError)"
            Abort
        ${EndIf}
        WriteRegDWORD HKCU "Software\Dinuosi\Installer" "DesktopShortcutCreated" 1
    ${EndIf}
    ${NSD_GetState} $RunApplicationCheckbox $0
    ${If} $0 == ${BST_CHECKED}
        SetOutPath "$INSTDIR"
        ClearErrors
        Exec '"$INSTDIR\dinuosi.exe"'
        ${If} ${Errors}
            MessageBox MB_OK|MB_ICONSTOP "$(LaunchError)"
            Abort
        ${EndIf}
    ${EndIf}
FunctionEnd

; 卸载确认页只隐藏目录标签和路径框，不修改实际安装目录或文件删除范围。
Function un.HideDirectory
    ShowWindow $mui.UnConfirmPage.DirectoryText ${SW_HIDE}
    ShowWindow $mui.UnConfirmPage.Directory ${SW_HIDE}
FunctionEnd

; 初始化安装器语言；交互运行允许选择，静默运行沿用已登记语言或系统匹配结果，不提前写登记。
Function .onInit
    !insertmacro MUI_LANGDLL_DISPLAY
FunctionEnd

; 在写入前检查现有主程序能否写入；正在运行时提示关闭，不结束用户进程。
!macro RequireClosed
    IfFileExists "$INSTDIR\dinuosi.exe" 0 ready
    System::Call 'kernel32::CreateFileW(w "$INSTDIR\dinuosi.exe", i 0x40000000, i 3, p 0, i 3, i 0, p 0) p.r0'
    ${If} $0 == -1
        MessageBox MB_OK|MB_ICONEXCLAMATION "$(CloseApplication)" /SD IDOK
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

; 从当前用户安装登记恢复语言，再检查文件占用；占用时中止卸载并保留进程和安装内容。
Function un.onInit
    !insertmacro MUI_UNGETLANGUAGE
    !insertmacro RequireClosed
FunctionEnd

Section
    Call CheckApplicationClosed
    SetOutPath "$INSTDIR"
    File /r "${STAGE}\*"
    WriteUninstaller "$INSTDIR\uninstall.exe"
    CreateDirectory "$SMPROGRAMS\Dinuosi"
    CreateShortCut "$SMPROGRAMS\Dinuosi\Dinuosi.lnk" "$INSTDIR\dinuosi.exe"
    Delete "$SMPROGRAMS\Dinuosi\卸载.lnk"
    Delete "$SMPROGRAMS\Dinuosi\Uninstall.lnk"
    CreateShortCut "$SMPROGRAMS\Dinuosi\$(UninstallShortcut).lnk" "$INSTDIR\uninstall.exe"
    WriteRegStr HKCU "Software\Dinuosi\Installer" "Language" "$LANGUAGE"
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
    Delete "$SMPROGRAMS\Dinuosi\Uninstall.lnk"
    ReadRegDWORD $0 HKCU "Software\Dinuosi\Installer" "DesktopShortcutCreated"
    ${If} $0 == 1
        Delete "$DESKTOP\Dinuosi.lnk"
    ${EndIf}
    RMDir "$SMPROGRAMS\Dinuosi"
    DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\Dinuosi"
    DeleteRegKey HKCU "Software\Dinuosi\Installer"
    RMDir "$INSTDIR"
SectionEnd
