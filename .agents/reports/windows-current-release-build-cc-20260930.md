# Windows 当前冻结产品 release 交叉编译 gate — CC

日期：2026-09-30。执行者：CC（Claude）。仅编译门禁，无 GUI/Host/网络/真实捕获/输入/SSH/部署；产物未执行。

## 结论

**cargo build（Windows MSVC release --bins）exit 0，两个产品 bin 编译成功并通过 PE 架构静态核对。**编译成功≠GUI 或 remote 连接验收；本轮不发布、不 staging 到 Windows。

## 构建

- 源冻结核对：`.agents/runs/windows-root-race-repair-sol-20260930/source-freeze.sha256`（177 条，含根 Cargo.toml/lock 及 4 个 crate Cargo）构建前/后各核对一次，均 **177/177 OK**（mismatch=0）。
- 环境：`source .agents/runs/desktop-feedback-host-windows-cc-20260930/logs/build-env.sh` 后立刻 `export CARGO_TARGET_DIR=/private/tmp/windows-current-release-build-cc-20260930.xKbMwg/target`（NEW，mktemp 唯一，/private/tmp 仅作工作卷）+ `CARGO_INCREMENTAL=0`。
- 命令：`cargo build --target x86_64-pc-windows-msvc --release --bins --locked --offline --message-format=json`
- **数字退出码：0**（`logs/build.exit`）。原始 JSON：`logs/compiler-artifacts.jsonl`（120 条 compiler-artifact）；stderr：`logs/build.log`（无 error/warning 级别行）。磁盘前后：`logs/disk-before.txt` / `disk-after.txt`（Data 卷余约 27GB，未触 3GB 限制）。

## 精确产物（来自 compiler JSON `target.kind=["bin"]` / `profile.release`（opt_level=3, debug_assertions=false）/ `executable` 路径，非 glob）

| bin（Cargo target 名） | 精确路径 | 大小 | SHA256 | PE 架构 |
|---|---|---|---|---|
| `computer-host` | `/private/tmp/windows-current-release-build-cc-20260930.xKbMwg/target/x86_64-pc-windows-msvc/release/computer-host.exe` | 5,673,472 B | `206dbf999de9713e7ce7cb96e2faee2a8a3b4233f02504699f183d06b166d593` | PE32+ console x86-64（`file` + 直读 MZ/PE 头 machine=0x8664） |
| `computer-client` | `/private/tmp/windows-current-release-build-cc-20260930.xKbMwg/target/x86_64-pc-windows-msvc/release/computer-client.exe` | 1,730,048 B | `5a30f6d8bf5bbf386fc87b9178668dd37778c115f369021d324112bfac362141` | PE32+ console x86-64（同上） |

两个 bin 均为全新路径（本轮独占 target），未触碰/覆盖任何旧 release、fixed exe、GUI baseline。JSON 中 `rpa_computer`/`rpa_display_topology` 等均为 lib artifact，两个可执行产物即上表两项。

## 边界 / Pending

- 未运行任何 cargo test / 本地 exe / 远端执行；未 staging 到 Windows；未打包发布。
- 不构成 GUI 验收、remote 连接验证、物理多屏或 feedback Stop 证据；GUI gate 仍 blocked。
- geometry/native C# 侧报告独立存在，未与本轮产物合并。
- 本轮仅写本报告与 runs 目录（logs 内 hash/JSON/exit/磁盘记录）；无源码/依赖变更；无 commit/push/worktree。

## 索引

- raw：`.agents/runs/windows-current-release-build-cc-20260930/logs/`（target-dir.txt、build.sh、compiler-artifacts.jsonl、build.log、build.exit、exe-hashes.txt、disk-before/after.txt）
- bins：保留于上述 /private/tmp 独立 target（独立，不并入既有产物）

停止。
