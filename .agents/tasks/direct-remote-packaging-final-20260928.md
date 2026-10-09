# Very narrow packaging final followup
Previouswriter58892terminal. Scope ONLYscripts/windows-remote-start.ps1, tests/remote_packaging.py, docs/remote-connection.md andownreport.agents/reports/direct-remote-packaging-final-20260928.md. Corewriter38759 disjointsrc+remote_transport. NoGUI/SSH/configchanges/commit/push; no newfeatures.
Independent29tests .agents/runs/direct-remote-packaging-independent2-20260928.log:1ERROR atline496 StopIteration. Test expectsmark_unhealthyincodebutonlycommentcontainsmarker; actualMark-Unhealthyfunction needs normalizeorsemanticassert. Fix THE TEST toassertactualfunctioncalls beforeexit ratherthancommentmarker. Do not weakenruntimefunctionsemantics. AddtestPSfiles containnoNUL/controlcharacters except newline/tab/CR: start.ps1 currentlyPythonwriteintroduced literalNUL-0x1f in regexrange causingbinaryfile; replacewithASCIIregexescape [\x00-\x1f"]. AllscriptsPSformat-fbindingalreadyfixed, preservethat.
A failedRED sourceoverlaptest left own evil-release/ atrepoROOT. It is knowncreatedbytests. Modifytest tocleanthat exactownedtest directory evenifassertfail, never arbitraryuserpaths. Youmayremove currentevil-release ONLY aftervalidateits RELEASE.json kindcomputer-remote-client andbincontentsfake testfixture. Report cleanup.
Docs add simplebuild commands(cargo build --release --bins). CrossWindowsringprecheck initialfailed missingllvm-lib, coordinatorprovedexistingrust-lld supports-flavorlink/lib; noinstallationneeded. Optional environmentworkarounddocument:
eval "$(cargo xwin env --target x86_64-pc-windows-msvc)"
export DYLD_LIBRARY_PATH="$(rustc --print sysroot)/lib"
export AR_x86_64_pc_windows_msvc="$(rustc --print sysroot)/lib/rustlib/aarch64-apple-darwin/bin/rust-lld"
export ARFLAGS_x86_64_pc_windows_msvc='-flavor link /lib'
cargo build --release --target x86_64-pc-windows-msvc --bins --offline
Mark workaroundthisMac-specificnotuniversal; Windowsnativecargo buildalsoworkswhenstandardtoolchaininstalled. Nohardcodeduserhome.
Run29tests trueexitcode(no pipehide), reportwithSKIPPowerShellparse(localpwshabsent)explicit. Coordinatorwillscp+parseontarget (EncodedCommandscriptcontentwas toolong, notscriptparseerror). Max10toolcalls/14turns. Begin.
