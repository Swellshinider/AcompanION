; NSIS hooks for the AcompanION installer (tauri.conf.json > bundle.windows.nsis.installerHooks).
;
; After the files are copied, check for the Npcap runtime that capture needs. If it is missing, ask
; whether to download and run the official Npcap installer or just open its download page.
; Npcap's license forbids redistributing it and its silent mode is OEM-only, so we never bundle it
; or run it silently: the user goes through Npcap's own wizard (and its UAC prompt).
;
; ponytail: Npcap version and SHA-256 are pinned. Bump both (and docs/decisions.md) when a new
; Npcap is wanted; if the pinned file ever disappears, the download fails and the page is opened.
!define NPCAP_URL "https://npcap.com/dist/npcap-1.90.exe"
!define NPCAP_SHA256 "682b57e1c05ef375931975c9d6ded40888f4bfdf9f4b11571e654f962f1199b6"
!define NPCAP_PAGE "https://npcap.com/#download"

!macro NSIS_HOOK_POSTINSTALL
  ; Never ask during a silent install or a self-update (updates run passive).
  ${IfNot} ${Silent}
  ${AndIf} $PassiveMode <> 1
  ${AndIf} $UpdateMode <> 1
    Push $0
    Push $1

    ; Same file npcap.rs loads: the real System32, not the SysWOW64 view of this 32-bit installer.
    ${If} ${RunningX64}
      ${DisableX64FSRedirection}
    ${EndIf}
    StrCpy $0 0
    ${If} ${FileExists} "$WINDIR\System32\Npcap\wpcap.dll"
      StrCpy $0 1
    ${EndIf}
    ${If} ${RunningX64}
      ${EnableX64FSRedirection}
    ${EndIf}

    ${If} $0 = 0
      MessageBox MB_YESNOCANCEL|MB_ICONQUESTION \
        "AcompanION needs Npcap to read game traffic, and it is not installed.$\r$\n$\r$\n\
Yes: download the official installer from npcap.com and run it now.$\r$\n\
No: open ${NPCAP_PAGE} so you can install it yourself.$\r$\n\
Cancel: skip it (AcompanION will remind you)." \
        IDYES npcap_install IDNO npcap_open_page
      Goto npcap_done

      npcap_install:
      ; Download, check the hash, then run Npcap's own installer (ShellExecute asks for admin).
      Delete "$TEMP\npcap-setup.exe"
      DetailPrint "Downloading Npcap..."
      ; curl.exe and certutil ship with Windows 10+ (NSISdl timed out on TLS here; PowerShell
      ; modules can be shadowed by the user's PSModulePath). findstr exits 0 only on a hash match.
      nsExec::ExecToStack 'curl.exe -fsSL --max-time 120 -o "$TEMP\npcap-setup.exe" ${NPCAP_URL}'
      Pop $0
      StrCpy $1 1
      ${If} $0 = 0
        nsExec::ExecToStack 'cmd /c certutil -hashfile "$TEMP\npcap-setup.exe" SHA256 | findstr /i /x ${NPCAP_SHA256}'
        Pop $1
      ${EndIf}
      ${If} $1 = 0
        DetailPrint "Running the Npcap installer..."
        ExecShellWait "open" "$TEMP\npcap-setup.exe"
        Delete "$TEMP\npcap-setup.exe"
        Goto npcap_done
      ${EndIf}
      Delete "$TEMP\npcap-setup.exe"
      MessageBox MB_OK|MB_ICONEXCLAMATION "Could not download Npcap. The download page will open instead."

      npcap_open_page:
      ExecShell "open" "${NPCAP_PAGE}"
    ${EndIf}

    npcap_done:
    Pop $1
    Pop $0
  ${EndIf}
!macroend
