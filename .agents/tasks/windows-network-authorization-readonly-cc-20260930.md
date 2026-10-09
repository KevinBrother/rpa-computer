# Windows existing network authorization read-only diagnostic

Only read state; DO NOT start Host/fixture/renderer/GUI/scheduledtask, bindports, capturedesktop, sendinput, click Allow/Cancel, add/modify/deletefirewallrules, credentials, permissions/globalsettings, restartservices, unlock, copybinariesorreplaceinstalledfiles. No code/testchanges, noAgent/Task. ActualGLM andSSHacer-win. Goal is distinguish knownoldpathauthorization vs newpreflightpath; not bypass permission by choosinganotherpath. ProtectNotepad24332. User has NOT confirmed currentdialog handled; even anallowrule isnot proofdesktopunobstructed.

Read previous windows-gesture-preflight2 report. Through read-only PowerShell query only application firewall filters/rules whose Program exactly matches observed project paths (case insensitive):
C:\computer-cc-preflight-20260930-191500\bin\computer-host.exe
C:\computer-native-20260929\bin\computer-host.exe
If APIrequiresadmin andfails, recordfailureSTOP noescalation. Get associatedrules Enabled/Direction/Action/Profile/PolicyStoreSourceType/PrimaryStatus (no otherapps dump). ReadactiveNetworkCategory/InterfaceIndex/IPv4Connectivity only (noSSID/user/accountnames/secret). Read8399 LISTEN presence only and exactNotepadPID24332 existence only, nointeraction. Do notinterpretmatchingruleasalways-effective policy or networksuccess; precedence/profileanddialogstateunknown. NoFileHash/contents ofca/token/privatekey.

Prefer UTF16 EncodedCommand plus UTF8output JSON toavoid repeatedSSHcmdquoting; disposablelocalqueryscript/ownremoteTEMPfileallowed onlyifneeded, no reusableproductimplementation. Save rawstdout/stderr/exits exactquerywith timestamp in `.agents/runs/windows-network-authorization-readonly-cc-20260930/`; short report `.agents/reports/windows-network-authorization-readonly-cc-20260930.md`. Clearly distinguish observedrules vs effectiveaccess vs UIpopup; don't ask/assumepermissiontochange. STOPafterread/report. This isn'tthirdGUIpreflight.
