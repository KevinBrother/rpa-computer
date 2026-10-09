# 原子多击修复后 CC 独立复测（2026-09-30）

## 结论

**GREEN：4 条命令全部通过，退出码均为 0。** 未修改任何源码/测试/脚本；仅写本报告与 4 个日志文件。

## 前置核查

- 测试文件 `src/runtime/session/tests/multiclick.rs` 当前 SHA-256：`c4d0b98bff81b7e5cec0475952c26af1a93537946091591427dcfa923685ea74`，与修复报告"修改后"hash 一致。
- `src/runtime/error.rs:56`：backend `"input_failed" => codes::INPUT_ERROR`；`codes::INPUT_ERROR = "input_error"`（error.rs:69）。映射与修复报告描述一致，产品代码未变。
- 修复报告中声称的唯一变更是 194 行断言由 `"input_failed"` 改为 `codes::INPUT_ERROR`；本轮未做逐行 diff 审查（首轮审查已覆盖），但 hash 匹配 + 全量测试通过 + 其它断言（trace、cleanup_calls、held_snapshot 等，位于 194 行之后）在本轮真实执行并通过，与修复声明一致。

## 测试结果（按执行顺序）

| # | 命令 | 结果 | 退出码 | 日志 |
|---|------|------|--------|------|
| 1 | `cargo test --lib runtime::session::tests::multiclick::uncertain_press_or_release_cleanup_keeps_pair_metadata_and_skips_later_pairs -- --exact` | 1 passed; 0 failed | 0 | `.agents/runs/atomic-multiclick-cc-retest-1-targeted.log` |
| 2 | `cargo test --lib runtime::session::tests::multiclick` | 4 passed; 0 failed | 0 | `.agents/runs/atomic-multiclick-cc-retest-2-multiclick.log` |
| 3 | `cargo test --lib` | 226 passed; 0 failed; 1 ignored | 0 | `.agents/runs/atomic-multiclick-cc-retest-3-lib.log`（69.93s） |
| 4 | `cargo test --tests` | 见下 | 0 | `.agents/runs/atomic-multiclick-cc-retest-4-tests.log` |

未加 `--ignored` / `--include-ignored`。

## 命令 4 实际执行的 target（按运行顺序）

1. lib 单元测试：**226 passed; 0 failed; 1 ignored**（74.23s）— 与命令 3 同一 target
2. `unittests src/bin/computer-client.rs`：0 tests
3. `unittests src/bin/computer-host.rs`：0 tests
4. `tests/protocol.rs`：**12 passed; 0 failed**（5.00s）
5. `tests/remote_transport.rs`：**17 passed; 0 failed**（153.26s；多个 remote_raw_tls 测试各超 60 秒为等待/超时类用例，均通过）
6. `tests/runtime_contract.rs`：**35 passed; 0 failed; 1 ignored**（`registry::request_registry_is_bounded_at_1000_without_evicting_retriable_records`，标记 slow 需显式 `--ignored`）（10.51s）
7. `tests/transport_lifecycle.rs`：**3 passed; 0 failed**（0.34s）

合计：293 passed、0 failed、2 ignored。所有 target 均实际运行完毕，无未跑到者。

ignored 共 2 项均为既有标记（live capture 诊断、slow 1000-step 注册表），非本次新增省略。

## 与首轮失败的对照

首轮失败断言（multiclick.rs:194，`input_failed` vs `input_error`）本轮在完整运行中真实通过——包括位于其后的 calls/cleanup_calls/held_snapshot 断言及外层 3×2 循环全部迭代。不沿用首轮报告中"后续 trace 全部执行"的错误推断；本轮结论全部来自本次运行的完整日志与退出码。

## 范围声明

- 本轮为编译+纯测试，非 GUI 验收。
- 其它 agent 独立目录的 feedback/fixture 工作未触碰；本轮未运行任何其它 crate 的测试。
- 未 commit/push。
