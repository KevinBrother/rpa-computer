# Windows focus StageB — SOURCE_FROZEN / 交回 CC

日期：2026-09-30（Asia/Shanghai）。只实现 Windows focus-01..10，不扩大 suite。
本轮实现者未运行任何测试/self-test/export/parity/程序集/GUI/截图/SSH/CC/其他 agent；
没有 commit/push/worktree，没有改权限或旧部署/快照。共享脏工作区其他 owner 的修改保留。

## 1. StageA 真 RED 与 StageB 状态

已只读核对 CC 原始证据目录：
`.agents/runs/windows-focus-fixture-red-cc-20260930/computer-focus-red-cc-69d85709_5316_4706_8747_2148ba53/`

- `build-exit.txt`：0。
- `selftest-evidence-9455456fa1be4e97a89e0722adbad9f6/summary.json`：native_exit_code=1、timed_out=false、stdout完整。
- 同一 `selftest-evidence-.../stdout.log`（不是父目录下的 stdout）：唯一失败
  `focus-args-accepts-focus-suite`，原 parser 报 `unknown suite: focus`，1/144失败，其余143 PASS。
- StageA `FocusSelfTest.cs` 原封保留，SHA256：
  `39ad3fc966a721b214ce3ed8d33268481bc41060f9f08519e4a23a94799de653`。
  StageB 只让严格 parser 自然接受 focus，不换断言/硬编码成功。

**StageB 源码已冻结，可交 CC Windows 验收；不声称测试 green 或 GUI 通过。**
没有已知的剩余源码实现阻断；Windows csc/原生消息/实际工具证据是否满足契约待 CC。
现有 GUI 授权弹窗阻断保持 pending，不改变系统权限。

## 2. 实现文件与范围

### 六个新原生模块

- `acceptance-fixture/windows/FocusCases.cs`：纯10例目录/独立manifest、原始事件模型、raw消息分类与校验。
- `acceptance-fixture/windows/FocusNative.cs`：已注册自有 HWND 的 WndProc/IMessageFilter；真实线程内
  GetFocus/GetActiveWindow/GetMessageTime；own release跟踪，不全局hook、不注入、不看其他应用。
- `acceptance-fixture/windows/FocusWindows.cs`：自有B、真正ShowDialog模态D；D内Check，专用Close/Minimize/Restore。
- `acceptance-fixture/windows/FocusForm.cs`：大字ID/nonce/instruction/首次结果、Check once/Next、append-only事件身份、
  HWND绑定、trial/check序号、after_check事件与case10覆盖心跳。初始化不冒充目标事件。
- `acceptance-fixture/windows/FocusJudge.cs`：纯own判定；原始字段/时序/index/focus/release与精确Text/Selection；
  不normalize，不以终态文本单独判成功，payload的实际WM_CHAR序列也须匹配。
- `acceptance-fixture/windows/FocusRegression.cs`：独立纯回归源码；严格参数、10例synthetic语义场景、
  缺activation/raw mismatch/wrong HWND与focus/未释放/modal/menu/restore/零事件假pass等负例。未执行。

### 必要入口与独立辅助文件

- `windows/BasicArguments.cs`：仅focus白名单、`FocusExportPath`、`--export-focus-cases`及console mode冲突规则。
- `windows/MainForm.cs`：仅focus测试注册、GUI前独立export分支、focus form路由和usage注释。
- `acceptance-fixture/build-windows.ps1`：仅新增Focus*.cs编译include，保留StageA模块存在性检查。
- `acceptance-fixture/start-windows.ps1`：仅suite白名单允许focus；未运行此启动脚本。
- `scripts/analyze-focus-gui.py`：独立focus诊断，不改旧analyzer。
- `tests/focus_gui_analyzer.py`：独立synthetic转录/oracle负例及受限正例源码，helper不再与kind关键字冲突。
- `acceptance-fixture/tasks/windows-focus.md`：新的GLM-only、8工具、source-free、自有窗视觉任务。
- `acceptance-fixture/README.md`：只追加Windows focus扩展说明。

StageB业务/文档写集共14文件（下节精确hash）；StageA测试模块列为不变编译依赖。
没有修改 BasicCases/Form/Judge/Native/SelfTest、旧Cases/Gesture源码、旧analyzers/tests、Mac/Linux、
Rust/root/renderer/设计CONTRACT。上述受保护C#文件与StageA清单逐文件指纹比较：12文件均相同，
见 `protected-source-fingerprints.json`。这只是文件指纹核对，不是测试验收。

## 3. 十例及诚实结论边界

| ID | 原生own场景 | 必须保留的门控 |
|---|---|---|
| 01 | A.edit → B.edit | 真实down/up、up时目标GetFocus、B WM_ACTIVATE与工具关联 |
| 02 | B.edit → A.edit | 真实往返focus/activation及按键无输入 |
| 03 | B激活后FOCUS | 正确owned EDIT、实际WM_CHAR、精确读回与tool text参数 |
| 04 | A EDIT Ctrl+A | 原始Ctrl/A keydown/up、11字符全选、文本不变 |
| 05 | 自有modal D输入MODAL | A自然disabled与真实WM_ENABLE、D激活；Check在D内，不补焦点 |
| 06 | 专用Close D后A输入BACK | 真modal打开、WM_CLOSE、A重新enabled、目标读回 |
| 07 | 原生own菜单退出后MENU | WM_ENTERMENULOOP/EXITMENULOOP与Escape80ms工具包络 |
| 08 | B最小化→A按钮恢复→RESTORED | WM_SIZE最小化/恢复、activation、真实按钮click及EDIT输入 |
| 09 | 只向A输入OWNED | 最高needs_supervisor；own输入证据不能证明全部用户窗未输入 |
| 10 | 明确FOCUS UNCONFIRMED下不盲输 | app只needs_tool_evidence；完整真实tool trace+clean Close+终态+Close后心跳 |

- 原始字段 `native_message/raw_wparam/raw_lparam/native_timestamp_ms/own_handle/focus_handle/active_handle`
  与 `kind/role` 分离；绑定只登记本进程自有窗口，不能反查用户应用。
- Check只读取Text/Selection/Enabled/WindowState等实际状态，不发目标输入、不修复焦点。
  输入动作仅由GLM点击真实own业务按钮/目标产生；所有全局键盘hook/SendInput/SendKeys均未新增。
- 已有焦点的EDIT再次点击不一定产生WM_SETFOCUS，因此以实际mouseup时GetFocus作焦点证据，
  而跨窗和激活例仍需WM_ACTIVATE，不能仅凭最终文本。
- case9固定 `global_protected_apps=unknown`，不得访问Notepad等用户窗口补证明。
- case10要求trial之后真实observation图像块与尺寸/geometry/based_on，且唯一管理Check事件能关联唯一
  call/result；策略段不可夹text/key/其他目标动作，Close干净且同session、完整CLI终态唯一，
  Close开始后不能再有工具调用，oracle心跳覆盖至Close结果后。心跳前若新增目标输入也不通过。
  此结论仍只限这份agent工具轨迹与own oracle，不是全桌面无输入。
- `safe_stop_confirmed` / `needs_supervisor` 均不计matched或valid_input；正常最高8个matched、80%，
  不是“10/10 GUI通过”。suite_complete只说明访问/记录结构完整，不是安全或GUI验收完成。
- 缺actual model、图像metadata、原始消息、先于事件的HWND绑定、唯一request/call/result或可信时间均unknown。
  不按Nth输入配对；first/final分开、retry单列、partial以全部10例为分母。
- `gui_verified=false`、`ten_valid_trials_per_action_gate=false`固定不自行升级。

## 4. 只编译证据（不是 CC 测试）

初次compile-only目录 `.agents/reports/windows-focus-fixture-sol-20260930-compile-213234/`：C#0、Python0。
此后静态修改完成，最终重新compile-only目录：
`.agents/reports/windows-focus-fixture-sol-20260930-final-compile-214952/`

- C#：`mcs -langversion:5 -target:winexe`，MainForm.cs/Cases.cs/Gesture*.cs/Basic*.cs/Focus*.cs，
  System/System.Drawing/System.Windows.Forms引用，exit **0**，日志无输出。
- Python：仅 `python3 -m py_compile scripts/analyze-focus-gui.py tests/focus_gui_analyzer.py`，exit **0**，日志无输出。
- 编译exe/cache全在新独占报告附件目录；没有运行exe、导入测试模块或分析器进行自检。
  `py_compile`只编译，不执行模块顶层。不能拿本机Mono产物代替Windows Framework csc产物。
- 精确命令在 `commands.txt`，数值退出码在 `csharp-exit.txt` / `python-exit.txt`。

## 5. 源码冻结指纹

最终目录内：
- `stage-b-sha256.txt`：本轮14文件。
- `source-sha256.txt`：全部C#编译输入、本轮文件、只读B2 analyzer/tests及旧parity所需工具/两份Swift目录源。
- `protected-source-fingerprints.json`：12份受保护C#与StageA相同。
- `frozen-at.txt`：精确冻结时间；报告自身不纳入源码manifest，避免自引用。

下列指纹为本轮14文件，CC复制后须逐一核对：

```text
7b0e18d6c7c4854f6b2a7407780c8c0f15ee23a596e7d7584b964b83457f05f4  acceptance-fixture/windows/FocusCases.cs
d18d887cc2e5389037f8b16985866f13d313c7c3992679f4734629478efa8447  acceptance-fixture/windows/FocusForm.cs
6696b7e1a9907cd577035ff1f6b77c7e29d48a8972b52728a34c74f5ebcfa9dc  acceptance-fixture/windows/FocusJudge.cs
498730fc32e7ff4f693a59d3ad0106ddf84da1a8ca2fe1c5542184fcfa8d98ff  acceptance-fixture/windows/FocusNative.cs
b48b2aaf061ab8e0bfd45feecd0a65ebf49145464a50f0821c40425570dd6068  acceptance-fixture/windows/FocusRegression.cs
a98aceb2a606ec765932e4fb3194cbcc20f4317304b708a91595f05590ebae17  acceptance-fixture/windows/FocusWindows.cs
cb943b417553f15f3a933df48f2d198a279c303c9ab8b8c1f5569a8b900ba7d9  acceptance-fixture/windows/MainForm.cs
31dbdd36ed71caa50d79a85e1acc7d0b2c6e33f2da177a1116566d341b3fdf59  acceptance-fixture/windows/BasicArguments.cs
65cd49e0dfc78a53115e77c57234a28ae848098ae6fb72dfb3aad49c42059e11  acceptance-fixture/build-windows.ps1
08e7bafd887227a503b87856f16f122b830c447aa0d9986da04bc9ff4cd64c64  acceptance-fixture/start-windows.ps1
15c06ae298631f6611d486a1189b1cbd32d09892105f7666d5e1cdadacd9b399  acceptance-fixture/README.md
128b9e6bfa1aa696b33c5de0cc600d80fd7cfc9d5ad07331a7856a12fc3ba98f  acceptance-fixture/tasks/windows-focus.md
5cab0102f0b9c5841a42f0b20fb8603f74bc01eacc8d5b0cb40d94422f342474  scripts/analyze-focus-gui.py
275a0867b38c3939512f40105e9e45d246c057b954c9c30a9cbd9ddc19096132  tests/focus_gui_analyzer.py
```

## 6. 仅供 CC Windows 执行的验收指令（实现者未执行）

先用上述manifest建立**新的冻结副本**，不要覆盖B2/StageA/GUI旧snapshot或bin。
带齐全部C#、两个focus Python文件以及只读依赖 `scripts/analyze-basic-input-gui.py`。
旧63 parity在Windows运行时还需 `acceptance-fixture/tools/check-cases-parity.py`、
`macos/Cases.swift` 和 `macos/GestureCases.swift`（只读目录数据，不运行Mac程序）。
下面runner为CC已使用的只读基础设施，复制其 `.ps1` 与 `windows-test-runner/Runner.cs` 并独立记录hash；
该runner不属本任务写集，本轮未修改。first RED即停、保留stdout/stderr/native exit，不制造green。

```powershell
# 在CC新的Windows源码snapshot根目录；建议存为短.ps1串行执行。
$ErrorActionPreference = 'Stop'
$root = (Get-Location).Path
$out = Join-Path $env:TEMP ('computer-focus-green-cc-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $out | Out-Null
powershell -NoProfile -ExecutionPolicy Bypass -File .\acceptance-fixture\build-windows.ps1 -OutDir (Join-Path $out 'buildout') 1> (Join-Path $out 'build.log') 2> (Join-Path $out 'build.err')
$code = $LASTEXITCODE
$code | Set-Content (Join-Path $out 'build-exit.txt')
if ($null -eq $code -or $code -ne 0) { throw "csc exit=$code" }
$exe = Join-Path $out 'buildout\ComputerUseAcceptance.exe'

function Invoke-FocusNative([string]$Label, [string]$Program, [string[]]$Argv) {
    $evidence = Join-Path $out ($Label + '-evidence') # must not already exist
    $config = Join-Path $out ($Label + '-config.json')
    $cfg = @{ executablePath=$Program; arguments=@($Argv); workingDirectory=$root;
              evidenceDirectory=$evidence; timeoutSeconds=120 }
    [IO.File]::WriteAllText($config, ($cfg | ConvertTo-Json -Depth 8), (New-Object Text.UTF8Encoding($false)))
    powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\windows-test-runner.ps1 -ConfigPath $config
    $exit = $LASTEXITCODE
    $exit | Set-Content (Join-Path $out ($Label + '-harness-exit.txt'))
    if ($null -eq $exit -or $exit -ne 0) { throw "$Label runner exit=$exit; preserve first RED and STOP" }
    $s = Get-Content -Raw (Join-Path $evidence 'summary.json') | ConvertFrom-Json
    if ($null -eq $s.native_exit_code -or $s.native_exit_code -ne 0 -or $s.timed_out) {
        throw "$Label native failure/missing/timeout; STOP"
    }
}

Invoke-FocusNative 'selftest' $exe @('--self-test')
# CC从真实stdout报告新总数；需逐条保留原143与StageA断言，不能只取末行。
Invoke-FocusNative 'export63' $exe @('--export-cases', (Join-Path $out 'legacy63.json'))
Invoke-FocusNative 'export20' $exe @('--export-platform-cases', (Join-Path $out 'windows-basic20.json'))
Invoke-FocusNative 'export-focus10' $exe @('--export-focus-cases', (Join-Path $out 'windows-focus10.json'))
# $python必须是CC已确认的实际python.exe，不用Windows Store启动占位符。
$python = (Get-Command python -CommandType Application -ErrorAction Stop).Source
Invoke-FocusNative 'focus-analyzer' $python @((Join-Path $root 'tests\focus_gui_analyzer.py'))
Invoke-FocusNative 'basic-analyzer-regression' $python @((Join-Path $root 'tests\basic_input_gui_analyzer.py'))
Invoke-FocusNative 'legacy63-parity' $python @((Join-Path $root 'acceptance-fixture\tools\check-cases-parity.py'), (Join-Path $out 'legacy63.json'))
```

CC独立核对：旧63目录不变、B2 Windows pointer10+keyboard10、focus Windows独立10例唯一有序ID
`focus-01`…`focus-10`，GUI与十次有效trial门禁false。任何字段缺失/数量不符按真实RED记录。
这些console操作不应创建窗口；验证native exit，不把harness成功或日志存在当验收成功。
不要猜新self-test/analyzer数量；由CC实际结果报告。

### GUI 后续单独授权时（当前仍暂停）

由协调者在真实interactive console session安排新exe和唯一evidence文件，启动helper须传新exe真实SHA256；
不给GLM源码/oracle。按 `tasks/windows-focus.md` 执行；首帧授权弹窗或缺靶场立即STOP Close，不绕路。
本轮不给或执行远端启动/GUI指令，避免误解除当前GUI暂停。

待真实GUI获批完成后，CC对原始oracle/完整转录使用：

```powershell
# 变量指向协调者保存的真实原始证据；output必须为不存在的新文件。
python .\scripts\analyze-focus-gui.py --suite focus --oracle $OraclePath --transcript $CompleteTranscriptPath --output $NewAnalysisPath
$analyzerExit = $LASTEXITCODE
# exit0仅表示诊断执行完成；还须审核JSON中的逐例first/final/unknown/blocked和安全门控。
```

不手填时间/actual model/终态/image metadata使之通过，不裁掉失败、授权弹窗或后续输入。
缺必要CLI终态timestamp、图像块或Close后心跳时case10保持unknown；截图内容/全局政策仍由监督者审核。

## 7. 未验证与非本任务

1. Windows Framework csc、PowerShell构建/启动脚本、全部self-test/analyzer/parity **StageB pending CC**。
   已有143/30/旧63通过是前轮CC证据，不替代这次回归；本机compile0不能称为CC green。
2. 真实WM焦点/activation、modal disable/reenable、菜单native loop、最小化恢复、释放/选择读回、
   首帧/高DPI/窗口重叠/可读性都未GUI验证。当前GUI0输入授权暂停不变。
3. oracle UTC、GetMessageTime与工具包络若偏差/缺失，诊断会unknown；不自动校正系统时间或伪造时间。
4. case9全用户窗未输入没有实现“自动证明”；明确保留supervisor未知门控。case10的safe stop非全局安全证明。
5. cancel/geometry/multi/crossapp不在本轮；没有完成所有动作十次首次有效trial。Mac/Linux保持原状且不安排测试。

**SOURCE_FROZEN：本轮源码工作结束，不为等待CC自行扩展或测试。CC可按上述精确指纹接管独立验收。**
