; Hook NSIS Mandor (KTD-48 plan service OS).
;
; Sebelum berkas app dihapus, app sendiri dijalankan dengan --uninstall-cleanup:
; menghapus dua kunci keep-alive Mandor dari %UserProfile%\.wslconfig dan
; melepas daemon di WSL (mandord uninstall, tanpa --force). Mode update
; (/UPDATE) tidak membersihkan apa pun. Kegagalan tidak menggagalkan uninstall.

!macro NSIS_HOOK_PREUNINSTALL
  ${If} $UpdateMode <> 1
    nsExec::Exec '"$INSTDIR\${MAINBINARYNAME}.exe" --uninstall-cleanup'
    Pop $0
    ${IfNot} ${Silent}
      ${If} $0 == 2
        MessageBox MB_OK|MB_ICONINFORMATION "Mandor agents are still working in WSL, so the Mandor daemon was left installed. When they finish, run this in WSL:$\r$\n$\r$\n  ~/.local/bin/mandord uninstall"
      ${ElseIf} $0 != 0
        MessageBox MB_OK|MB_ICONEXCLAMATION "Mandor could not remove its WSL daemon automatically. Run this in WSL:$\r$\n$\r$\n  ~/.local/bin/mandord uninstall$\r$\n$\r$\nIf you enabled keep-alive, also remove instanceIdleTimeout and vmIdleTimeout from %UserProfile%\.wslconfig."
      ${EndIf}
    ${EndIf}
  ${EndIf}
!macroend
