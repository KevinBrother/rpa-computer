# Windows renderer Desktop — StageA frozen handoff (2026-09-30)

状态：**STAGE_A_FROZEN / EXPECTED_RED / CC_NATIVE_RED_PENDING**。本报告不是实测 RED/GREEN，未执行任何测试或程序集。等待真实 Claude CLI + GLM 在 Windows 上验证 RED，再由协调者明确授权 StageB；现在停止源码写入。

## 范围与实现

仅改动以下路径（保留既有脏树，无 commit/push/reset/worktree/subagent/CC）：

1. `desktop-feedback/windows/Model.cs`：独立值类型 `ScreenFacts`、`ScreenResolution` 和纯 `ScreenResolver.Resolve(Surface, Pointer, IList<ScreenFacts>)`。保留原来的依次首个匹配、四分量绝对差严格 `<0.5`、pointer 以 Surface 半开矩形判断的行为。无 Screen.AllScreens、Win32、Native 或 WinForms 调用。
2. `desktop-feedback/windows/Windows.cs`：现有 Render 的单屏匹配调用纯 resolver；真实枚举只留在 GUI adapter，并按事实对象对应原 Screen。fallback、Place/PMv2 DPI、ring 绘制、窗口样式、Stop callback/取消门控、affinity 均未改语义。
3. `desktop-feedback/windows/SelfTests.cs`：新增独立 `RendererDesktopSelfTests` 类，并在旧 147 checks 和 StrictProtocolCases 后注册。Program.cs 未改，--self-test 仍在 Native.Initialize / WinForms / HWND 之前分支。
4. `.agents/reports/windows-renderer-desktop-red-sol-20260930.md`：本冻结交接报告。

两个现有构建脚本显式枚举源码，本阶段不获授权修改范围外的脚本。因此独立小类放在已有编译单元，而非新增无法被原生脚本包含的 cs 文件。Model.cs 103 行，SelfTests.cs 223 行，Windows.cs 175 行。

feedback v1 字段、Surface 序列化与默认行为、Rust/Host/root、macOS、fixture、部署均未改。当前仍不区分 desktop ID，只匹配单屏几何；这是 StageA 原行为，不是 StageB。没有硬编码 false/null 来制造 RED。

## 旧基线与保留证明

已读取 CONTRACT.md 最新 multiscreen amendment、计划 locked choices + Task5、Windows 源码，以及 renderer-strictness-windows-cc-red/green-20260930 报告。CC 旧 GREEN 记录：原生 Framework64 --self-test exit=0、147 checks。该历史事实不能代替本次测试。

将本次新增类与一行注册从 SelfTests.cs 文本剔除，原文件重构 SHA-256 为 `e7430d8980c3d4b7b756621d98b4088c4bc1a7c41466bc067d42081a4b377cfb`，精确等于旧 CC GREEN SelfTests.cs hash；未删除/弱化原断言。此操作只是文本/hash 核对，非测试执行。

## 新增纯 tests（16 checks，按执行顺序）

1. `renderer_single_exact_negative_origin`
2. `renderer_single_pointer_inside`
3. `renderer_single_preserves_sub_half_pixel_tolerance`
4. `renderer_single_rejects_half_pixel_boundary`
5. `renderer_single_unmatched_geometry`
6. `renderer_single_preserves_first_match`
7. `renderer_single_pointer_right_edge_excluded`
8. `renderer_single_null_pointer_hidden`
9. `renderer_other_id_bbox_not_desktop`
10. `renderer_desktop_id_case_sensitive`
11. `renderer_desktop_id_no_prefix_inference`
12. `renderer_legacy_single_desktop_id_matches_geometry`
13. `renderer_desktop_negative_origin_dual_screen_bbox`
14. `renderer_desktop_anchor_real_controlled_workarea`
15. `renderer_desktop_pointer_on_real_screen`
16. `renderer_desktop_gap_pointer_rejected_without_losing_surface`

前 12 项是旧 single 行为及非 desktop ID 的 positive/拒绝回归，静态预计通过；最后 4 项是未实现的 Desktop 要求，静态预计失败。若未来全量通过，总数应为 **147 + 16 = 163**；现在 fail-fast 不会跑完。

双屏固定 facts：left bounds=(-1920,-200,1920,1080)，workarea=(-1920,-200,1920,1040)；right bounds=(0,0,2560,1440)，workarea=(0,0,2560,1400)。Desktop bbox=(-1920,-200,4480,1640)。pointer=(-1800,-100) 在真实 left；gap pointer=(-100,1200) 在 bbox 内但不在任一屏。

**首个静态预期失败**：`self_test_renderer_desktop_negative_origin_dual_screen_bbox`；Main 现有 WireFailure 分支预期 exit=1、stderr 为该代码、无成功总数，stdout 空。原因是该 bbox 不匹配任何单屏，纯 resolver 返回 null。不是 parser failure 或 GUI 初始化 failure。

后续 3 项（被 fail-fast 遮蔽，未实测）：anchor 要求返回的真实受控屏工作区在其 Bounds 内；真实屏 pointer 应获准；gap pointer 要求合法 Desktop resolution 非 null 且 CanShowPointer=false，不能通过“拒绝整个 surface”冒充正确 gap 判断。本阶段全部因缺少 Desktop resolution 而失败。anchor check 不指定 primary 或按 pointer 选屏策略，也不声称已经覆盖最终 DPI 缩放后的 Stop HWND 边界。

**没有实现的 StageB**：精确 desktop 分支、全部 active screen bbox/union 匹配、overlap/nonfinite/unknown 拓扑拒绝、Desktop 的真实 controlled 工作区选择和 gap ring 门控。应在 CC 实测 RED + 协调者授权之后实现，不因静态预计而越过 gate。

## Compile-only（非 Windows 验收）

仅使用现有 check-csharp-compile.sh 同样的 mcs C#5 参数、references 和 8 个明确源码文件；输出改为全新 TEMP，未运行脚本的固定 build 输出路径，未覆盖旧 exe/部署。未执行 Mono、exe、--self-test 或其他 tests。

编译命令（实际输出路径如下）：

```sh
mcs -langversion:5 -sdk:4 -warnaserror+ -target:winexe -platform:x64 \
  -r:System.dll -r:System.Core.dll -r:System.Drawing.dll -r:System.Windows.Forms.dll \
  -out:OUTPUT desktop-feedback/windows/{JsonValue,Protocol,Model,IPC,Native,Windows,SelfTests,Program}.cs
```

- OUTPUT：`/tmp/windows-renderer-stagea-sol.i4uFAM/desktop-feedback-windows-syntax-only.exe`
- compile log：`/tmp/windows-renderer-stagea-sol.i4uFAM/compile.log`
- 完整日志（compiler 自身 stdout/stderr 为空；追加退出信息）：

```text
command: mcs -langversion:5 -sdk:4 -warnaserror+ -target:winexe -platform:x64 (existing script references/source list; fresh TEMP output)
compile_exit=0
NO ASSEMBLY OR TEST EXECUTION
```

这是本地跨平台语法编译，不是 Mac/Linux 测试，也不是 Windows Framework64 原生构建或验收。

## 冻结 SHA-256

以下全部冻结文件 hash；真正源码改动仅上述三个 cs，其余作为 unchanged 边界证据。

| 文件 | SHA-256 |
|---|---|
| `desktop-feedback/windows/IPC.cs` | `f24d5b1029da08c74d99aba6ad6fbe9b2f58a9a656a4af23fbaf2156a0170ba3` |
| `desktop-feedback/windows/JsonValue.cs` | `8d23af10506240d9f72be513d3d846a73363a6691013ca885675ec5e9a03ab4a` |
| `desktop-feedback/windows/Model.cs` | `06f2ca22dd8ab2d1aac83b08b006a6fd6129f59b368289d19ec0b79b05406669` |
| `desktop-feedback/windows/Native.cs` | `74e33605d0f82909bf21c3568ccf453c4bf79213ed792184598d60cef1c2b911` |
| `desktop-feedback/windows/Program.cs` | `3afb0375d09bf523131a24e6f921cfe095fedb5148a9f2f3e9f551d3ff7ba675` |
| `desktop-feedback/windows/Protocol.cs` | `265fe9dbaafb08b1a31793e14140221299fac98f0f00b4a0ee4085031cf89b31` |
| `desktop-feedback/windows/SelfTests.cs` | `0a9dae5e76bc45c3b3ea868e07851d178d60c491fb4f30009f8b820be15fd391` |
| `desktop-feedback/windows/Windows.cs` | `77b686394c035d4e9ffb840821ed0e126dcf1e9dc69e209c45a28e614be85025` |
| `desktop-feedback/windows/app.manifest` | `4f5f2c276e99b0ada7cbd8ec34324717029642aeec86a3f1ab1eb7fe669d0089` |
| `desktop-feedback/windows/desktop-feedback-windows.exe.config` | `9d49e908618fe07944a81351c5c8e4495b9320028543d8bc7fdfdf9cb44a5c7c` |
| `desktop-feedback/build-windows.ps1` | `f2e113e490d8be515053a91a78109530b16a9b04ed2fdcc03e96198d7f86d19e` |
| `desktop-feedback/check-csharp-compile.sh` | `bfda99030e6ed5187f5662d8a9c4df2dacfdde846b579b38b559bf286f991079` |

## CC 交接（仅供协调者分配，不由实现者执行）

1. 上传/复制这组 frozen Windows 源及 unchanged build-windows.ps1 至全新 Windows TEMP；先核对 hash，保留旧部署与 raw RED/GREEN 日志。
2. 使用原 build-windows.ps1 原生 Framework64 csc /langversion:5 /warnaserror+ 构建，输出至该 TEMP，记录数字 build exit 和产物 hash。
3. 仅执行 --self-test，无普通 renderer/Host/GUI/截图/输入，记录数字 native exit、完整 stdout/stderr 和 frozen source hash。首个预期失败见上；不删/跳过断言来转绿。
4. 返回真实 RED 证据给协调者；StageB 待明确授权。真实 GUI、DPI、capture exclusion、Stop 取消链仍 pending，本报告不替代验收。

未开启 SSH/窗口/截图/输入；未新开 Mac/Linux tests。当前运行模型配置无法由实现者切换或核实，不虚报已使用指定模型身份。
