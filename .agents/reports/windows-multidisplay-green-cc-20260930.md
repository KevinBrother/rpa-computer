# Windows root multidisplay StageB — CC 独立验证（绿测尝试）

日期：2026-09-30。执行者：CC（Claude），Windows-only 交叉验证。依据 SOURCE_FROZEN 报告 `.agents/reports/windows-multidisplay-integration-sol-20260930.md`。

## 结论（非全绿）

**29 项新/原 multidisplay 纯 seam 测试中 26 pass / 3 fail / 0 ignored。**
`feedback::display_tests::` 3/3 全部 FAIL（native exit 101）。按协议：**首轮失败即保留，未运行全量默认 suite，未重试掩盖**。需要代码 owner 修复 `src/feedback/display_tests.rs` 相关实现/断言后另行复测。

## 源码冻结核对

- `source-freeze.sha256`（34 条）：构建前、构建后、测试后各核对一次，均 **34/34 OK**（`shasum -a 256 -c`，mismatch=0）。
- Source manifest SHA256 `946e8ef4...6582d9`、原6RED文件 SHA256 `79c30152...c26cb3e1` 见 SOL 报告，均包含于上述 34 条并通过核对。

## 构建（交叉链接）

- 环境：`source .agents/runs/desktop-feedback-host-windows-cc-20260930/logs/build-env.sh`，随后立刻 `export CARGO_TARGET_DIR=$PWD/.agents/runs/windows-multidisplay-green-cc-20260930/target`（新目录，构建前不存在）。
- 命令：`cargo test --target x86_64-pc-windows-msvc --locked --offline --no-run --lib --message-format=json`
- **数字退出码：0**（`logs/build.exit`）。原始 JSON：`logs/compiler-artifacts.jsonl`；stderr：`logs/build.log`。
- 从 JSON 提取（非 glob）唯一 test artifact：`profile.test=true`、`target.name=rpa_computer`、`target.kind=["lib"]`、`fresh=false`。
- **精确路径**：`.agents/runs/windows-multidisplay-green-cc-20260930/target/x86_64-pc-windows-msvc/debug/deps/rpa_computer-ec32ad14260fc3bf.exe`（15,398,912 字节）。
- **本地 SHA256**：`0ba99c30e78f8ffe7754fe2e0c4e5a9e26b36613cc9e7261b244e3ecff940a60`

## Windows 侧部署与哈希核对

- 保留目录（NEW，本次创建）：`C:\Users\Administrator\AppData\Local\Temp\windows-multidisplay-green-cc-20260930-213205-895953247\`（子目录 `exe\`、`scripts\`、`config\`、`evidence\`）。
- 远端 exe SHA256（`Get-FileHash`）：`0BA99C30E78F8FFE7754FE2E0C4E5A9E26B36613CC9E7261B244E3ECFF940A60` — **与本地 byte-identical**。
- runner：冻结 `scripts/windows-test-runner.ps1` + `Runner.cs` 原样上传，未修改。每次 summary 中 `runner_source_sha256=ad2a607c...9572c4`、`runner_module_sha256=aced18e6...972214`。
- 每个单独 filter 使用**独立 NEW evidence 目录**（`evidence\filter1..6`），runner 原子预留成功。
- staging 期间失败尝试全部保留（`filters-run.output`/`run2`/`run3`）：① ssh 引号损坏 `-File` 路径；② `Runner.cs` 需位于 `scripts\windows-test-runner\` 子目录（runner Add-Type 路径约定）；③ evidence 父目录未预建（Win32=3）。均为 invocation/staging 修复，未改 runner/测试源码。

## 29 项精确结果（runner summary + lib 输出，逐 filter 顺序执行）

总 exe 内 lib 测试总数 279（=26 pass + 3 fail + 250 filtered-out 其余项，各 filter filtered 计数与该数自洽）。

| # | filter | 结果 | native exit | 明细 |
|---|---|---|---|---|
| 1 | `runtime::runtime::tests::multidisplay_contract_red::` | **6 pass / 0 fail** | 0 | 原6RED 全部转 GREEN；0.46s |
| 2 | `backend::display::tests::` | **6 pass / 0 fail** | 0 | compose/负原点/竖屏/scale/gap/预算/无fallback；0.04s |
| 3 | `runtime::runtime::tests::multidisplay_integration::` | **11 pass / 0 fail** | 0 | 全 11 项 GREEN；1.61s |
| 4 | `feedback::display_tests::` | **0 pass / 3 FAIL** | **101** | 见下；0.00s |
| 5 | `mcp::bounded_output::tests::` | **2 pass / 0 fail** | 0 | 转义字节限额/超限保留事实；0.00s |
| 6 | `runtime::execute::tests::topology_guard_cannot_dispatch_after_input_deadline` | **1 pass / 0 fail** | 0 | deadline 诚实 not_started；0.80s |

每 filter：`--test-threads=1 --nocapture`，timeout 120s（schema 上限 3600s，支持），均未超时（`timed_out=false`），stdout/stderr drain 完整无截断，`root_session_identity_source=Process.SessionId`，`root_executable_identity_source=exact_absolute_ProcessStartInfo_FileName`。

### 失败明细（原样保留，未改任何源码/断言）

- `desktop_surface_has_union_bounds_and_internal_gap_move_has_no_ring_target` — panic `src/feedback/display_tests.rs:46:28`: `Result::unwrap()` on `Err(Protocol(InvalidField))`
- `full_desktop_stop_still_permanently_prevents_open_or_resume` — panic `src/feedback/display_tests.rs:77:5`: `{"error":{"code":"cancelled","message":"desktop feedback refused the new session authority"}}`
- `wrapper_forwards_complete_facts_and_query_capture_errors` — panic `src/feedback/display_tests.rs:31:28`: `Result::unwrap()` on `Err(Protocol(InvalidField))`

三项均为 `Protocol(InvalidField)` 系（一项表现为 feedback open 被以 cancelled/authority 拒绝），指向 feedback adapter 与 wire 协议字段校验之间的真实 seam 不一致，**非 CLI/环境/runner 失败**。

## 全量 suite：未运行

依据协议"首个测试失败后不得 full suite / retry 掩盖"，filter4 失败后（其余新 filter 结果已安全收集完毕）即停止。基线 244/0/6 ignored、预计 267 pass 等数字本轮**均不适用**，留待修复后复测。

## 已知 ignored（未触碰）

现有 6 项 ignored（真实截屏 / fake process）按要求保持 ignored，未加 `--ignored` / `--include-ignored`。

## 副作用与残留检查

- 6 个 filter 均为纯 seam 测试（FakeBackend/MemoryFrames），无真实桌面/输入/网络/进程副作用；本轮未观察到意外行为。
- 残留检查仅限自有 TEMP 目录列举：目录内仅本次上传的 exe/scripts/config/evidence；**无存活 rpa_computer 进程**（按路径匹配，无全局/name kill）。runner 报告 `cleanup_state=root_exited`。
- 所有失败尝试的 evidence/config 原样保留在 runs 目录，未删除。
- 本轮无源码写入（仅本报告、runs 目录与 invocation helper）；未 commit/push/worktree；无 release 构建（本轮不要求）。

## 明确未验证 / Pending

- GDI 真实多屏枚举、物理单位、混合 DPI/负坐标/竖屏/gap 实机捕获、native 句柄释放、GUI feedback、热插拔/旋转中途 drag——**本轮 mock/seam GREEN 不构成以上证据，全部 pending**，需协调授权后另排真实 GUI 验收。
- 全量默认 suite 回归：待 feedback 失败修复后执行。
- feedback 永久 Stop 禁止 reopen/resume 断言本轮未通过，其语义有效性待修复后复核。

## 产物索引

- 编译/哈希/日志：`.agents/runs/windows-multidisplay-green-cc-20260930/logs/`
- 各 filter 原始 evidence（Windows 下载副本）+ config：`.agents/runs/windows-multidisplay-green-cc-20260930/evidence/`
- invocation helpers 与失败尝试：`.agents/runs/windows-multidisplay-green-cc-20260930/`（`filters.ps1`、`filters-run*.output`、`*.stderr`、`tempdir.txt`、`layout-fix.ssh.stderr`）

交接后本轮停止；等待协调者指派 feedback 修复 owner。
