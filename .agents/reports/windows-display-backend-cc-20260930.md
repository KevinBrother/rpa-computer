# Windows display backend CC+GLM 纯测试验证报告

日期：2026-09-30。实现者冻结报告：`.agents/reports/windows-display-backend-sol-20260930.md`（§9 SOURCE_FROZEN）。
原始证据：`.agents/runs/windows-display-backend-cc-20260930/`。执行模型 GLM（glm-5.3-flash）。

## 1. 结论

**三个纯测试 exe 全部通过：31 passed / 0 failed / 0 ignored，无重跑变绿。**

| 二进制（profile.test artifact） | 本地 = 远端 SHA256 | 结果 | native exit |
|---|---|---|---|
| `rpa_windows_display-157891d661a3095f.exe`（lib unit） | `21f2c92684fa41f69e465ae159e264760a93ccfbbbee1f481dae39ad62d48c7e` | **11 passed; 0 failed; 0 ignored**；0.05s | 0 |
| `pipeline-1109899243bd42f1.exe`（integration） | `4ca91fe452f5f4db805af625fc45053e317c4b4db55190bba1b3bd6d9ade21ea` | **15 passed; 0 failed; 0 ignored**；0.07s | 0 |
| `pixels-86992d531c6d3836.exe`（integration） | `cd50115e66cf9e8a95724c47c1050a452ad845aca2270cef02a3d99742c453ef` | **5 passed; 0 failed; 0 ignored**；0.04s | 0 |

与实现者报告的 31 个测试源码（11 unit + 15 pipeline + 5 pixels）完全一致。无失败，无修复，无跳过。

## 2. 源冻结核验

- 运行前逐文件比对实现者 §9 的 21 个 SHA-256：**21/21 一致**；清单整体 SHA-256 `8119844f82ac1229c3a45536042b61b528547cf1298b177533cc6597db1366b7` 一致。
- 运行后重新生成同一清单再次比对：**diff 为空，清单 SHA 仍为 `8119844f…`**。源码/测试零改动。

## 3. 测试纯度确认（源码阅读）

- `tests/pipeline.rs` 15 个测试使用文件内自定义 `struct Provider`（impl `FrameProvider` 假源），不实例化 `GdiProvider`。
- `tests/pixels.rs` 5 个测试为纯函数级断言。
- 11 个 unit 测试位于 dpi/identity/mode/owner，均为纯判定/RAII 生命周期。
- 全 crate grep：`attach`/`EnumDisplayMonitors`/`BitBlt`/`CreateDIBSection` 仅出现在 `src/native/**` 真实实现；测试源无 `#[ignore]`、无环境变量依赖、无 GUI/OS 输入/真实显示枚举。运行 `--test-threads=1 --nocapture`，未加 `--ignored`。

## 4. 构建与执行事实

- 构建（Mac 交叉，仅链接不运行）：`cargo test --manifest-path crates/windows-display/Cargo.toml --tests --target x86_64-pc-windows-msvc --locked --no-run --message-format=json`，exit 0。注：任务原文的 `cargo build --no-run` 不是有效参数，改用 `cargo test --no-run`（等价：构建不运行）。
- 环境：复用 `build-env.sh`（cargo xwin），新 `CARGO_TARGET_DIR=.agents/runs/windows-display-backend-cc-20260930/target`，未触碰其他 crate 的产物。
- 远端：acer-win 新唯一 TEMP `C:\Users\Administrator\AppData\Local\Temp\wdisp-cc-20260930-201713-99154`（新建，无覆盖）；certutil SHA256 3/3 与本地一致。
- 执行方式（按可靠性要求）：子脚本 `run-one.ps1` 直接 `&` 调用 exe 并立即把 `$LASTEXITCODE` 写入 marker 文件；父脚本 `supervise-one.ps1` `Start-Process -PassThru`，等待前记录 PID/ExecutablePath/CreationDate/SessionId，180s 上限，超时先按 PID+路径+CreationDate 复核身份再 kill。三者在数秒内完成，未触发超时。
- **ExitCode 证据形态**：`$p.ExitCode` 再次为空（与既往 Windows 运行一致的可信缺陷），三个二进制的通过判定均取自子脚本 marker（`marker=0`）+ supervisor 退出码 0，未把 null 当作通过。每个 exe 的完整 stdout/stderr 已存 `remote-logs/`。

## 5. 边界（如实声明）

- 纯 provider 测试全绿**不证明**：真实 GDI 枚举/截图、真实多屏拓扑、DPI/旋转实机行为、HMONITOR 身份核验、资源释放实机路径、指针映射、renderer 接线、root 集成、GUI 验收、性能/编码耗时。
- 未运行任何 desktop/Host/fixture/交互任务；未触碰 Notepad24332、既有 Host/fixture/tasks、凭据/设置；未使用 Session1；Mac/Linux 上除交叉编译外无任何执行。
- 未做 commit/push/worktree；未改任何产品/测试源码。
- 执行进程均为 SSH 会话 0 下的 powershell 子进程（proc-info 记录见 `remote-logs/*.proc-info.txt`）；纯测试无需交互 desktop。

## 6. 原始产物清单

```
.agents/runs/windows-display-backend-cc-20260930/
├── compiler-artifacts.jsonl        # 构建消息流（含 3 个 profile.test exe）
├── build.log                        # 构建原始 stderr（首次 cargo build --no-run 参数错误亦留痕）
├── local-exe-sha256.txt             # 本地 exe SHA256
├── remote-dir.txt                   # 远端 TEMP 路径
├── remote-run-evidence.txt          # 远端回读汇总
├── remote/                          # 上传的 run-one.ps1 / supervise-one.ps1
├── remote-logs/                     # 每个 exe 的 stdout/stderr/exit marker/proc-info/exit-summary
└── target/                          # 本次独立编译产物
```
