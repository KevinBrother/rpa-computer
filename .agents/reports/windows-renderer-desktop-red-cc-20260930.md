# Windows renderer Desktop baseline RED — CC native verification (2026-09-30)

状态：**RED_CONFIRMED**。acer-win 原生 Framework64 csc 构建 exit=0；`--self-test` 原生 exitcode=1，stderr 为 `self_test_renderer_desktop_negative_origin_dual_screen_bbox`，stdout 空。与 SOURCE_FROZEN 报告 `windows-renderer-desktop-red-sol-20260930.md` 的首个静态预期失败**逐字一致**。仅执行 self-test；无 GUI/renderer/Host/截图/输入；源码零改动、零 commit/push/worktree/subagent。

## 冻结源核对（Mac 侧 SHA-256，上传后远端复核一致）

全部 11 个文件与冻结报告 hash 完全一致：

| 文件 | SHA-256 | 冻结报告 |
|---|---|---|
| windows/IPC.cs | f24d5b1029da08c74d99aba6ad6fbe9b2f58a9a656a4af23fbaf2156a0170ba3 | 一致 |
| windows/JsonValue.cs | 8d23af10506240d9f72be513d3d846a73363a6691013ca885675ec5e9a03ab4a | 一致 |
| windows/Model.cs | 06f2ca22dd8ab2d1aac83b08b006a6fd6129f59b368289d19ec0b79b05406669 | 一致 |
| windows/Native.cs | 74e33605d0f82909bf21c3568ccf453c4bf79213ed792184598d60cef1c2b911 | 一致 |
| windows/Program.cs | 3afb0375d09bf523131a24e6f921cfe095fedb5148a9f2f3e9f551d3ff7ba675 | 一致 |
| windows/Protocol.cs | 265fe9dbaafb08b1a31793e14140221299fac98f0f00b4a0ee4085031cf89b31 | 一致 |
| windows/SelfTests.cs | 0a9dae5e76bc45c3b3ea868e07851d178d60c491fb4f30009f8b820be15fd391 | 一致 |
| windows/Windows.cs | 77b686394c035d4e9ffb840821ed0e126dcf1e9dc69e209c45a28e614be85025 | 一致 |
| windows/app.manifest | 4f5f2c276e99b0ada7cbd8ec34324717029642aeec86a3f1ab1eb7fe669d0089 | 一致 |
| windows/desktop-feedback-windows.exe.config | 9d49e908618fe07944a81351c5c8e4495b9320028543d8bc7fdfdf9cb44a5c7c | 一致 |
| build-windows.ps1 | f2e113e490d8be515053a91a78109530b16a9b04ed2fdcc03e96198d7f86d19e | 一致 |

上传后 acer-win 侧 `Get-FileHash` 复核（`upload-listing.txt`）与上述一致（大写格式差异除外）。

## 执行环境

- 目标机：`acer-win`（SSH BatchMode）。全新 TEMP：`C:\Users\Administrator\AppData\Local\Temp\windows-renderer-desktop-red-cc-20260930-203948-4959`
- 仅执行 `--self-test`（Program.cs 在 Native.Initialize/WinForms/HWND 之前分支）；未触碰 Notepad24332、任何运行中的 GUI、Host、计划任务。
- Mac 侧原始证据：`.agents/runs/windows-renderer-desktop-red-cc-20260930/`（tempdir、upload-listing、runner 脚本、self-test-windows.output、ssh.stderr 等）。
- `.agents/runs/windows-renderer-desktop-red-cc-20260930.jsonl` 是此前另一会话遗留 transcript，本次未改动。

## 构建与测试（原生数字证据）

- 构建：`powershell -NoProfile -ExecutionPolicy Bypass -File <TEMP>\build-windows2.ps1 -Output <TEMP>\build2\desktop-feedback-windows.exe`，Framework64 v4.0.30319 csc，`/langversion:5 /target:winexe /platform:x64 /optimize+ /warnaserror+`
- **build_exit=0**；build log：`Built ...\build2\desktop-feedback-windows.exe (not launched; not acceptance)`
- exe SHA-256：`1AE3CFA362038253792CE856C7A2BA38B8946659DCF1D43C8090B4F71B0D2B0C`
- 测试：`Start-Process -FilePath <exe> -ArgumentList '--self-test' -Wait -PassThru -RedirectStandardOutput/-RedirectStandardError`
- **test_exitcode=1**
- **stderr（完整原文）：`self_test_renderer_desktop_negative_origin_dual_screen_bbox`**
- **stdout：空**

## 计数核对（区分旧/新，不虚构未执行数量）

新类注册顺序位于旧 147 checks 与 StrictProtocolCases **之后**（见冻结报告 §3）。fail-fast 在第一个新 check 处终止，结构性证明：

- **旧 147 checks：全部真实执行并通过**（否则不会到达新类）。
- **新 16 项中前 12 项 positive 回归：全部真实执行并通过**（首败是第 13 项）。
- **第 13 项 `renderer_desktop_negative_origin_dual_screen_bbox`：实测失败（预期 RED）**——双屏 bbox=(-1920,-200,4480,1640) 不匹配任何单屏，纯 resolver 返回 null。
- **末 3 项被 fail-fast 遮蔽，未执行**；本报告对其通过/失败**不做任何计数声明**。

若未来实现 StageB 后全量通过，预期总数 147+16=163（当前未验证，仅为冻结报告声明）。

## 尝试记录（全部保留，未删除/重建同一 OutDir）

1. **Attempt 1（失败，保留证据）**：runner（`self-test-windows.runner.ps1`，已保留）放置于 `<TEMP>\windows\` 内调用，build-windows.ps1 以 `$PSScriptRoot\windows\` 解析源路径导致 `windows\windows\*.cs` CS2001；且该 runner `$ErrorActionPreference='Stop'` 把 csc stderr 变为终止错误，未及输出 build_exit（ssh stderr 原文要点：`powershell : csc compilation failed (1)` / NativeCommandError）。attempt 1 的 Mac 侧 stdout/stderr 文件被 attempt 2 的重定向覆盖，未保留副本；远端 `<TEMP>\build\build.log`（CS2001 ×8 + warning CS2008）为该次原始证据，未删除。仅修正调用方式，未改任何源/脚本。
2. **Attempt 2（有效）**：脚本复制到 `<TEMP>\build-windows2.ps1`（与 `windows\` 同级），输出全新 `<TEMP>\build2\`，`$ErrorActionPreference='Continue'` + `*> $buildLog` 立即捕获数字 `$LASTEXITCODE`。结果如上。

## 边界声明

- 纯 layout self-test **不是**渲染 GUI 验收；真实多屏/DPI/Stop 取消链/capture exclusion 仍 pending。
- 未实现 StageB（desktop 分支、bbox/union 匹配、拓扑拒绝、gap ring 门控）；本 RED 不授权实现，待协调者明确 StageB gate。
