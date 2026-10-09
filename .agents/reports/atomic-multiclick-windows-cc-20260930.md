# 原子多击 Windows 真实构建与无 GUI 纯测试（2026-09-30, CC+GLM）

只写本报告与 `.agents/runs/atomic-multiclick-windows-cc/`、Windows 侧 TEMP 专用目录。未修任何产品源码/测试/脚本，未 commit/push，未启动 Host/fixture，未做任何 GUI 输入/截图，未创建/停止服务或计划任务，未触碰用户 Notepad(pid 24332)。SSH 仅部署与运行纯测试 exe。

## 1. 测试代码审查（步骤1：通过，未停止）

- `crates/native-input` 全 crate `#[ignore]` 计数 = **0**，无非 ignored 的 GUI 注入测试。
- Windows 相关测试：
  - `windows/builder.rs` tests：25 个纯 EventSpec 规划测试（无 Win32 调用）。
  - `windows.rs` tests（7 个）：仅调用 `to_input` 构造 INPUT 记录并断言字段；唯一 FFI 是无副作用的 `MapVirtualKeyW`；**不调用 SendInput**（代码注释与实现核对一致）。
  - `session.rs` tests：20 个纯 mock 状态机测试。
- `lib.rs:29` 通过 `#[cfg(all(test, not(target_os = "windows")))]` 把真实 SendInput planner 的纯测试也编入非 Windows 主机。
- 无截图/capture/PostMessage/mouse_event 调用。
- **结论：非 ignored 用例均为纯测试，可安全在目标机执行。**

## 2. 实际构建（步骤2：均 exit 0）

环境：本机既有 cargo-xwin 0.23.1 + rust-lld 方案（docs/remote-connection.md 原命令，未安装新工具）：

```sh
eval "$(cargo xwin env --target x86_64-pc-windows-msvc)"
export DYLD_LIBRARY_PATH="$(rustc --print sysroot)/lib"
export AR_x86_64_pc_windows_msvc="$(rustc --print sysroot)/lib/rustlib/aarch64-apple-darwin/bin/rust-lld"
export ARFLAGS_x86_64_pc_windows_msvc='-flavor link /lib'
```

| 命令 | exit | 日志 |
|---|---|---|
| `cargo test --manifest-path crates/native-input/Cargo.toml --target x86_64-pc-windows-msvc --no-run --message-format=json` | 0（`Finished \`test\` profile ... in 3.14s`） | `native-win-test-json.log` / `native-win-test-stderr.log` |
| `cargo build --release --target x86_64-pc-windows-msvc --bins --offline --message-format=json` | 0（`Finished \`release\` profile ... in 34.44s`） | `root-win-release-json.log` / `root-win-release-stderr.log` |

root release bins 产物：`target/x86_64-pc-windows-msvc/release/computer-host.exe`、`computer-client.exe`（**仅构建，未部署、未运行**）。历史 root 全量 release 的 llvm-lib 坑未复现（本次走 rust-lld AR/ARFLAGS 方案）。

## 3. 测试 exe 定位与部署（步骤3）

- exe 路径**取自本次 cargo JSON compiler-artifact**（非目录挑选）：
  `crates/native-input/target/x86_64-pc-windows-msvc/debug/deps/rpa_native_input-31ef65e8abc01d82.exe`
- 本机 SHA-256：`8b233ac52a414300baf68d5380222dd5af006cc6662e81391d70044f411f97f7`
- 远端 TEMP：`C:\Users\Administrator\AppData\Local\Temp`
- **新建唯一目录**：`C:\Users\Administrator\AppData\Local\Temp\computer-multiclick-cc-20260930-20260930-180928`（scp 仅该测试 exe，无其他文件）
- 远端 `Get-FileHash` SHA-256：`8B233AC5...F97F7` —— 与本机一致（大小写差异仅 PowerShell 输出格式）。

## 4. Windows 真实运行（纯测试，无 --ignored/--include-ignored）

SSH 直接执行，未创建交互 ScheduledTask（本任务为无 GUI 纯测试，不需要）：

```
powershell -NoProfile -Command "& C:\...\computer-multiclick-cc-20260930-20260930-180928\rpa_native_input-31ef65e8abc01d82.exe; exit $LASTEXITCODE"
```

- **exit = 0**
- **`test result: ok. 57 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s`**
- stdout 完整保存：`.agents/runs/atomic-multiclick-windows-cc/native-win-test-run-stdout.log`；stderr：`native-win-test-run-stderr.log`
- 覆盖模块：session、windows::builder、windows::tests（含 unicode wVk=0、wheel 拒绝零/溢出、扩展键 flag、绝对虚拟桌面 move 字段等回归）。

## 5. 声明与边界

- **以上是"Windows 目标机真实运行成功"**：测试 exe 在 acer-win（Windows 11 Pro, NODE1）原生执行且全部通过——不是"交叉编译成功"。
- root Windows release `--bins` 仅完成交叉构建（exit 0），**未在 Windows 上运行、未验收**。
- **GUI 未验收**：本任务未执行任何双击/三击/拖拽真实 GUI 操作、未启动 Host、未截图。多击真实语义验收由协调者另行调度。
- 失败项：无。
