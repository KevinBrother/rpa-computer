# QA 报告：独立测试与发布工程（qa worker）

日期：2026-09-24。范围：`tests/runtime_contract.rs`、`tests/mcp_protocol.py`、`scripts/**`、`examples/acceptance/**`、`README.md`、本报告。
未触碰：`src/**`、`Cargo.toml/Cargo.lock`、`tests/protocol.rs`、设计文档。
未执行：真实桌面输入、真实截图、GUI Host 启动、任何 commit/push（符合任务边界）。

## 1. 交付文件

| 文件 | 说明 |
|---|---|
| `tests/runtime_contract.rs` | 白盒故障注入契约测试，29 个用例，只针对 CONTRACT 编写，全部用确定性 mock Backend |
| `tests/mcp_protocol.py` | 纯 stdlib 黑盒 MCP 协议 harness（~570 行），默认拒绝真实 GUI |
| `scripts/package-release.sh` | 源无关发布目录打包：只含 binary/MCP 配置/任务提示词 + sha256 清单，含包含性校验（拒绝符号链接、源码、凭据） |
| `scripts/run-blackbox-claude.sh` | 黑盒 Claude 启动策略：cwd=发布目录、strict MCP、仅 computer 工具、sandbox-exec 拒绝读源码、失败即拒启（fail closed） |
| `scripts/sandbox-probe.sh` | 沙箱负向探针：证明 deny profile 真的拒绝读源码树（隔离证据，非提示词表演） |
| `scripts/mcp-bridge.py` | stdio ↔ 认证环回 TCP 桥（只连 127.0.0.1、16MiB 帧上限、token 从文件读取且永不打印） |
| `scripts/deploy-windows-test.sh` | Windows 部署：只复制 binary/token/任务/启停脚本，逐文件 sha256 校验，token ACL 收紧；**不启动 GUI** |
| `scripts/windows-start-host.ps1` | 协调者用：交互式计划任务启动 Host，校验进程 SessionId==explorer SessionId，否则中止 |
| `scripts/windows-stop-host.ps1` | 停止 Host 并移除计划任务 |
| `scripts/discover-windows-toolchain.sh` | 只读探查 acer-win 构建工具 |
| `examples/acceptance/*.md` | 5 个验收任务提示词 + 执行 Agent 须知（图像识别→靶点→计算器→文本编辑→取消） |
| `README.md` | 重写为新架构：实际实现的命令/权限/测试/远程路径/已知限制 |

## 2. 已执行的测试与真实结果

### 2.1 `tests/runtime_contract.rs`（白盒）

- 状态：**编译失败（被实现阻塞）**。core 的 `src/runtime` 尚未导出到 `src/lib.rs`（lib.rs 仍只有旧模块），backend 的 `Capture` 无 `Clone`（已在测试中用手工重建适配）。当前 lib 本身有 12–14 个编译错误（borrow checker、image 0.24 API `ImageReader`/`Limits` 不存在、`HashSet` 未导入、`to_json` 私有等），core/backend worker 仍在修复（`.agents/runs/*.jsonl` 活跃）。
- 在 lib 可编译前，本文件无法执行。**这是实现未就绪，不是测试缺陷**；测试未为实现放宽任何断言。
- 覆盖清单（对应任务要求）：
  - 校验先于任何事件：21 个非法动作参数化用例 + 关闭后拒绝 + not_started 语义（`invalid_action_kinds_and_shapes_never_dispatch_input` 等）
  - 真实尺寸缩放：960×540 图像 → 1920×1080 原生空间 2× 映射断言；声明尺寸==PNG 实际解码尺寸（`coordinates_map_from_capture_dimensions_to_native_input_space`、`image_declared_size_matches_encoded_size_on_observe`）
  - 陈旧序号/几何：未知 based_on、输入后旧观察失效、只读 observe 共享序号、geometry version 变化拒绝（4 个用例）
  - 副作用后去重：完全相同重放返回先前结果且 inject 计数不变；相同 ID 不同 payload 冲突；部分失败后同 ID 不盲重试；`get_step` 不产生输入（4 个用例）
  - 清理失败：close/shutdown 不得谎报 released、所有失败路径都尝试 release（3 个用例）
  - 部分派发：第二事件失败 → partial/unknown，且 ≠ not_started/dispatched（2 个用例）
  - 输入后截图失败：专用 FailCaptureBackend（inject 后 capture 必败），`dispatched` 必须保留、observation_outcome=failed/skipped、不附带图像（2 个用例）
  - 取消：cancel 标志后新输入为零、错误码 cancelled；pause 阻断输入、resume 后旧观察必须失效（2 个用例）
  - 缓存边界：请求登记表 1000 条上限、第 1001 条 resource_limit、旧记录不可被逐出；0×0 capture 不得产生零尺寸成功观察（2 个用例）
  - 语义类：chord 必须是 Key 事件非 Text、scroll 正号=下且刻度直通、hold 先 press 后 release、Unicode text_input、close 幂等、未知工具/未知会话结构化错误（7 个用例）

### 2.2 `tests/mcp_protocol.py`（黑盒协议）

- **安全闸门已验证**：
  - 无 mock 标志时拒绝运行（exit 2，输出 REFUSAL）✔
  - `--with-input` 必须搭配 `--allow-real-gui` ✔
- **自测**（`QA_HARNESS_SELFTEST=1` + 一次性假 Python host，非产品代码）：**51 通过 / 3 失败**。3 个失败全部是假 host 自身的有意简化（不校验参数类型、不响应 cancel、第二实例 lease 状态不隔离），恰好证明 harness 能抓到：参数类型不校验、取消后谎报成功、跨进程 lease 不失效。harness 逻辑（帧解析、base64/PNG 尺寸校验、EOF/取消/排他场景、错误码断言）工作正常。
- 覆盖：initialize/initialized/ping/tools-list、8 个工具精确集合、describe/open/observe 图像块（mimeType、base64、**PNG IHDR 实际尺寸==metadata**、观察必填字段）、step 结果四元 outcome、重放一致性、同 ID 不同 payload 冲突、pause 阻断、未知工具/未知方法(-32601)/畸形 JSON(-32700)/参数类型错误后服务不 wedge、EOF 后进程 15s 内退出且 rc=0、长等待 observe 的 cancel notification、双进程 lease_conflict。
- **尚未对真实 computer-host 运行**：binary 未构建成功；且 transport 未实现 `--mock-backend`/`--test-backend` 显式测试后端标志（`BackendFactory::Test` 存在但 CLI 未暴露）。**阻塞项，需要 transport worker 增加显式 mock 标志（绝不从 desktop 静默回退）**，否则协议 harness 只能在 `--allow-real-gui` 诊断模式下跑——那需要协调者调度。
- mock 图片链路通过 ≠ 真实截图能力，本报告不作此宣称。

### 2.3 脚本验证

- 全部 bash `bash -n` / python `py_compile` 通过。
- `sandbox-probe.sh` 在本机 macOS 实跑通过：deny profile 下读源码文件/列目录被拒绝，profile 可加载，树外 benign 读正常 → **OS 级隔离可实现且已证伪"提示词即隔离"**。
- `package-release.sh` 用 stub 二进制端到端跑通：目录结构、清单、RELEASE.json、包含性校验正常；仓库内 release-dir 拒绝逻辑正常（exit 2）。
- `mcp-bridge.py` 自测通过：token 作为首行发送、token 不出现在 stdout/stderr、双向帧转发正确、EOF 干净退出 rc=0；非环回 host 拒绝（exit 1）。
- `deploy-windows-test.sh`：bash 语法通过；未实跑（无 Windows 二进制，见 §4）。
- `run-blackbox-claude.sh`：未实跑（需真实 release + 协调者调度 GUI 测试）。

## 3. 发布与隔离设计（给协调者）

### 本地黑盒验收序列

```bash
cargo build --release --bin computer-host
scripts/package-release.sh                                  # → ../rpa-computer-release
scripts/sandbox-probe.sh                                    # 隔离证据（每次发布复跑）
scripts/run-blackbox-claude.sh --release-dir ../rpa-computer-release \
    --task ../rpa-computer-release/tasks/01-image-recognition.md
# 顺序：01 图像链路 → 02 靶点 → 03 计算器 → 04 文本编辑 → 05 取消
```

- 启动策略：cwd=发布目录（无 CLAUDE.md/.git/源码自动上下文）；`--strict-mcp-config`；`--allowedTools` 仅 8 个 `mcp__computer__*`；macOS `sandbox-exec` deny 源码树读，探针失败即拒启。
- **诚实声明的残余风险**：GUI 不是密闭能力沙箱。sandbox-exec 阻断进程级文件读，但 Agent 理论上可操作 Finder 视觉上浏览源码目录（截图里看到代码）。缓解：受控测试桌面（协调者确保无源码/终端窗口打开）+ 提示禁令 + 工具轨迹审计 + 最终截图人工核对。此项必须在最终验收报告中显式列出，不得宣称"完全隔离"。
- `--strict-mcp-config` 不等于禁用 hooks/插件/记忆/继承指令；第一轮验收需核对实际工具清单并记录污染因素（设计 §8.1）。
- 验收提示词均未内嵌画面答案（01/02 的真值只存在于受控桌面），防止"读提示答题"。

### Windows 路径

```bash
scripts/discover-windows-toolchain.sh acer-win   # 已执行：无 rustc/cargo/MSVC（见 §4）
# （有二进制后）scripts/deploy-windows-test.sh --host-ssh acer-win --binary <exe>
# 协调者：ssh acer-win powershell -File C:\rpa-computer-test\windows-start-host.ps1
# 协调者：ssh -N -L 127.0.0.1:8399:127.0.0.1:8399 acer-win
# 本地 MCP 客户端经 scripts/mcp-bridge.py --port 8399 --token-file <本地token副本>
```

- Host 启动强制校验 SessionId==explorer SessionId；SSH Start-Process/WMI（Session 0）路径被脚本结构排除。
- 部署只含 binary/token/任务/启停脚本，逐文件 sha256 校验；token 远端 ACL 收紧到测试用户；无 RPAD/Executor 依赖；不改防火墙。

## 4. 阻塞项与缺失工具（需协调者/其他 worker 处理）

1. **lib 编译失败（12–14 个错误）**：core/backend 仍在修复 borrow/image-0.24-API 等错误；`src/lib.rs` 尚未导出 `runtime` 模块。QA 测试在此之后才能执行。
2. **无显式 mock 后端 CLI 标志**：`tests/mcp_protocol.py` 安全默认依赖 `--mock-backend`/`--test-backend`/`--backend=mock` 之一。请 transport worker 在 `computer-host` 增加显式测试后端标志（可内置确定性 mock，窗口尺寸固定），绝不从 desktop 静默回退。否则黑盒协议测试只能以真实 GUI 诊断模式运行。
3. **Windows 工具链缺失**：acer-win 无 rustc/cargo/MSVC Build Tools（已只读确认）。选项：(a) 用户/协调者在 acer-win 安装 VS Build Tools + rustup（QA 不装）；(b) 本机已有 `x86_64-pc-windows-msvc` rust-std，但缺 MSVC linker，无法交叉链接——可评估 `x86_64-pc-windows-gnu`，但 `screenshots`/`enigo` 对 gnu target 的支持未验证，需 backend worker 确认。**未安装任何工具、未改 Windows 任何配置。**
4. **跨构建去重**：request 记录仅进程生命周期内有效（CONTRACT 允许）；1000 条上限用例在 mock 下会跑 1000 次 step，若实现过慢需标记为 slow test——待可编译后确认耗时。
5. 旧 `examples/calc_loop.rs` 与旧公共 API（action/prompt/env）仍在 README 中标注为"迁移中"，待 core 清理后需最终校对 README。

## 5. 测试未做事项（边界声明）

- 未运行任何真实截图、真实鼠标/键盘输入、未启动 GUI Host 或 Claude GUI 测试。
- 未在 Windows 安装/修改任何内容；SSH 仅用于 `echo ok` 连通性与只读发现。
- 未 commit/push；未使用 worktree/subagent；未读写父目录其他项目。
- mock 测试通过不等于真实桌面能力；所有真实 GUI 验收命令已列于 §3，由协调者串行调度。
