; ZannenToolbox NSIS 钩子（由 tauri.conf.json bundle.windows.nsis.installerHooks 引入）
;
; 职责：
; 1. PREINIT：读注册表卸载项，识别"全新安装 / 升级安装"，写入日志
; 2. POSTINSTALL：向安装目录落一份安装状态文件，供应用首启页做差异化动效
;
; Tauri NSIS 模板本身会在检测到旧版时先卸载（静默升级）；这里的钩子补充
; 识别与用户态标记。

Var /Global ZannenInstallKind
Var /Global ZannenPrevVersion

!macro NSIS_HOOK_PREINIT
  StrCpy $ZannenInstallKind "fresh"
  StrCpy $ZannenPrevVersion ""

  ; 系统外观检测（深浅色）：NSIS 为 Win32 经典 UI 无法整体换肤，
  ; 记录检测结果供安装日志与后续自定义页使用。
  ReadRegDWORD $0 HKCU "Software\Microsoft\Windows\CurrentVersion\Themes\Personalize" "AppsUseLightTheme"
  ${If} $0 == 0
    DetailPrint "System appearance: dark"
  ${Else}
    DetailPrint "System appearance: light"
  ${EndIf}

  ; currentUser 模式写在 HKCU，perMachine 模式写在 HKLM，两处都查
  ReadRegStr $ZannenPrevVersion HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\ZannenToolbox" "DisplayVersion"
  ${If} $ZannenPrevVersion != ""
    StrCpy $ZannenInstallKind "upgrade"
  ${Else}
    ReadRegStr $ZannenPrevVersion HKLM "Software\Microsoft\Windows\CurrentVersion\Uninstall\ZannenToolbox" "DisplayVersion"
    ${If} $ZannenPrevVersion != ""
      StrCpy $ZannenInstallKind "upgrade"
    ${EndIf}
  ${EndIf}

  DetailPrint "ZannenToolbox install kind: $ZannenInstallKind (previous: $ZannenPrevVersion)"
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ; 安装状态文件：应用首次启动时读取以决定引导页/升级页
  FileOpen $0 "$INSTDIR\.zannen-install-state" w
  FileWrite $0 '{"install_kind":"$ZannenInstallKind","prev_version":"$ZannenPrevVersion"}'
  FileWriteByte $0 "10"
  FileClose $0
!macroend
