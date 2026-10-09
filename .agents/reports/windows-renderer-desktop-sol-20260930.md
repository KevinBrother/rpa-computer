# Windows renderer Desktop — StageB frozen implementation (2026-09-30)

状态：**SOURCE_FROZEN / COMPILE_ONLY_OK / CC_WINDOWS_GREEN_PENDING**。

StageB 已获用户 GO；本轮只代码实现、源码静态核对和 C#5 compile-only。**未执行任何测试/程序集/GUI/SSH/CC**，不声称 native GREEN、实际可见性/焦点行为或 capture exclusion 已通过。冻结后停止写入，交协调者安排真实 Claude CLI + GLM Windows 验证。

## 1. 已核验的前置 RED（历史 CC 证据，不是本轮执行）

- `.agents/reports/windows-renderer-desktop-red-cc-20260930.md`
- `.agents/runs/windows-renderer-desktop-red-cc-20260930/self-test-windows.output`
- 原文：`build_exit=0`、`test_exitcode=1`、stderr `self_test_renderer_desktop_negative_origin_dual_screen_bbox`、stdout 空。
- 原147项 + StageA前12项已经越过；第13项真实失败，末3项未执行，**没有把末3项计为通过**。

已按 CONTRACT 末尾和多屏计划 locked choices/Task5 实现。feedback v1字段不变，Surface.id仅精确 `desktop` 特判。

## 2. 本轮改动路径

1. `desktop-feedback/windows/Model.cs`（186行）：纯 ScreenFacts 校验、严格 Desktop resolver、PointerScreen/RingCenter、独立纯 OverlayLayout。
2. `desktop-feedback/windows/Windows.cs`（193行）：事实适配、真实屏 anchor/layout 接线、实际窗口/按钮边界与 DPI 回读、无效拓扑/空间不足的显式失败、ring 仅在合法真实屏点显示。
3. `desktop-feedback/windows/SelfTests.cs`（365行）：保留原163项的期望；增加 RendererBoundarySelfTests，新增72项纯分支边界检查。
4. `desktop-feedback/windows/Program.cs`（89行）：仅 Stop callback 增加异常收尾；先 queue Stop，再渲染；若渲染失败，经 Fail/OutputPump.Finish 排队 Error 并终止，不让异常直接逃出点击事件。
5. `.agents/reports/windows-renderer-desktop-sol-20260930.md`：本报告。

未改 Rust/root/feedback wire/default surface serialization/macOS/fixture/现有部署。未改 Native.cs、IPC.cs、Protocol.cs、JsonValue.cs、manifest/config 或两个现有构建脚本。无新增权限、全局光标、worktree/subagent/commit/push/reset。保留脏树和旧证据。

现有构建脚本固定枚举8个 cs，独立小类保留在现有编译单元，避免超出写集修改构建脚本；没有1000行大文件。

## 3. Resolver 行为

- `surface.Id == "desktop"` 为唯一 Desktop 分支（区分大小写）；其他ID只允许单屏几何匹配，不能因bbox大小而推断 Desktop。
- Desktop 使用全部传入 active Screen facts；GUI adapter 从 Screen.AllScreens 构造，不读取鼠标或根据 pointer 筛选屏。
- 所有 bounds/workarea 必须正宽高、整数边界计算不溢出、workarea包含于真实 bounds，facts/设备名/列表不可未知或空。Desktop 再拒绝重复identity（OrdinalIgnoreCase）、任意正面积 overlap/mirror。
- 用 long 计算全部屏幕 bbox；union跨度超int可表示范围拒绝；Surface四分量必须精确等于bbox（Desktop不借用single的0.5容差）。Surface非finite/非正尺寸/边界和溢出拒绝。
- single 保留 StageA首个匹配及 `<0.5` 容差、允许mirror/first-match；Desktop绝不走该分支。pointer额外要求落在实际选中屏 Bounds 内，不能落在single外另一屏。
- Desktop anchor固定为已验证列表首屏；不把bbox origin当工作区，也不跟随pointer改变。支持负原点、纵向、对角相接、不同workarea。若该屏空间不足明确报错，不偷偷换到未知/未受控屏。
- 合法Desktop的gap/null/nonfinite/outside pointer只令CanShowPointer=false，保留resolution/anchor，不清空surface、不发geometry错误。
- 显示ring时使用实际PointerScreen；浮点坐标四舍五入后夹在该屏有效整数像素中，避免屏边缘的合法浮点point被舍入到gap。已有phase/session/StopRequested/TTL门控保留。

## 4. Layout 与故障处理

`OverlayLayout.Calculate(ScreenFacts,double dpi)` 只处理值数据，无 Screen.AllScreens/Win32/WinForms初始化：

- DPI必须finite且 >=1（真实GetDpiForWindow是非零uint）。使用对应DPI的16px margin、48px height、88px Stop、8px gap、80px最小状态宽度；按比例向上取整，目标总宽448px。
- 总宽按workarea限制；若最小状态区+gap+完整Stop+两侧margin或高度放不下，抛 `surface_workarea_too_small`，**不缩成不可操作按钮、不让Stop越界、不声称已显示**。
- 验证空间够用后才将double尺寸转int，避免极大DPI的整数溢出；最终Bar/Stop都须包含于workarea。
- Place跨屏探测DPI时先隐藏旧Bar/Stop，定位到实际受控屏工作区，再调用现有GetDpiForWindow。设置计算后的Bounds后，检查实际Bar/Stop矩形、Stop HWND DPI、显示状态、Button正尺寸及其client containment；OS改变尺寸/位置等不满足要求时抛 `surface_layout_unavailable`。
- 选择/拓扑错误抛 `surface_geometry_unavailable`，隐藏反馈，并由现有Tick catch → Fail → Error/Dispose/OutputPump.Finish(65)收尾；不再在已声明但不可解析的surface上显示主屏fallback并继续声称成功。无surface的启动/等待态仍可用实际fallback屏布局。
- Stop callback在渲染之前queue真实Stop事件；本轮添加catch确保渲染失败也走原有队列收尾。未改FeedbackState取消去重、事件内容或IPC。OutputPump原有有界队列/300ms watchdog仍在；**不保证管道阻塞下事件已被Host接收或cleanup已完成**，也未新增虚假released/Stop成功回报。
- 无激活风格、MA_NOACTIVATE、装饰click-through与Stop可交互区分、PMv2和affinity调用保持。Native.RequestExclusion仍仅表示requested/readback，不以透明或纯自测冒充capture exclusion实测。

**需要后续原生/GUI审查的边界**：实际DPI迁移、ShowWithoutActivation、WinForms最小尺寸/按钮Dock、真实点击Stop取消、热插拔与屏幕枚举、Host收到renderer故障后的终止/cleanup以及capture exclusion。纯self-test不能证明这些GUI/Host行为。

## 5. 原163项完整保留、新增72项

将SelfTests.cs中的新增类与一行注册剔除，并还原一行StageA历史注释，重建SHA-256为：

`0a9dae5e76bc45c3b3ea868e07851d178d60c491fb4f30009f8b820be15fd391`

与StageA冻结文件（CC实际RED运行版本）逐字节一致。旧断言未改expected、未删除或跳过。新增类注册在原163项之后，Program --self-test仍在Native.Initialize/WinForms/Application.Run之前分支。

新增名称如下（动态后缀注明展开范围；只有源码计数，未执行）：

- `renderer_desktop_partial_bbox_rejected`
- `renderer_desktop_exact_component_{0..3}`：x/y/width/height 各偏移0.25，4项。
- `renderer_desktop_nonfinite_{0..3}_{0..2}`：四分量 × NaN/+Infinity/-Infinity，12项。
- `renderer_desktop_unknown_topology`
- `renderer_desktop_empty_topology`
- `renderer_desktop_unknown_member`
- `renderer_desktop_duplicate_identity_rejected`
- `renderer_desktop_missing_identity_rejected`
- `renderer_desktop_mirror_not_first_match`
- `renderer_desktop_overlap_rejected`
- `renderer_desktop_zero_screen_rejected`
- `renderer_desktop_negative_screen_rejected`
- `renderer_desktop_edge_overflow_rejected`
- `renderer_desktop_workarea_outside_rejected`
- `renderer_desktop_empty_workarea_rejected`
- `renderer_desktop_negative_surface_rejected`
- `renderer_desktop_surface_edge_overflow_rejected`
- `renderer_null_surface_unavailable`
- `renderer_desktop_null_pointer_keeps_surface`
- `renderer_desktop_nan_pointer_hidden`
- `renderer_desktop_infinite_pointer_hidden`
- `renderer_desktop_touching_edge_belongs_to_right`
- `renderer_desktop_right_edge_excluded`
- `renderer_desktop_bottom_edge_excluded`
- `renderer_desktop_fractional_rounding_not_into_gap`
- `renderer_desktop_outside_pointer_keeps_surface`
- `renderer_single_pointer_other_screen_hidden`
- `renderer_desktop_vertical_negative_origin`
- `renderer_desktop_vertical_gap_stable_anchor`
- `renderer_desktop_anchor_order_not_pointer`
- `renderer_single_vertical_bbox_not_desktop`
- `renderer_desktop_bbox_origin_itself_is_gap`
- `renderer_layout_diagonal_anchor_not_bbox_origin`
- `renderer_desktop_case_alias_identity_rejected`
- `renderer_desktop_union_span_overflow_rejected`
- `renderer_layout_96_workarea_offsets`
- `renderer_layout_negative_monitor_containment`
- `renderer_layout_high_dpi_small_area_stop_reachable`
- `renderer_layout_minimum_exact_fit`
- `renderer_layout_one_pixel_too_narrow`
- `renderer_layout_one_pixel_too_short`
- `renderer_layout_tiny_area_not_claimed_visible`
- `renderer_layout_negative_dpi`
- `renderer_layout_sub_native_dpi`
- `renderer_layout_zero_dpi`
- `renderer_layout_nan_dpi`
- `renderer_layout_infinite_dpi`
- `renderer_layout_huge_dpi_no_integer_overflow`
- `renderer_layout_unknown_screen`
- `renderer_layout_overflow_bounds`
- `renderer_layout_workarea_outside_bounds`
- `renderer_layout_dpi_containment_{0..6}`：96/120/144/168/192/240/288 DPI，7项。

52个名称字面量，展开4个exact-component、12个nonfinite、7个DPI case后为 **72项**。全量预计 **147+16+72=235 checks**。预期native exit=0、stderr `self-test: 235 checks passed (pure; no GUI/ready/input)`、stdout空；这只是待CC验证的期望。任何失败须保留原始日志交回，不修改断言掩盖失败。

## 6. Compile-only日志（非原生测试证据）

使用现有check-csharp-compile.sh的相同references/显式源列表与C#5 flags，输出在全新TEMP；没有运行该脚本默认build路径，也未覆盖部署。两次编译都未执行产物。

```sh
mcs -langversion:5 -sdk:4 -warnaserror+ -target:winexe -platform:x64 \
  -r:System.dll -r:System.Core.dll -r:System.Drawing.dll -r:System.Windows.Forms.dll \
  -out:OUTPUT desktop-feedback/windows/{JsonValue,Protocol,Model,IPC,Native,Windows,SelfTests,Program}.cs
```

- 第一次产物：`/tmp/windows-renderer-stageb-sol.PKiBdT/desktop-feedback-windows-syntax-only.exe`
- 第一次日志：`/tmp/windows-renderer-stageb-sol.PKiBdT/compile-1.log`
- **最终源码对应产物**：`/tmp/windows-renderer-stageb-sol.PKiBdT/desktop-feedback-windows-syntax-only-2.exe`
- 最终日志：`/tmp/windows-renderer-stageb-sol.PKiBdT/compile-2.log`
- 两次compiler stdout/stderr均空，追加退出信息均为：

```text
compile_exit=0
NO ASSEMBLY OR TEST EXECUTION
```

这是语法跨编译，不是Mac/Linux测试、Framework64构建或native GREEN。

## 7. 冻结SHA-256

| 文件 | SHA-256 |
|---|---|
| `desktop-feedback/windows/IPC.cs` | `f24d5b1029da08c74d99aba6ad6fbe9b2f58a9a656a4af23fbaf2156a0170ba3` |
| `desktop-feedback/windows/JsonValue.cs` | `8d23af10506240d9f72be513d3d846a73363a6691013ca885675ec5e9a03ab4a` |
| `desktop-feedback/windows/Model.cs` | `60fbf87a0ac5519cdf358a31aa56227e763bd601a26fe9525efa085bc1b79f72` |
| `desktop-feedback/windows/Native.cs` | `74e33605d0f82909bf21c3568ccf453c4bf79213ed792184598d60cef1c2b911` |
| `desktop-feedback/windows/Program.cs` | `de88c1a2170a526eabc5a425baad0672c2201298c0144a6dc24f1eedd85a8982` |
| `desktop-feedback/windows/Protocol.cs` | `265fe9dbaafb08b1a31793e14140221299fac98f0f00b4a0ee4085031cf89b31` |
| `desktop-feedback/windows/SelfTests.cs` | `c7f619d8999e658cbed4a95a32b5f741b79fc39581c133f824ec58bcb51bc588` |
| `desktop-feedback/windows/Windows.cs` | `459e1b19ff9628b47ee14633d9bd75c60595e69aa523e3bda711445b3c10c2bf` |
| `desktop-feedback/windows/app.manifest` | `4f5f2c276e99b0ada7cbd8ec34324717029642aeec86a3f1ab1eb7fe669d0089` |
| `desktop-feedback/windows/desktop-feedback-windows.exe.config` | `9d49e908618fe07944a81351c5c8e4495b9320028543d8bc7fdfdf9cb44a5c7c` |
| `desktop-feedback/build-windows.ps1` | `f2e113e490d8be515053a91a78109530b16a9b04ed2fdcc03e96198d7f86d19e` |
| `desktop-feedback/check-csharp-compile.sh` | `bfda99030e6ed5187f5662d8a9c4df2dacfdde846b579b38b559bf286f991079` |

本报告自身hash在最终交接消息给出，避免自引用hash。

## 8. 真实待CC Windows命令（本轮未执行）

由协调者安排真实Claude CLI+GLM。先将上述frozen文件放入Windows的独立SourceRoot，目录结构必须为 `SourceRoot/build-windows.ps1` 与 `SourceRoot/windows/*.cs`，不要把build脚本放在windows子目录而形成windows/windows误路径。先逐项核对上述hash，任何不一致停止；不复用旧部署。下面脚本可由CC保存/执行，参数为该已核对的SourceRoot：

```powershell
param([Parameter(Mandatory=$true)][string]$SourceRoot)
$ErrorActionPreference = 'Stop'
$run = Join-Path $env:TEMP ('windows-renderer-desktop-green-cc-' + [guid]::NewGuid().ToString('N'))
$null = New-Item -ItemType Directory -Path $run
Copy-Item -LiteralPath (Join-Path $SourceRoot 'windows') -Destination (Join-Path $run 'windows') -Recurse
Copy-Item -LiteralPath (Join-Path $SourceRoot 'build-windows.ps1') -Destination (Join-Path $run 'build-windows.ps1')
Get-ChildItem -LiteralPath (Join-Path $run 'windows') -File | Get-FileHash -Algorithm SHA256 | Format-List | Out-File (Join-Path $run 'source-hashes.txt')
Get-FileHash -LiteralPath (Join-Path $run 'build-windows.ps1') -Algorithm SHA256 | Format-List | Out-File (Join-Path $run 'source-hashes.txt') -Append
$exe = Join-Path $run 'build/desktop-feedback-windows.exe'
$buildScript = Join-Path $run 'build-windows.ps1'
$ps = Join-Path $env:WINDIR 'System32/WindowsPowerShell/v1.0/powershell.exe'
# Child PowerShell preserves numeric native build exit even on compiler stderr.
$ErrorActionPreference = 'Continue'
& $ps -NoProfile -ExecutionPolicy Bypass -File $buildScript -Output $exe > (Join-Path $run 'build.stdout') 2> (Join-Path $run 'build.stderr')
$buildExit = $LASTEXITCODE
$ErrorActionPreference = 'Stop'
Write-Output "run=$run"
Write-Output "build_exit=$buildExit"
Get-Content -LiteralPath (Join-Path $run 'build.stdout')
Get-Content -LiteralPath (Join-Path $run 'build.stderr')
if ($buildExit -ne 0) { exit $buildExit }
Get-FileHash -LiteralPath $exe -Algorithm SHA256 | Format-List
$p = Start-Process -FilePath $exe -ArgumentList '--self-test' -Wait -PassThru -RedirectStandardOutput (Join-Path $run 'self-test.stdout') -RedirectStandardError (Join-Path $run 'self-test.stderr')
Write-Output "test_exitcode=$($p.ExitCode)"
Write-Output '--- stderr ---'
Get-Content -LiteralPath (Join-Path $run 'self-test.stderr')
Write-Output '--- stdout ---'
Get-Content -LiteralPath (Join-Path $run 'self-test.stdout')
exit $p.ExitCode
```

CC应将数字build/test exit、完整stdout/stderr、源/产物hash及run目录写入新的独立GREEN或失败报告；保留每次attempt，不覆盖旧RED。此脚本只运行纯--self-test，不授权普通renderer/Host/GUI/截图/输入。任何后续GUI验证仍需单独授权。
