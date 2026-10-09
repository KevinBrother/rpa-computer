# Windows repaired-token 独立回归 — CC 复验

> ## ⚠️ CORRECTION（2026-09-30 追加，见文末完整 Correction 段）
>
> 原报告以下三处表述有误/误导，以本纠正为准；原正文保留作历史记录，不作废：
>
> 1. **"Phase A、B 全 GREEN（38/38）"错误**。实际执行次数为 Phase A 31 次 + Phase B 27 次 = **58 次执行**；且 Phase A 的 31 项与 Phase C 全 suite、Phase B 内部存在同测试重复覆盖，**58 不是 unique test 数**。
> 2. **"两项 Phase C 失败与 token 修复直接相关"无因果证据**。它们只是在更广完整回归（全 281 项 suite）中首次暴露；目前 root owner 正在定位，**不应先认定是产品 bug 或测试 bug**。早先 3 项 feedback 失败已转绿则有直接执行证据（Phase A filter4，3 pass / native 0）。
> 3. **Phase D 未运行，且本轮 staging 仅上传/核对了 8 个 exe，遗漏 `windows_cancel_contract-7e9ced150c16f50d.exe`**（`logs/remote-exe-hashes.txt` 仅 8 条）。此前"旧 cancel 9/9 GREEN"来自 pre-token 旧产物（`7dbb...`），不能替代 repaired root 上的重验；后续回归必须补第 9 个 exe 的上传、双端哈希核对与执行。

日期：2026-09-30。执行者：CC（Claude），Windows-only，Session 0，纯 contract 回归。依据 SOURCE_FROZEN 报告 `.agents/reports/windows-multidisplay-token-repair-sol-20260930.md`。

## 结论

**Phase A、B 全 GREEN：31 + 27 = 58 次执行（含重复覆盖，非 unique 数）；Phase C 全 lib suite 273 pass / 2 FAIL / 6 ignored（native 101）；Phase D 按首败停机规则未运行。**

首轮全 suite 中两个失败均已原样保留，未重试、未改断言/源码：

1. **首个 RED**：`feedback::tests_runtime::stop_racing_resume_geometry_cannot_restore_authority_or_clear_cancel`
   — panic `src/feedback/tests_runtime.rs:222:56`: `called Option::unwrap() on a None value`
2. `runtime::session::tests::cancellation_during_hold_stops_promptly`
   — panic `src/runtime/session/tests.rs:416:5`: `assertion left == right failed, left: NotStarted, right: Partial`

两项均系在更广完整回归中**首次暴露**，目前 root owner 正在定位，因果归属未定（详见文末 Correction 2）；需修复/定位结论后另行复测。**修复前的 feedback 3 FAIL 已真实转 GREEN。**

## 源码冻结核对

- 新 `source-freeze.sha256`（52 条）：构建前、构建后各核对，均 **52/52 OK**（mismatch=0）。
- `topology-source.sha256`（14 条）：14/14 OK。
- 原6RED 文件 pin `79c30152...`、原3feedback 文件 pin `db4e8b8d...` 与 manifest 逐条一致（本轮未改动）。
- 本轮无 pin 变化；未发现其他 owner 并行改动本轮 52 文件。

## 构建（两次独立 cargo test --no-run，均 JSON）

新独占 target：`.agents/runs/windows-multidisplay-token-retest-cc-20260930/target`（创建前不存在）。env 同前轮 xwin 脚本后覆盖 `CARGO_TARGET_DIR`。

| 构建 | 命令要点 | 数字退出码 | 日志 |
|---|---|---|---|
| 1 topology crate | `cargo test --manifest-path crates/display-topology/Cargo.toml --target x86_64-pc-windows-msvc --locked --offline --no-run --message-format=json` | **0** | `logs/topology-artifacts.jsonl` / `topology-build.log` / `topology-build.exit` |
| 2 root lib+4 integration | `cargo test --target x86_64-pc-windows-msvc --locked --offline --no-run --lib --test protocol --test runtime_contract --test transport_lifecycle --test windows_cancel_contract --message-format=json` | **0** | `logs/root-artifacts.jsonl` / `root-build.log` / `root-build.exit` |

全部 9 个 test artifact 由 JSON `profile.test=true` + `target.name/kind` 精确提取（非 glob）：

| exe（JSON 提取） | SHA256（本地） | Windows 远端核对 |
|---|---|---|
| `rpa_computer-ec32ad14260fc3bf.exe`（lib） | `4cd0d12bf701c8c9bc9259bf4c136c28819ed62ff7f6ba20f195e3e02ae14969` | 一致（`rpa_computer.exe`） |
| `rpa_display_topology-c3ea4c766536e049.exe`（lib） | `e0f707239af6d1330f28529c8133d8357b865b005ea53fc06b075cc7ac4682bb` | 一致 |
| `capture-76f3887d89e17295.exe` | `5b81b0e1203572491bc0e3ba0068f550f04e222d6326f8ecc7fbcdb1fe70ada8` | 一致 |
| `mapping_drag-13f2fb91e69584c9.exe` | `9d8111c60acc8e3d59230958a57f878aa4a2b4bd9645c87bd63af3e8b62eb1da` | 一致 |
| `topology-2d98b6f81f25ff24.exe` | `2bb1a785c787e910648962920e3fe6f1bdd87f5284f3597a7484a424c21b153d` | 一致 |
| `protocol-04609477eb68437d.exe` | `daa1249cbaf8c7a3138f260b823645d840cabf120c71383e1ef8e83b2a05b374` | 一致 |
| `runtime_contract-f2f25769410ce146.exe` | `16c183df5d3429780a61e4837afadbe69b56bdca2c85cde1ad91dc6f268deece` | 一致 |
| `transport_lifecycle-ab3e23fc3a29ac12.exe` | `f944b37a57a3e5dadefcfd6bdf52d8de896e69187a1e96e3d4c6977820fac11a` | 一致 |
| `windows_cancel_contract-7e9ced150c16f50d.exe` | `d7f7eb886f14863fd5bce9a77b4892df362a419b7168dd9a98707bb2cdc35960` | **未核对（见 Correction 3：未上传/未执行）** |

root lib exe 文件名与上轮相同但**内容已变**（旧 `0ba99c30...` → 新 `4cd0d12b...`），确认不是旧产物复用。

## Windows 侧部署

- NEW 目录：`C:\Users\Administrator\AppData\Local\Temp\windows-multidisplay-token-retest-cc-20260930-a\`（`exe\`、`scripts\windows-test-runner\Runner.cs`、`config\`、`evidence\` 均先建父目录；evidence 叶目录由 runner 原子预留）。
- 冻结 runner 未修改；config 全部用 `ConvertTo-Json` 生成。
- 远端 8 exe SHA256 与本地逐一相符（`logs/remote-exe-hashes.txt`）。（**Correction 3：实际仅 8/9 个 test exe 被上传核对，缺 windows_cancel_contract，见文末。**）
- 本轮 staging 无失败尝试（复用前轮经验，避免了父目录/Runner.cs 路径/引号问题）。

## Phase A：原 29 + 新 2 root token tests（每 filter 120s，全部 GREEN）

| filter | 结果 | native exit |
|---|---|---|
| `runtime::runtime::tests::multidisplay_contract_red::` | 6 pass / 0 fail | 0 |
| `backend::display::tests::` | 6 pass / 0 fail | 0 |
| `runtime::runtime::tests::multidisplay_integration::` | 11 pass / 0 fail | 0 |
| `feedback::display_tests::` | **3 pass / 0 fail（旧 RED 转 GREEN）** | 0 |
| `mcp::bounded_output::tests::` | 2 pass / 0 fail | 0 |
| `runtime::execute::tests::topology_guard_...input_deadline` | 1 pass / 0 fail | 0 |
| `backend::display::metadata::token_tests::`（新） | 2 pass / 0 fail（`feedback_codec_still_rejects_legacy_debug_version`、`root_geometry_full_generation_survives_feedback_codec`） | 0 |

root lib 总数 = 281（filter 计数自洽：1+280 … 6+275）。

## Phase B：topology crate 4 exe 默认 suite（各 120s，全部 GREEN）

| exe | 结果 | native exit |
|---|---|---|
| `rpa_display_topology`（lib unit） | **6 pass / 0 fail**（含全部 5 个 `topology::token_tests::*`：stable / independent-trackers / revision-change / u64-boundaries / no-concatenation-collisions） | 0 |
| `capture` | 7 pass / 0 fail | 0 |
| `mapping_drag` | 10 pass / 0 fail | 0 |
| `topology` | 4 pass / 0 fail | 0 |

实际输出计数：**27 pass / 0 fail / 0 ignored**（SOL 预估"5 新 + 22 旧"与实测 6/7/10/4 分布一致，按输出如实记录）。

## Phase C：root lib 全默认 suite（timeout 480s，实际 305.12s）

- **结果：273 passed / 2 FAILED / 6 ignored，native exit 101，`outcome=native_failure`，`timed_out=false`，`cleanup_state=root_exited`，stdout/stderr drain 完整无截断，`executable_sha256=4cd0d12b...4969`，root_session_id=0。**
- 275 non-ignored 与任务简报预测一致（273+2），未强制凑数，按实际枚举记录。
- 失败明细见"结论"；两项失败无与 token 修复的因果证据，系更广回归中首次暴露（见文末 Correction 2）。

## Phase D：未运行

依据"任何 native 失败：保留首个 RED，停止广域回归"协议，Phase C 出现 2 FAIL 后 **4 个 pure integration exe（protocol / runtime_contract / transport_lifecycle / windows_cancel_contract）本轮未执行**。取消 9 项对修复后 root 的重验留待 Phase C 修复后进行。

## 已知 6 ignored（保持 ignored，未加 --ignored）

1× `backend::live_test::live_backend_capture_only`（真实截屏 manual）；5× `feedback::tests_process::*`（fake renderer 进程）。与既有基线一致。

## 副作用与残留

- A/B/新增测试均为内存/纯 seam；Phase C 全 suite 含既有纯回归，未观察到计划外桌面/网络/进程副作用；Notepad 进程未受影响。
- 残留检查（只读，仅按自有 TEMP 路径匹配）：**无存活本轮 exe 进程**；无全局/name kill。
- 本轮无源码写入（仅本报告、runs 目录、invocation helpers）；无 commit/push/worktree；无 release 构建。

## 明确 Pending

- 上述 2 项 Phase C FAIL 的 owner 修复与复测；修复后 Phase D 四 exe（含取消 9 项对 repaired root 的重验）。
- GDI/物理多屏、GUI release、feedback Stop 真实验证、capture exclusion、remote 17 —— 全部 pending，本轮纯 contract 证据不覆盖。

## 产物索引

- 构建 JSON/退出码/日志/exe 哈希：`.agents/runs/windows-multidisplay-token-retest-cc-20260930/logs/`
- 各 phase 原始输出：`.agents/runs/.../evidence/phase{A,B,C}.*`
- Windows 原始 evidence/configs 下载副本：`.agents/runs/.../evidence/remote-evidence*/`
- invocation 脚本：`.agents/runs/.../phase{A,B,C}.ps1`、`build.sh`

交接后停止；等待协调者指派 Phase C 两失败的修复 owner。

---

## Correction（2026-09-30 追加）

以下纠正针对原报告的表述错误；原始 raw 证据全部保留未动，原正文各段保留为历史记录。本轮 correction 未运行任何测试/编译/SSH/GUI，未修改产品代码/测试代码。

### 1. 执行次数：58 次执行，非 38/38；58 ≠ unique tests

依据原始 `evidence/phaseA.output` 与 `evidence/phaseB.output`：

- Phase A 7 个 filter 实际执行：6+6+11+3+2+1+2 = **31 次**；
- Phase B 4 个 exe 实际执行：6+7+10+4 = **27 次**；
- 合计 **58 次执行**。原报告标题"38/38"是笔误级错误。
- 其中重复覆盖明显：Phase A 的 31 项随后全部再次出现在 Phase C 的 281 项全 suite 中；Phase A 与 Phase C 是同一 `rpa_computer.exe`（`4cd0d12b...`）对同一批测试的重复执行。**58 不能计为 unique test 数**；unique 计数应以 Phase C 的 281 项枚举（273 pass + 2 fail + 6 ignored）和 Phase B 的 27 项为准，且两者亦无去重合并口径。

### 2. Phase C 两失败：无因果证据，不预判归属

原报告称两项失败"与 token 修复直接相关的 stop/resume/cancel 竞态域"——这一因果表述**没有证据支持**：

- 这两项（`feedback::tests_runtime::stop_racing_resume_geometry_cannot_restore_authority_or_clear_cancel`、`runtime::session::tests::cancellation_during_hold_stops_promptly`）只是**在更广的完整回归（全 281 项 suite）中首次暴露**；此前没有任何运行把它们与 token 修复关联或解耦的证据。
- 目前 root owner 正在定位；在定位结论出来前，**不应认定是产品 bug，也不应认定是测试 bug**。
- 相比之下，"早先 3 项 feedback 失败已转绿"有直接执行证据：Phase A `backend::display_tests` 后第 4 个 filter `feedback::display_tests::` = 3 passed / 0 failed / native exit 0。

### 3. Phase D 未运行；staging 漏第 9 个 exe；旧 cancel 9/9 不可替代

- Phase D 4 个 pure integration exe（含 `windows_cancel_contract`）**本轮均未执行**。
- staging 缺陷：本轮实际上传并完成双端哈希核对的仅 **8 个 exe**，`logs/remote-exe-hashes.txt` 仅 8 条，**`windows_cancel_contract-7e9ced150c16f50d.exe`（本地 SHA256 `d7f7eb886f14863fd5bce9a77b4892df362a419b7168dd9a98707bb2cdc35960`）未上传、未远端核对**。原报告表格将其列为"一致"是错误的——该行远端核对从未发生。
- 此前协调通知的"取消 9/9 GREEN"产生于 **pre-token 旧产物**，不能替代 repaired root（`4cd0d12b...`）上的重验。后续回归轮次必须补齐第 9 个 exe 的上传、双端 SHA256 核对与实际执行。

### 保留与边界

- 全部 raw（`evidence/phase{A,B,C}.output`、`remote-evidence*/`、构建 JSON/日志/哈希）原样保留。
- 本 correction 只修改本报告文件；未替 root owner 修改任何源码/测试/断言。
