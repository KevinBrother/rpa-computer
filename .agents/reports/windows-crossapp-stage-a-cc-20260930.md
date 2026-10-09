# Windows crossapp StageA — CC independent review + first RED — 2026-10-01

- 执行身份：CC（真实 Windows acer-win，Session 0 纯 console，仅经 approved bounded runner 执行作者精确 test 命令；无 GUI/浏览器/Notepad/Calculator/Host/网络/应用启动；无 Mac/Linux 执行；root Rust 未触碰；无 commit/push/worktree）。实际模型 glm-5.3-flash。
- 结论：**首次 RED 已按预期捕获，零源码修改。** 9/9 冻结源码远端前后哈希核对一致；`python -B tests\windows_crossapp_pack.py -v`（120s bounded runner）实际 **Ran 49 tests，FAILURES=65，ERROR=0，SKIP=0，native exit=1**；全部失败均为真实 `NotImplementedError` → `AssertionError: missing capability from <API>: StageA: …`（5 个能力专属消息全部出现），无 import/语法/路径/超时/下载致因。FAIL 条数 65>49 符合作者预告（subTest 展开）。未 rerun-for-green，测试后源码哈希不变。

## 1. 规格/源码独立语义审查（执行前，无阻断）

- 冻结清单 `frozen-source-sha256.txt`（清单自身 SHA256=`c8e73799c1d8f940d56fa123e1320ecaa17e7b2ba82f2381f0ac9ff06e552751`）共 9 文件：5 新增（API.md、crossapp_artifacts.py、crossapp_pack.py、crossapp_paths.py、tests/windows_crossapp_pack.py）+ 2 只读旧任务（drag.md、windows-focus.md）+ runner ps1/Runner.cs。本地 `shasum -a 256 -c` **9 OK**。
- **5 个 API 均为唯一函数体直接 `raise NotImplementedError("StageA: … capability missing")`**，无任何先实现/探测/创建/删除/启动逻辑（`crossapp_paths.py`/`crossapp_pack.py`/`crossapp_artifacts.py` 逐一亲读确认）。
- **测试真实针对准备/边界/检查**：49 个 test 方法（作者 inventory AST 计数与实际文件逐名一致）全部通过 `self.call(fn,…)` 调用上述 API 并将 NotImplementedError 转为带函数名的 `self.fail`（**非 skip、非 import 即过**）。覆盖 NEW root 校验、UNC/device/drive-relative/traversal/ADS/保留名/reparse（lstat 注入自有路径事实，无 mklink 权限依赖）、六例唯一 nonce/trial、supervisor/agent zone 隔离、UTF BOM 严格解码、CRLF/LF/混合/NFC-NFD/末尾空格、Explorer 64 文件移动后清单 hash、browser 导出上限与自报不可信、drag/focus 原任务内嵌+源 hash、checker 只读。import 仅 stdlib + 3 个 stub 模块，`sys.path` 按 `__file__` 定位。
- **唯一对测试源码的质疑点（非阻断）**：`test_reused_native_tasks_preserve_scope_and_source_hash` 用 `hashlib.sha256((REPO / path).read_bytes())` 对照 `reuse.source_sha256`，即准备器须读仓库内任务文件；测试自身直接读 REPO 源文件属只读自证数据源，可接受。其余无规格问题，未发现需停工的缺陷。

## 2. 冻结传输与 staging

- 9 文件按相对路径 tar → scp/base64 → 远端**新唯一 TEMP**（保留布局）：
  `C:\Users\Administrator\AppData\Local\Temp\computer-crossapp-red-cc-240d648b785d499bb49420980cecf890`
- 远端 `Get-FileHash` 对照 manifest：**9 OK / 0 MISMATCH**（`xa-red-hashverify.txt`），运行后复查再次 **9 OK / 0 MISMATCH**（`xa-red-hashrecheck.txt`，即测试过程未改任何源码）。
- Python：真实安装 `C:\Users\Administrator\AppData\Local\Programs\Python\Python312\python.exe`（前轮只读确认 3.12.10，非 Store 占位符）；runner summary `executable_sha256=4d6f5f81…`、`root_session_id=0`。

## 3. 首次 RED 执行（单步，approved runner，120s，`PYTHONDONTWRITEBYTECODE=1` 仅设于本次父进程环境）

- config（`evidence/crossapp-config.json`，5 属性）：`executablePath`=上述 python；`workingDirectory`=staged 根；`evidenceDirectory`=`…\xaout\crossapp-evidence\crossapp-stagea`（父目录预建，leaf 由 runner 原子 CreateNew）；`arguments=["-B","<根>\tests\windows_crossapp_pack.py","-v"]`（作者精确参数）；`timeoutSeconds=120`。
- **summary.json 原始关键值**：`native_exit_code=1`、`outcome="native_failure"`、`timed_out=false`、`cleanup_state="root_exited"`、`root_pid=3988`、`root_start_utc=2026-09-30T17:22:27.5553555Z`、stdout 0 B、**stderr 127,827 B**（unittest 主写 stderr，drain 完整未截断）、runner 两源 SHA=`ad2a607c…`/`aced18e6…` 与冻结一致。
- **harness exit ≠ native**：runner harness `harness_exit_code=1`；权威状态取 summary `native_exit_code=1`，未用 wrapper/CLI exit 冒充。

## 4. RED 实际构成（逐行来自原始 stderr，collect.txt 归档）

```
Ran 49 tests in 0.055s
FAILED (failures=65)
FAIL: 65  ERROR: 0  SKIP: 0
```

- **65>49**：subTest 展开（如 `test_resolve_traversal_both_separators`、`test_resolve_ads_reserved_trailing_alias_rejected`、`test_mixed_newlines_and_extra_cr_rejected` 等每个 subTest 单独计 FAIL），与作者预告一致，不要求恰好 49。
- 每条 FAIL 链均为：stub `raise NotImplementedError("StageA: …")` → 测试 `call()` 捕获 → `self.fail("missing capability from <fn>: <原因>")`。130 条 `missing capability from` 行 = 65×2（NotImplementedError + AssertionError 各一），与 FAIL 数精确吻合。
- **5 个能力专属消息全部出现**：
  - `StageA: validate_new_root safety capability missing`
  - `StageA: resolve_owned_path containment capability missing`
  - `StageA: prepare_owned_pack six-case capability missing`
  - `StageA: strict Notepad byte comparison missing`
  - `StageA: owned artifact checking capability missing`
- **无非预期致因**：无 ImportError/SyntaxError/PermissionError/FileNotFoundError/timeout/下载依赖；样例 traceback 见 collect.txt（`test_agent_supervisor_zones_do_not_leak` 等前三块已归档）。49/49 方法均触及真实未实现能力——按 inventory 49 方法名对照 65 条 FAIL 名单，无方法缺失、无方法因环境原因提前崩溃。

## 5. Wrapper 错误记录（原样保留，与被测物分开）

1. `stage-listing.txt` 之前两次 staging 尝试：首次 ssh 内联命令被引号层吞掉（弃用，改 on-disk 脚本）；二次 `tar` 打开 `…890+/frozen.tgz` 失败（`+` 进入路径）且 scp 实际落盘于 `Temp\xa-red-frozen.tgz` 而非脚本预期路径——第三次以字面源路径修正后成功（`stage-listing.txt` 为成功版清单）。
2. `master-attempt1-failed.ps1`：首次把 config `evidenceDirectory` 设为**已预建存在的目录**，runner 按 CreateNew 语义拒绝（`validation_failure`，harness 70，"Evidence directory already exists; no overwrite"），同脚本还有 `Join-Path -Exclude` 显示错误。此为 wrapper 配置错误，**未产生任何 native 测试结果**；修正为 leaf 不存在、父目录预建后一次成功。runner 的原子拒绝行为本身工作正常。
3. `collect-attempt1-broken.ps1`：结果行正则 `'^('` 语法错误致首次 collect 中止（结果行改 `-like 'FAILED*'` 后成功）。
4. 一次将两条 `powershell -File` 用 `;` 连进同一 ssh，被解析为单个 -File 参数失败——拆分两次执行成功。无 cmd `%ERRORLEVEL%` 作权威状态；无 rerun-for-green（native 步骤仅成功执行一次）。

## 6. 测试后完整性与残留

- 哈希复查 9 OK / 0 MISMATCH（§2）。
- 残留：read-only `Win32_Process` 按 **owned 根路径**过滤 CommandLine（不按名称/全局，无 kill）：**0 个进程**（`xa-red-residual.txt`）。runner summary `root_exited`。Notepad24332 未触碰、未查询窗口。

## 7. 归档

`.agents/runs/windows-crossapp-stage-a-cc-20260930/`：
- `xa-red-frozen.tgz`（传输包副本）、`stage-listing.txt`
- `evidence/`：`summary.json`（SHA256 `6ef34a7b…`）、`stderr.log`（127,827 B，SHA256 `2727a429…`）、`stdout.log`（0 B）、`config.json`、`identity.json`
- `collect.txt`（65 FAIL 构成/唯一消息/traceback 样例）、`xa-red-hashverify.txt`、`xa-red-hashrecheck.txt`、`xa-red-residual.txt`
- wrapper 全套脚本 + 3 份失败尝试原件（`master-attempt1-failed.ps1`、`collect-attempt1-broken.ps1`、stage 脚本修正前内容在 §5 描述）
- `xa-red-remote-final.tgz`（远端 staged 根终态，SHA256 `84ec19ae…`）。远端 TEMP 根保留未清理。

## 8. 边界（不隐瞒）

1. **StageA 只记录 RED**：未实现任何功能、未运行准备 CLI/导出/任何第二阶段、未启动任何应用或 GUI；StageB 由作者实现，CC 不写功能。
2. 65 FAIL 是"全部触及 NotImplementedError"的 RED 构成，**不是任何通过证明**；无 TDD 绿色声明。
3. 测试对 reparse/symlink 的注入基于测试自身路径的 lstat mock，不依赖也不验证真实 NTFS link 权限（作者设计内）。
4. 运行目录内另有并发工作区文件（task.md/transcript.jsonl/空 stderr.log，非本轮产生）未触碰。
5. root Rust、Mac/Linux、Host/renderer、防火墙/授权/全局配置未触碰；实际模型 glm-5.3-flash。

**STOP：StageA 首次 RED 已捕获并归档（49 tests / 65 failures / native exit 1，全为 NotImplementedError 链）。等待作者 StageB 实现授权；本轮零源码修改。**
