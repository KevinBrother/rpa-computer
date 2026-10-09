# Windows crossapp StageB — SOURCE_FROZEN

- 已停止源码修改。5 API、2薄CLI、6例生成任务、独立离线页及只读文件诊断已实现，运行正确性待CC。
- 原49方法断言未改；新增13方法位于 `tests/windows_crossapp_pack_stage_b.py`。StageA历史manifest/tar/RED未覆盖；6个历史只读输入（原测试/API/任务/runner）SHA相同，见 `preserved-stage-a-inputs.json`。
- 改动：`acceptance-fixture/crossapp/`内8个Python模块、3个页面资产、README；两CLI、新增测试模块、`acceptance-fixture/tasks/windows-crossapp.md`。API.md保留历史合同。完整24文件清单含原drag/focus任务、runner及规格只读依赖。
- compile-only：本机Python3.14.0 `py_compile` **exit0**；`node --check course.js` **exit0**。精确命令见同目录 `compile-only-r3.log`。缓存仅 `/tmp`。未运行测试、API、prepare/check、SSH、GUI或其他agent；未commit/worktree。

## 最后授权修订（r3-final）

- native05/06明确共用一次Describe/Open，原内嵌任务控制收尾；最终Close后仅文字报告，零额外工具。
- crossapp06在原通用第1步后、任何target输入前，同一session执行pause→核验真实paused回复→resume→核验回复→重新Observe→原第2步。仅未来GUI获授权且supervisor确认A/B所有权后执行；paused期间不输入/不Observe；error/unknown立即Close/STOP。不做held cancel，不在focus-10 Close后调用工具。
- 新增一条静态任务契约测试，原内嵌任务及49条RED断言不改。原初版冻结manifest/archive/receipt保留，使用下列r3-final交CC。

## 冻结

- Manifest：`.agents/runs/windows-crossapp-stage-b-sol-20260930/frozen-source-r3-sha256.txt`
- SHA256：`200a9217c66344ffd35ff3529edd626124a399e264c55ce516d3d2e83b16f93f`
- Archive：`.agents/runs/windows-crossapp-stage-b-sol-20260930/windows-crossapp-stage-b-source-r3.tar.gz`
- SHA256：`b0daca75456fd884a84f7fef19bef3bb9910613e74d68e61b697f9ee5ad20db8`
- 冻结时间：`2026-10-01T01:54:35.868123+08:00`

## CC Windows待执行（先纯验证，GUI仍未授权）

在独立解包源码目录，使用已调查Python3.12；每条通过既有bounded runner独立保留native exit/stdout/stderr/timeout/root_exited。以下是准确子进程命令，作者未执行：

```powershell
$py = 'C:\Users\Administrator\AppData\Local\Programs\Python\Python312\python.exe'
& $py -B tests/windows_crossapp_pack.py -v
& $py -B tests/windows_crossapp_pack_stage_b.py -v
# C:\Temp须已存在；该root必须全新、私有、本地且无并发修改。
$owned = 'C:\Temp\crossapp-stage-b-' + [guid]::NewGuid().ToString('N')
& $py -B scripts/prepare-windows-crossapp.py --root $owned
& $py -B scripts/check-windows-crossapp-artifacts.py --root $owned
```

两测试分别保留原49/新增13方法的真实结果，失败即停并保留原始证据。prepare native0仅表示文件准备完成；check native0仅表示输出诊断（可含mismatch/unknown），不可当验收通过。新包6例应needs_evidence，所有GUI/ownership验证为false。runner入口：`powershell.exe -NoProfile -File scripts/windows-test-runner.ps1 -ConfigPath <NEW_CONFIG.json>`；配置恰含 `executablePath`、`arguments`数组、`workingDirectory`、全新`evidenceDirectory`、`timeoutSeconds`（建议180），子进程参数即上述命令去除`& $py`部分。

## 限制 / 仍待证据

- 未执行Windows回归或页面运行；无GUI成功结论，授权弹窗/权限现状不变，不启动/配置/杀应用。安装存在不能证明独立激活，PID24332禁止操作。
- Notepad空nonce资产≠结果文件；字节严格解码，不normalize换行/Unicode/空格。Explorer文件终态仅artifact_match，不证明drag；Calculator需三个独立真实截图。Browser记录/checkbox/isTrusted均不可信GUI证明，profile/download须未来supervisor明确确认。
- 05/06保留原任务边界与其实际可见内层nonce；外层pack身份不等于fixture身份，实际PID/创建时间/HWND及每个inner case/trial/nonce未显式绑定仍needs_preflight/needs_evidence。不是新增外部应用数量。
- 文件操作前后lstat/身份复核、独占创建、拒绝UNC/ADS/reparse/硬链接及Windows远程映射盘；不删除覆盖。**不承诺抵抗并发祖先替换的原子TOCTOU安全**，root和parent必须私有且无并发修改。部分准备失败保留，不自动清理或复用。

SOURCE_FROZEN：源码写入停止，交CC独立review/执行。
