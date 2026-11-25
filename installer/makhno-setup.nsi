; MakhnoYMMod 2.2.3 — установщик «Яндекс Музыка 5.128.0 + мод» (Windows x64)
; Сборка: makensis (Linux) из каталога installer/ (рядом — каталог app/)
; Требуется NSIS 3.x с MUI2. Скрипт в UTF-8 (с BOM при сборке вручную).

Unicode true
ManifestDPIAware true

!define MOD_NAME    "MakhnoYMMod"
!define MOD_VERSION "2.2.3"
!define APP_NAME    "Яндекс Музыка"
!define APP_VERSION "5.128.0"
!define APP_EXE     "Яндекс Музыка.exe"
!define PUBLISHER   "Makhno"
!define URL_REPO    "https://github.com/Ortodoxian88/MakhnoYMMod"

Name "${MOD_NAME} ${MOD_VERSION} (${APP_NAME} ${APP_VERSION})"
OutFile "MakhnoYMMod-Setup-${MOD_VERSION}-win-x64.exe"

; установка per-user: права администратора не нужны
InstallDir "$LOCALAPPDATA\Programs\${MOD_NAME}"
InstallDirRegKey HKCU "Software\${MOD_NAME}" "InstallDir"
RequestExecutionLevel user

; повторная установка поверх своей же — в тот же каталог
VIAddVersionKey "FileDescription" "Установщик ${MOD_NAME} ${MOD_VERSION} — ${APP_NAME} ${APP_VERSION} + мод"
VIAddVersionKey "FileVersion" "2.2.3.0"
VIAddVersionKey "LegalCopyright" "MIT © Makhno; десктоп-клиент © Яндекс"
VIProductVersion "2.2.3.0"
VIFileVersion "2.2.3.0"

SetCompressor /SOLID lzma
SetCompressorDictSize 96

Icon "icons\makhno.ico"
UninstallIcon "icons\makhno.ico"
BrandingText "${MOD_NAME} ${MOD_VERSION}"

!include "MUI2.nsh"
!include "FileFunc.nsh"
!include "LogicLib.nsh"

!define MUI_ABORTWARNING
!define MUI_ICON "icons\makhno.ico"
!define MUI_UNICON "icons\makhno.ico"
!define MUI_FINISHPAGE_RUN "$INSTDIR\${APP_EXE}"
!define MUI_FINISHPAGE_RUN_TEXT "Запустить ${APP_NAME} с модом"
!define MUI_FINISHPAGE_LINK "Репозиторий ${MOD_NAME}"
!define MUI_FINISHPAGE_LINK_LOCATION "${URL_REPO}"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "license.txt"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "Russian"
!insertmacro MUI_LANGUAGE "English"

; ---------------------------------------------------------------- скрипт ---

Section "${MOD_NAME} (${APP_NAME} + мод)" SecCore
  SectionIn RO

  ; если приложение из НАШЕГО каталога запущено — просим закрыть,
  ; иначе файлы будут заблокированы и копирование сорвётся
  IfFileExists "$INSTDIR\${APP_EXE}" 0 +3
    MessageBox MB_OKCANCEL|MB_ICONINFORMATION \
      "Перед установкой закройте ${APP_NAME}.$\n$\nПродолжить установку?" IDOK +2
    Abort

  SetOutPath "$INSTDIR"
  ; всё дерево клиента с модом: содержимое app/ кладём прямо в $INSTDIR
  ; (File /r "app" создал бы вложенный каталог app внутри $INSTDIR)
  File /r "app\*"

  ; деинсталлятор
  WriteUninstaller "$INSTDIR\Uninstall.exe"

  ; меню «Пуск»
  CreateDirectory "$SMPROGRAMS\${MOD_NAME}"
  CreateShortCut "$SMPROGRAMS\${MOD_NAME}\${MOD_NAME} (${APP_NAME}).lnk" "$INSTDIR\${APP_EXE}"
  CreateShortCut "$SMPROGRAMS\${MOD_NAME}\Удалить ${MOD_NAME}.lnk" "$INSTDIR\Uninstall.exe"

  ; реестр
  WriteRegStr HKCU "Software\${MOD_NAME}" "InstallDir" "$INSTDIR"
  WriteRegStr HKCU "Software\${MOD_NAME}" "ModVersion" "${MOD_VERSION}"
  WriteRegStr HKCU "Software\${MOD_NAME}" "AppVersion" "${APP_VERSION}"

  ; запись в «Программы и компоненты»
  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  IntFmt $3 "0x%08XKB" $0
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${MOD_NAME}" \
    "DisplayName" "${MOD_NAME} ${MOD_VERSION} (${APP_NAME} ${APP_VERSION})"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${MOD_NAME}" \
    "DisplayVersion" "${MOD_VERSION}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${MOD_NAME}" \
    "DisplayIcon" "$INSTDIR\${APP_EXE}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${MOD_NAME}" \
    "UninstallString" "$INSTDIR\Uninstall.exe"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${MOD_NAME}" \
    "QuietUninstallString" "$\"$INSTDIR\Uninstall.exe$\" /S"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${MOD_NAME}" \
    "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${MOD_NAME}" \
    "Publisher" "${PUBLISHER}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${MOD_NAME}" \
    "URLInfoAbout" "${URL_REPO}"
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${MOD_NAME}" \
    "EstimatedSize" $3
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${MOD_NAME}" \
    "NoModify" 1
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${MOD_NAME}" \
    "NoRepair" 1
SectionEnd

Section /o "Ярлык на рабочем столе" SecDesktop
  CreateShortCut "$DESKTOP\${MOD_NAME} (${APP_NAME}).lnk" "$INSTDIR\${APP_EXE}"
SectionEnd

; ------------------------------------------------------------- деинсталл ---

Section "Uninstall"
  ; закрываем приложение из нашего каталога
  MessageBox MB_OKCANCEL|MB_ICONINFORMATION \
    "Закройте ${APP_NAME} перед удалением.$\n$\nПродолжить удаление?" IDOK +2
  Abort

  Delete "$DESKTOP\${MOD_NAME} (${APP_NAME}).lnk"
  RMDir /r "$SMPROGRAMS\${MOD_NAME}"

  RMDir /r "$INSTDIR"
  IfFileExists "$INSTDIR" 0 +3
    MessageBox MB_OK|MB_ICONSTOP \
      "Не удалось удалить все файлы (приложение было запущено?). Удалите $INSTDIR вручную."
    Abort

  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\${MOD_NAME}"
  DeleteRegKey HKCU "Software\${MOD_NAME}"
SectionEnd
