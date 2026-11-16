; app.nsi - universal per-user installer template for kind=cargo projects.
; The toolkit runs: makensis /DPRODUCT_NAME=... /DVERSION=... ... app.nsi
; Required defines: PRODUCT_NAME, VERSION, SOURCE_DIR, EXE_NAME, OUT_FILE,
;                   INSTALL_SUBDIR, UNINST_KEY.
; Optional defines: PUBLISHER (default = PRODUCT_NAME).
; SOURCE_DIR is a staging folder with ONLY the app exe + vc redist DLLs
; (shared target-dir means the real exe folder holds foreign artifacts).
; ASCII-only on purpose: makensis + Windows quirks around non-ASCII scripts.

Unicode true
RequestExecutionLevel user

!ifndef PRODUCT_NAME
  !error "PRODUCT_NAME define is required"
!endif
!ifndef VERSION
  !error "VERSION define is required"
!endif
!ifndef SOURCE_DIR
  !error "SOURCE_DIR define is required"
!endif
!ifndef EXE_NAME
  !error "EXE_NAME define is required"
!endif
!ifndef OUT_FILE
  !error "OUT_FILE define is required"
!endif
!ifndef INSTALL_SUBDIR
  !error "INSTALL_SUBDIR define is required"
!endif
!ifndef UNINST_KEY
  !error "UNINST_KEY define is required"
!endif
!ifndef PUBLISHER
  !define PUBLISHER "${PRODUCT_NAME}"
!endif

!define UNINST_ROOT HKCU
!define UNINST_PATH "Software\Microsoft\Windows\CurrentVersion\Uninstall\${UNINST_KEY}"

OutFile "${OUT_FILE}"
SetCompressor /SOLID lzma

Name "${PRODUCT_NAME} ${VERSION}"
InstallDir "$LOCALAPPDATA\Programs\${INSTALL_SUBDIR}"

Page directory
Page instfiles
UninstPage uninstConfirm
UninstPage instfiles

Section "Install"
  SetOutPath "$INSTDIR"
  File "${SOURCE_DIR}\${EXE_NAME}"
  File /nonfatal "${SOURCE_DIR}\*.dll"
  WriteUninstaller "$INSTDIR\uninstall.exe"

  CreateDirectory "$SMPROGRAMS\${PRODUCT_NAME}"
  CreateShortcut "$SMPROGRAMS\${PRODUCT_NAME}\${PRODUCT_NAME}.lnk" "$INSTDIR\${EXE_NAME}"
  CreateShortcut "$SMPROGRAMS\${PRODUCT_NAME}\Uninstall ${PRODUCT_NAME}.lnk" "$INSTDIR\uninstall.exe"

  ; Add/Remove Programs entry (per-user, HKCU).
  WriteRegStr ${UNINST_ROOT} "${UNINST_PATH}" "DisplayName"     "${PRODUCT_NAME}"
  WriteRegStr ${UNINST_ROOT} "${UNINST_PATH}" "DisplayVersion"  "${VERSION}"
  WriteRegStr ${UNINST_ROOT} "${UNINST_PATH}" "Publisher"       "${PUBLISHER}"
  WriteRegStr ${UNINST_ROOT} "${UNINST_PATH}" "InstallLocation" "$INSTDIR"
  WriteRegStr ${UNINST_ROOT} "${UNINST_PATH}" "DisplayIcon"     "$INSTDIR\${EXE_NAME}"
  WriteRegStr ${UNINST_ROOT} "${UNINST_PATH}" "UninstallString" "$INSTDIR\uninstall.exe"
  WriteRegDWORD ${UNINST_ROOT} "${UNINST_PATH}" "NoModify" 1
  WriteRegDWORD ${UNINST_ROOT} "${UNINST_PATH}" "NoRepair" 1
SectionEnd

Section "Uninstall"
  Delete "$INSTDIR\${EXE_NAME}"
  Delete "$INSTDIR\uninstall.exe"
  Delete "$INSTDIR\*.dll"
  RMDir "$INSTDIR"

  Delete "$SMPROGRAMS\${PRODUCT_NAME}\${PRODUCT_NAME}.lnk"
  Delete "$SMPROGRAMS\${PRODUCT_NAME}\Uninstall ${PRODUCT_NAME}.lnk"
  RMDir "$SMPROGRAMS\${PRODUCT_NAME}"

  DeleteRegKey ${UNINST_ROOT} "${UNINST_PATH}"
SectionEnd
