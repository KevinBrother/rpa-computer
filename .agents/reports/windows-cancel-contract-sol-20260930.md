# Windows cancel contract sidecar — SOURCE_FROZEN / COMPILE_ONLY

日期：2026-09-30。作者：Codex sol。仅指定 3 个新 source 文件 + 本报告；没有产品/root/focus/已有 tests/Cargo 修改，没有 worktree/commit/push/reset/测试运行/GUI/SSH/委派。

状态：**9 项真实生产 Worker 中途取消契约测试源码冻结；Windows target compile/link-only exit0；CC+GLM 运行 pending。cancel07 网络 disconnect 与真实 GUI 全部 deferred，不能据此称 allcancel 完成。**

## 实现与静态审阅

已读取任务 `.agents/tasks/windows-cancel-contract-sol-20260930.md`、用户提供的 AGENTS 约束、`.agents/CONTRACT.md`、acceptance cancel01..10、实际 Backend/Worker/CancelHandle/ingress/McpService/Runtime API，以及现有 runtime_contract/transport_lifecycle/contract 测试约定。读取 using-superpowers、brainstorming、writing-plans、TDD、verification skills；用户明确已批准限定任务且禁止额外写计划、产品修改和测试运行，按此优先约束执行，不人工制造 RED，不启动测试代理。

实现清单、9 个精确测试名、每 case oracle、CC Windows 指令全部在 `docs/windows-cancel-contract-matrix.md`。Backend 在 actual dispatch 记录输入/held 后 blocked，监督者通过真实生产控制 stop，再由 RAII release 放行；没有 mock 自设 cancel/直接清 held 模拟取消。新 integration target 独立，不阻塞 root 自有 testtargets 冻结。所有测试非 ignored、Windows cfg。

静态审阅纠正：MCP text JSON 从 type=text 内容块查找（不假定第0块）；采用固定两 move drag 验证实际位移；failure backend input_failed 精确映射 input_error，而成功取消是 cancelled；明确 handle_eof 只产生 Stop，后续 Worker shutdown 必须由 transport owner 做，本测试显式调用这个生产 continuation。加入旧 cancelled action ledger 重放不再注入、fresh action dedup、wrong typed wire ID/late ID generation negative。没有执行这些断言，不宣称 GREEN。

## 自有 source 冻结（不包含 root/focus）

```text
70db6724efaa4c55ac70e27b829522810cdf1dca03ed273a3d2be900a19dcbc0  tests/windows_cancel_contract.rs
411efdce9e8f642e90cded9ded5468bffd57f29a6da1398eea5d533759e81a8c  tests/windows_cancel_contract/support.rs
48e8b94765c5075abeca7f277b84b96fc6eca71f87b8ac4a12440a4ccf0956e1  docs/windows-cancel-contract-matrix.md
```

静态 test 函数计数：9，无 ignored；support 仅辅助模块。报告自身 SHA256 由最终交付消息另列，避免自引用。

## public API 基线（只读 pin，不是本人冻结/接管 root）

基于 root StageB `.agents/reports/windows-multidisplay-integration-sol-20260930.md` SOURCE_FROZEN 的实际工作区接口：BackendFactory::Test、Backend trait 默认 primary/display_snapshot(None)、Worker::start/call_with_deadline/cancel_handle/request_generation/transport_gate/shutdown、CancelHandle::cancel/is_cancelled、TransportGate::cancel_request、McpService::new/handle_line/handle_eof、Reply。Worker enqueue 是 pub(crate)，integration 没有调用私有 enqueue；pause/close 在独立 Job 调用真实 public call_with_deadline。

root source-freeze.sha256 已只读核验：
- Manifest SHA256：`946e8ef40adc542e613c6f099458183db0cb2558508e87e962466b4a536582d9`。
- 对其全部条目核验不匹配：`[]`。

以下为 final compile 前 public API/执行依赖只读指纹（不列入自有 source freeze）：

```text
4348216e627eb244e1b23325e03cf9424a82d032499422be2f72362080abba49  Cargo.toml
c8910dbd0a156b6d3cbcbe83392d725cdbcdee20d016ec471e7bb7392b36df35  Cargo.lock
32cb49f8273ef5615c823be69a9be4b0b9fc6376c37ba5bded892e05937a738f  src/backend/mod.rs
9b688bdbf87c1fbc5611e1d81d0fdd1017c55714e3b627114e825e247a7a479c  src/mcp/backend_factory.rs
df8aafd064a8cbacdc3b5abca27e19bd8fe980031d65891f5639675edb8b8786  src/mcp/worker.rs
4e71bdf14abff00e0419fac557c38e64d9f502fe534991280c0c472290f5e986  src/mcp/worker/tracker.rs
37302212085f69f6bf1fa2f93d7c5de6ad05304e231af5d5f3285201dfad731a  src/mcp/worker/native.rs
245195b0cf62a6307d500f6af5433027df56b40d8a06cedf4b4803abbd3d96af  src/mcp/server.rs
8f41dd43df941bc3887e8e2edab8e7f649d5e36ca761fc68199abbcc9281cadf  src/mcp/ingress.rs
8ec84a7d05e81015c2240e97d8271d943007b3eb0208b892dd529a27e114951e  src/mcp/jsonrpc.rs
741565945764d7693b569b97d83da9325d3d15c7bc4bc192954863cc6b6740e1  src/runtime/mod.rs
d82ea4f3093f4a76785c40b462c4a15f0dad2ce06c6689dd3a7fc4bcd6b1182d  src/runtime/runtime.rs
85c69de87a2ed6c2943ed618e803dcfed78c28b4e2cb33d2fabf978d1476c88b  src/runtime/session.rs
ee0e713823c8acb0a75dfe8f470ca9a612ee0027b75d90363de48d34dcf2693e  src/runtime/session/observation.rs
49cb3076220f50971b73504cec2248c1a0b8abf2c8383ddd208c317d95bc2fd7  src/runtime/actions.rs
f4e878eca49b4b376af8f308cc8a8eb1fd82d1a924ad87437ae267da3f3d3ffc  src/runtime/plan.rs
4f0e59ccf45a29969a6612bc901a7ca026225aad91049435a157a3cc9ded8a10  src/runtime/execute.rs
dc01ac0e76dfa3fd9cc0dd35e4e3929df0c5fdb1aca2494999036b1af2412cb7  src/runtime/error.rs
69e36a58e90975ab986a51339d469d8a3003c6e7dbaa0c5f26590a567216d8ec  src/runtime/display.rs
133def87fa1085d18f73e16855cf24b5525cb795787c35c1c9cb9ea232041dab  src/runtime/display_input.rs
a82cfd908f530c63063a6ab157d9e9414dff64ff563dd8d14b0802b739a1b169  src/feedback/config.rs
```

Final compile 前/后监测 Cargo、src 全部 Rust、crates Rust/Cargo 共 144 文件，变化列表：`[]`。
首次 compile 没有前/后完整 root 快照，所以不追溯声称首轮期间零变化；首轮实际 exit0。最终 compile 有上列前/后监测，API/root 若之后变化需 CC 重新核对并重编译；不能把本次证据适用于未知后续版本。

## compile-only 原始证据（未运行产物）

- 起始本地时间：`2026-09-30T21:37:59.833958+08:00`；elapsed `4.066s`。
- 命令：`cargo test --locked --offline --target x86_64-pc-windows-msvc --test windows_cancel_contract --no-run --message-format=json`。
- 环境：source 既有 `.agents/runs/desktop-feedback-host-windows-cc-20260930/logs/build-env.sh` 后立即覆盖 `CARGO_TARGET_DIR=$PWD/.agents/runs/windows-cancel-contract-sol-20260930/target`；不覆盖 root/旧 artifacts。
- 本次 native cargo 数字 exit：`{p.returncode}`。首次 no-run 全新 target exit0，22.65s（依赖+Windows链接）；本次是最终源码。

原始 final stderr：

```text
   Compiling rpa-computer v0.1.0 (/Volumes/doc/workspace/datagrand/rpa/rpa-computer)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 3.99s
```

本次 stdout compiler-artifact 目标记录（完整对象，不以旧 glob 推断）：

```json
[
  {
    "reason": "compiler-artifact",
    "package_id": "path+file:///Volumes/doc/workspace/datagrand/rpa/rpa-computer#0.1.0",
    "manifest_path": "/Volumes/doc/workspace/datagrand/rpa/rpa-computer/Cargo.toml",
    "target": {
      "kind": [
        "test"
      ],
      "crate_types": [
        "bin"
      ],
      "name": "windows_cancel_contract",
      "src_path": "/Volumes/doc/workspace/datagrand/rpa/rpa-computer/tests/windows_cancel_contract.rs",
      "edition": "2021",
      "doc": false,
      "doctest": false,
      "test": true
    },
    "profile": {
      "opt_level": "0",
      "debuginfo": 2,
      "debug_assertions": true,
      "overflow_checks": true,
      "test": true
    },
    "features": [],
    "filenames": [
      "/Volumes/doc/workspace/datagrand/rpa/rpa-computer/.agents/runs/windows-cancel-contract-sol-20260930/target/x86_64-pc-windows-msvc/debug/deps/windows_cancel_contract-7e9ced150c16f50d.exe",
      "/Volumes/doc/workspace/datagrand/rpa/rpa-computer/.agents/runs/windows-cancel-contract-sol-20260930/target/x86_64-pc-windows-msvc/debug/deps/windows_cancel_contract-7e9ced150c16f50d.pdb"
    ],
    "executable": "/Volumes/doc/workspace/datagrand/rpa/rpa-computer/.agents/runs/windows-cancel-contract-sol-20260930/target/x86_64-pc-windows-msvc/debug/deps/windows_cancel_contract-7e9ced150c16f50d.exe",
    "fresh": false
  }
]
```

本次 stdout build-finished 记录：

```json
[
  {
    "reason": "build-finished",
    "success": true
  }
]
```

- Windows test exe：`/Volumes/doc/workspace/datagrand/rpa/rpa-computer/.agents/runs/windows-cancel-contract-sol-20260930/target/x86_64-pc-windows-msvc/debug/deps/windows_cancel_contract-7e9ced150c16f50d.exe`
- SHA256：`7dbbbedabaa4098d67348ff1681d73a9772a2bb193eabbe0de32aacafc6ce830`
- 大小：`11896320` bytes。

该命令只构建限定 integration target，未加 --lib/--tests/ignored，未运行 exe，未 --list。Windows cfg 的 body 已 Windows 交叉编译和链接，不是 Mac empty cfg 检查。也没有编译或运行 focus fixture。与 root 仅 cargo check 的证据不同，本证据可以证明本 target 的 Windows 交叉链接，但仍不能证明 Windows tests/native/GUI 通过。

## CC handoff / STOP

1. root SOURCE_FROZEN 已存在；统一由协调者安排 CC+GLM。再次核对自有3条 source hash、root freeze/API pin；有变化报告协调者，不修产品或削弱断言。
2. 按矩阵文档用 NEW unique target `--test windows_cancel_contract --no-run --message-format=json` 提取此次唯一 exe 与hash。作者 build artifact 可校验复用，但不混同后续 root 更新后的 build。
3. Windows Session0 纯测试，external watchdog 120s；只经协调者批准 runner 执行 --list 和 --test-threads=1 --nocapture，立即保留非 null numeric native exit、stdout/stderr、PID/path/creation/session 身份。预期9项是源码计数，不是已获 GREEN。首红 STOP，反馈精确 assertion/生产语义缺口给 root owner。
4. 网络断连 cancel07、真实 stdio pipe EOF端到端、全部 GUI held/release/focus 恢复仍 deferred。无 sockets/Host/native injection/外部进程/全局hooks；清理失败仅 synthetic backend，不能用于真实桌面故意遗留键。
5. 不修改 focus 或 root 文件，不要求其等待 sidecar 或把此 target 纳入已完成的 root 编译；停止自有 source 写入。没有自行运行或委派任何测试。

## 最后静态核对

限定两个自有 Rust 文件 `rustfmt --check --edition 2021 --config skip_children=true` exit0；Python 只读核对自有 source 全部 SHA 与报告匹配、9个唯一 test 函数与矩阵逐字对应、无 ignored/末尾空白。这是静态检查，未运行测试或产物。最终 source 未在最终 compile 后改动。
