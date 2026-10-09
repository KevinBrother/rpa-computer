# 原子多击协议边界断言修复（2026-09-30）

## 状态

**READY_FOR_CC_RETEST：仅测试断言修正，源码已停止变动，等待 CC+GLM 复测。**

本轮没有执行测试、验收、编译、GUI、CLI 或其它 agent；没有 commit/push/worktree。只写本测试文件与本报告，未修改产品代码、旧交付报告、冻结 manifest 或 CC 原始日志。

## 核查与根因

已读取 CC 的两轮真实独立测试证据：

- `.agents/runs/atomic-multiclick-cc-lib.log`：225 passed、1 failed、1 ignored；exit 101；69.18s。
- `.agents/runs/atomic-multiclick-cc-tests.log`：225 passed、1 failed、1 ignored；exit 101；73.50s。该命令在 lib 测试阶段失败，不能据此宣称后续 integration tests 已执行。

两轮均在 `src/runtime/session/tests/multiclick.rs:194` 的同一断言失败：actual=`input_error`，expected=`input_failed`。

`src/runtime/error.rs::backend_code` 第 56 行明确将 backend `input_failed` 映射为 `codes::INPUT_ERROR`；该常量值为 `input_error`。测试中的 `record.error` 已是 Runtime 协议层的 `ToolError`，原断言错误地使用 backend 层错误码。产品映射是既有正确边界，本轮不修改它。

## 唯一测试变更

文件：`src/runtime/session/tests/multiclick.rs`，函数：
`uncertain_press_or_release_cleanup_keeps_pair_metadata_and_skips_later_pairs`。

```diff
-            assert_eq!(record.error.as_ref().unwrap().code, "input_failed");
+            assert_eq!(record.error.as_ref().unwrap().code, codes::INPUT_ERROR);
```

保留所有其他逻辑及断言，包括：

- Press/Release 两类故障与 count=1..=3 循环；
- `InputOutcome::Partial`；
- `CleanupOutcome::Released`；
- 完整按钮事件 trace、同 count 的重复 release、跳过后续未按下 pair；
- cleanup 调用次数为 1；
- held snapshot 为空。

RecordingInput 继续返回 backend `input_failed`，确保测试仍覆盖真实的协议边界映射；不将 mock 错误码也改成协议码，不放宽为任意错误/多种错误码之一。

## 同类问题检查

只读检查了新增 multiclick Runtime 模块、plan 回归、Host dispatch 测试与 macOS mouse 测试的错误码断言。在本轮新增 Runtime multiclick 模块中，只有上述一条协议层错误码断言使用了错误的 `input_failed` 期望，未发现第二处同类断言。

Host/native 层测试中的 `input_failed` 断言直接检查 backend/native 返回值，没有经过 `backend_code`，不能随此处一并改成 `input_error`；这些断言与测试 mock 均保持原样。本结论来自源码检查，不代表测试通过。

## 文件 hash 与冻结记录

测试文件 SHA-256：

- 修改前：`e399be55f0985870d1392392a4483e28fc9b1282d337e12269ea5d77640b83a1`
- 修改后：`c4d0b98bff81b7e5cec0475952c26af1a93537946091591427dcfa923685ea74`
- 旧冻结 manifest 中记录：`e399be55f0985870d1392392a4483e28fc9b1282d337e12269ea5d77640b83a1`
- 修改前与旧冻结记录一致：`true`

旧 `.agents/runs/atomic-multiclick-sol/handoff-source-hashes.json` **原样保留，没有覆盖**。本报告记录本轮新的测试文件 hash；以旧 manifest 加本报告作为修复链路。原报告的编译/开发自检均发生在此修复之前，不作为本轮测试通过证据。

本轮写集仅：

1. `src/runtime/session/tests/multiclick.rs`
2. `.agents/reports/atomic-multiclick-sol-repair-20260930.md`

## 待 CC+GLM 复测命令（本角色未执行）

工作目录：`/Volumes/doc/workspace/datagrand/rpa/rpa-computer`。

先复测精确失败项，确认越过错误码断言后，后续完整 trace/cleanup/count 断言仍成立：

```sh
cargo test --lib runtime::session::tests::multiclick::uncertain_press_or_release_cleanup_keeps_pair_metadata_and_skips_later_pairs -- --exact
```

再复测该集成测试模块与完整 root 测试：

```sh
cargo test --lib runtime::session::tests::multiclick
cargo test --lib
cargo test --tests
```

不要增加 `--ignored` / `--include-ignored`。保留每条命令的原始输出和真实退出码，避免以未启用 pipefail 的 tee/tail 管道掩盖失败。测试执行与独立验收仍由协调者调度真实 CC+GLM；如有后续失败，交回实现者修复。

**本轮修复未验证为 green；只有 CC 复测结果能够给出通过结论。**
