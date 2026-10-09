# CC+GLM Windows pure-topology verification — display-topology

日期：2026-09-30。任务：Windows 目标机（acer-win）纯测试验证 `crates/display-topology`（实现者报告：`.agents/reports/display-topology-core-sol-20260930.md`）。
运行日志：`.agents/runs/display-topology-windows-cc-20260930/`（compiler-artifacts.jsonl、build.log、local-hashes.txt、四个 exe 的 .out/.err）。

## 范围与写集

- 只写本报告与 `.agents/runs/display-topology-windows-cc-20260930/`，以及 acer-win 新 TEMP 目录 `C:\Users\Administrator\AppData\Local\Temp\display-topology-cc-20260930-1`。
- 未改任何产品源码/测试/配置/root 工程；未 commit/push/worktree；未启动 Host/fixture；无 GUI/截图/输入/显示设置改动；未运行 `--ignored`。SSH 仅部署与运行纯测试 exe。
- 本地只做交叉编译（`--no-run`），无 Mac/Linux 测试执行。

## 静态审查（非执行证据）

- `src/lib.rs`：`#![forbid(unsafe_code)]`、`#![deny(missing_docs)]`，crate 声明不含枚举/截图/像素分配/注入/sleep/取消。
- grep 全部 src/tests：无 `std::process`、无网络、无 OS API（CGDisplay/user32/winapi）、无 `thread::sleep`、无子进程调用。测试为纯函数级，符合"纯测试可直接 SSH 执行"的前提。
- 测试计数核对：21 integration（topology 4 + capture 7 + mapping_drag 10）+ 库内 unit 1（src/topology.rs）= 22，与实现者报告一致。
- 独立 `[workspace]`（crates/display-topology/Cargo.toml），未触碰 root 工程。

## 交叉编译（Mac → x86_64-pc-windows-msvc）

按 docs/remote-connection.md 与 crate README 的 cargo-xwin / rust-lld 方案，未执行任何本地 `cargo test`（仅 `--no-run`）：

```sh
eval "$(cargo xwin env --target x86_64-pc-windows-msvc)"
export DYLD_LIBRARY_PATH="$(rustc --print sysroot)/lib"
export AR_x86_64_pc_windows_msvc="$(rustc --print sysroot)/lib/rustlib/aarch64-apple-darwin/bin/rust-lld"
export ARFLAGS_x86_64_pc_windows_msvc='-flavor link /lib'
cargo test --manifest-path crates/display-topology/Cargo.toml \
  --target x86_64-pc-windows-msvc --no-run --offline --message-format=json
```

本地退出码 **0**。产物路径取自本次 JSON（`profile.test=true` 且 `.exe`，非目录 glob）：

| target | exe |
| --- | --- |
| rpa_display_topology | `crates/display-topology/target/x86_64-pc-windows-msvc/debug/deps/rpa_display_topology-c3ea4c766536e049.exe` |
| topology | `.../topology-2d98b6f81f25ff24.exe` |
| capture | `.../capture-76f3887d89e17295.exe` |
| mapping_drag | `.../mapping_drag-13f2fb91e69584c9.exe` |

## 部署与 hash 校验

- 远端新建唯一目录：`C:\Users\Administrator\AppData\Local\Temp\display-topology-cc-20260930-1`（scp 仅四个 exe，无其他文件）。
- 本地 SHA256（local-hashes.txt）与远端 `Get-FileHash` 逐一一致（大小写不敏感）：

```
rpa_display_topology: 585bb572b8a53455550fdb9622bd8abe7b59815f14108e4d647408c0f50a68c0
capture:              af91f23b07da3008f0002758750b15a6762593030ec9fb6e80abf82479c37f62
topology:             37427d275feeddac7e98744dd229213d992a00a2d1b8a51f4656ddea6438491b
mapping_drag:         1ce35d98f91886fe8f487844fe8539412fa0669a8bcfcd348d3dea1cafb2c5b6
```

## Windows 目标机执行（acer-win）

命令（逐个，无 `--ignored`）：

```powershell
& C:\Users\Administrator\AppData\Local\Temp\display-topology-cc-20260930-1\<exe> --test-threads=1 --nocapture
exit $LASTEXITCODE
```

| exe | exit | passed | failed | ignored |
| --- | --- | --- | --- | --- |
| rpa_display_topology-…049.exe | 0 | 1 | 0 | 0 |
| topology-…f24.exe | 0 | 4 | 0 | 0 |
| capture-…295.exe | 0 | 7 | 0 | 0 |
| mapping_drag-…4c9.exe | 0 | 10 | 0 | 0 |

**实际合计：22 passed；0 failed；0 ignored**（逐 exe 实测计数相加，非假设；与预期 22 一致）。完整 stdout/stderr 与 exit 保存于 runs 目录四个 `.out`/`.err`。

以上是"交叉编译成功"与"Windows 目标机（Windows 11 Pro, NODE1）原生执行成功"两个独立事实，均已达成。

## 验收限制（不做过度声明）

1. 本轮全部为纯逻辑测试。绿了不证明：真实 Windows/macOS/Linux display enumeration、真实逐屏 capture、rotation 朝向、像素合成/过滤/编解码、Host/Runtime selection/generation 接线、`multi-01..12` 真机双屏行为或跨屏拖拽。
2. 静态审查未发现纯度违规（无 unsafe/进程/网络/OS 调用）；本轮无执行期失败，无红灯需交回实现者。
3. 未运行任何 `#[ignore]` 测试；未改动显示拓扑或启动 Host。
