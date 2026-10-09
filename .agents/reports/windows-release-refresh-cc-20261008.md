# Windows 当前冻结产品 release 交叉编译刷新 — CC（compile-only）

日期：2026-10-08。执行者：CC（Claude）。仅编译门禁（FOR WINDOWS，本机 macOS 交叉编译）；无 GUI/Host/网络/真实捕获/输入/SSH/部署/远端触碰；产物未执行。旧 2026-09-30 产物 `/private/tmp/windows-current-release-build-cc-20260930.xKbMwg` 已缺失，本轮为可复现的全新构建。

## 结论

**cargo build（Windows MSVC release --bins）exit 0，两个产品 bin 编译成功并通过 PE 架构静态核对。**编译成功≠GUI 或 remote 连接验收；本轮不发布、不 staging 到 Windows。

## 构建环境（复用已验证命令，未改动任何源码/依赖）

- 环境脚本：`.agents/runs/windows-release-refresh-cc-20261008/logs/build-env.sh`（逐条复用 `desktop-feedback-host-windows-cc-20260930/logs/build-env.sh` 的 cargo-xwin env / DYLD_LIBRARY_PATH / AR rust-lld / ARFLAGS 四行，仅 CARGO_TARGET_DIR 换为本轮新路径 + `CARGO_INCREMENTAL=0`）。
- CARGO_TARGET_DIR：`/private/tmp/windows-release-refresh-cc-20261008.fRruU4/target`（本轮 NEW mktemp 独占目录，`logs/target-dir.txt`）。
- 工具链（`logs/env-versions.txt`）：rustc 1.98.1 (48a229cea 2026-09-01, LLVM 22.1.8)、cargo 1.98.1、cargo-xwin 0.23.1、toolchain stable-aarch64-apple-darwin (default)。
- 命令（`logs/build.sh`）：`cargo build --target x86_64-pc-windows-msvc --release --bins --locked --offline --message-format=json`
- **数字退出码：0**（`logs/build.exit`）。原始 compiler JSON：`logs/compiler-artifacts.jsonl`（143 条，本轮独占 target 全新编译、`fresh: false`；旧 2026-09-30 轮为 120 条）；stderr：`logs/build.log`（97 行，**0 error / 0 warning**）。编译耗时约 52s。磁盘：`logs/disk-before.txt` / `disk-after.txt`（Data 卷余约 50.2GB，消耗约 0.35GB，未触 3GB 限制）。

## 源冻结核对

`.agents/runs/windows-root-race-repair-sol-20260930/source-freeze.sha256`（177 条）：构建前 **177/177 OK**、构建后 **177/177 OK**，mismatch=0，**无源码漂移**。

## 精确产物（来自 compiler JSON `target.kind=["bin"]` / `profile.opt_level="3", debug_assertions=false` / `executable` 字段，非 glob）

| bin（Cargo target 名） | 原始精确路径 | 大小 | SHA256 | PE 架构 |
|---|---|---|---|---|
| `computer-host` | `/private/tmp/windows-release-refresh-cc-20261008.fRruU4/target/x86_64-pc-windows-msvc/release/computer-host.exe` | 5,673,472 B | `1aeedcd930528abc29358555a6d7c7abb7b0f8ee8bbf9f6e9d9b39abe8164b9c` | PE32+ console x86-64（`file` + 直读 MZ/PE 头 machine=0x8664） |
| `computer-client` | `/private/tmp/windows-release-refresh-cc-20261008.fRruU4/target/x86_64-pc-windows-msvc/release/computer-client.exe` | 1,730,048 B | `2f1f8335160ecd5ee7d3d86d0f88f75be8111c05bb4df600fa6b0ae96d622dc7` | PE32+ console x86-64（同上） |

## 持久化产物（durable copy，新文件，未覆盖任何既有产物）

| bin | 持久路径（`.agents/runs/windows-release-refresh-cc-20261008/artifacts/`） | SHA256（复制后复检，与原始一致） |
|---|---|---|
| `computer-host` | `computer-host.exe` | `1aeedcd930528abc29358555a6d7c7abb7b0f8ee8bbf9f6e9d9b39abe8164b9c` |
| `computer-client` | `computer-client.exe` | `2f1f8335160ecd5ee7d3d86d0f88f75be8111c05bb4df600fa6b0ae96d622dc7` |

## 与 2026-09-30 旧产物对比

- 旧报告（`windows-current-release-build-cc-20260930.md`）：host `206dbf99…d593` / client `5a30f6d8…2141`。本轮哈希与其**不同**、大小**逐字节相同**（5,673,472 / 1,730,048 B）。本轮基于同一 source-freeze（177/177），哈希差异源于非源码因素（依赖解析/工具链环境随时间变化的正常表现）；以本轮持久化产物为当前冻结源的实际编译凭证。

## 边界 / Pending

- 未运行任何 cargo test / 本地 exe / 远端执行；未 SSH / staging / 打包发布；未触碰 acer-win 上的任何状态。
- **不构成**当前版本 Windows GUI、remote 连接、物理多屏、feedback/Stop 正确性证据；后续真机验收仍由 acer-win 上的 GUI gate 负责。
- 本轮仅写 `.agents/runs/windows-release-refresh-cc-20261008/`（logs + artifacts）与本报告；无源码/测试/构建辅助代码变更；无 commit/push/worktree；模型路由未改动。

## 索引

- raw：`.agents/runs/windows-release-refresh-cc-20261008/logs/`（build-env.sh、build.sh、target-dir.txt、env-versions.txt、compiler-artifacts.jsonl、build.log、build.exit、exe-hashes.txt、disk-before.txt、disk-after.txt）
- bins（durable）：`.agents/runs/windows-release-refresh-cc-20261008/artifacts/`（computer-host.exe、computer-client.exe，新独占文件）
- bins（原始）：保留于上述 /private/tmp 独占 target

停止。

## 协调者证据核对与措辞收窄

已读取原始 build.exit=0、compiler JSON build-finished.success=true，并重算持久化两PE哈希、长度及machine=0x8664。产物仅完成编译，尚未在Windows执行。177项源码清单匹配不证明全部构建输入可复现；两个版本的文件大小相同、SHA不同，不能称“逐字节相同”。本轮没有定位哈希差异根因，不把依赖解析/工具链变化当成已证实原因；后续按本轮确切SHA测试。
