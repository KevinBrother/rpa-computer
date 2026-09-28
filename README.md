# rpa-computer

跨平台 computer-use Runtime：让已有 Agent（如 Claude Code）通过 **截图 + 键鼠**
操作计算机。API/CDP/a11y 覆盖不了的场景，视觉通道仍可独立工作。

**本项目不是 Agent**：不包含模型调用、任务规划、重试策略。Claude Code（用户已配置好模型）
作为外部 Agent，通过 MCP 使用本 Runtime 提供的 `computer_*` 工具。

```
用户任务 → Claude Code（已有模型配置）→ MCP（stdio/loopback TCP）
         → computer-host（本仓库编译产物）→ Runtime → 平台后端 → 当前桌面
```

## 架构与模块

| 模块 | 职责 |
|---|---|
| `src/backend/` | 平台后端 trait + macOS/Windows 实现：真实截屏（PNG 实际尺寸）与键鼠注入、按键名校验、按下状态清理 |
| `src/runtime/` | 会话、观察、坐标变换（图像像素 ↔ 原生输入单位）、动作校验、串行执行、请求去重、取消、清理、结构化结果 |
| `src/mcp/` | stdio/loopback-TCP MCP 接入层、跨进程桌面写者互斥锁、看门狗、紧急停止 |
| `src/bin/computer-host.rs` | 唯一对外二进制（见下） |
| `tests/runtime_contract.rs` | 独立白盒故障注入契约测试（mock 后端，不碰真实桌面） |
| `tests/mcp_protocol.py` | 纯 stdlib 黑盒 MCP 协议测试（默认拒绝真实 GUI） |
| `scripts/` | 发布打包、黑盒 Claude 启动（含沙箱隔离）、Windows 部署/交互式启动、stdio↔TCP 桥 |
| `examples/acceptance/` | 黑盒验收任务提示词（图像识别→靶点→计算器→文本编辑→取消） |

当前源码树只包含上表所列 Runtime 模块（早期模型直调 demo 已移除，不再存在
"迁移进行中"的旧代码）。设计依据为 `.agents/CONTRACT.md` 与设计文档
`docs/2026-09-24-computer-use-runtime-design.md`。

## 当前状态（2026-09-28）

- 实现仍在验证与加固中，当前源码与候选二进制**均非最终冻结物**，本文不作任何
  最终通过声明。
- 独立全量回归（2026-09-28，`cargo test --all-targets --offline`）：**246 通过、
  2 忽略**（196 单测 + 12 协议 + 35 契约 + 3 传输）；`cargo fmt --check` 与
  `cargo clippy --all-targets -- -D warnings` 干净。最近一次 Clippy 收尾修复见
  `.agents/reports/clippy-closeout-20260928.md`。
- 黑盒 MCP 协议回归：mock 传输 **40 通过**；协议套件 **50 通过 1 跳过**。
- 真机验证进展：无源码真实 Claude → MCP → Windows Host（Session1 交互式桌面）
  链路已能**截取真实桌面**（1920 物理桌面，图像按 1365/960 缩放返回）并成功
  **派发键鼠输入**；8 工具策略审计通过、只读约束合规。**但语义正确性未成立**：
  计算器任务失败——模型把清晰桌面描述为黑屏并触达上下文上限；随后两次只读视觉
  探针编造了截图中并不存在的 nonce/图形。即 Windows 截图 + 输入派发链路已建立，
  **语义级 GUI 验收成功次数仍为 0**，不作任何 8/10 之类的通过声明。
- 当前阻塞点：视觉通道的**语义正确性**（模型/服务图像链路存疑，需排查，不能
  断定模型必然无视觉能力）。macOS 侧截图预检返回了真实图像，但尚无受控 GUI
  验收通过记录。
- 历史记录（2026-09-24）：当时 Windows 桌面处于锁屏，GDI 截图返回
  `0x80070005` 拒绝访问；当日 release 协议回归为 49 通过 1 失败（observe+立即
  取消竞态）。该竞态已由传输写者修复（见 `.agents/reports/` 下传输收尾报告），
  锁屏问题已在解锁的交互式会话中解除，但如上文所述，语义验收仍未通过。

## computer-host 命令行

```text
computer-host                              stdio MCP 服务（默认）
computer-host --listen 127.0.0.1:PORT --token-file PATH
                                           仅环回 TCP（Windows 交互会话远程测试用）；
                                           连接后第一行必须是 **原始 token 字符串**（换行结尾，
                                           不是 JSON 帧），拒绝非环回绑定
computer-host --describe                   打印能力与诊断（不产生输入）
computer-host --version                    版本
computer-host --mock-backend           **诊断 mock 后端**：协议与诊断链路测试用，
                                           不做真实截屏/键鼠，绝不视为真实 GUI 证据；
                                           可与 stdio 或 --listen 组合
computer-host --help                       帮助
```

- **默认（不带 --mock-backend）即为真实后端**：真实截屏与键鼠注入，需要目标平台
  相应权限/交互会话；只有显式 `--mock-backend` 才切换到协议级 mock。

- stdout 只承载 MCP 协议帧；诊断走 stderr，不含 token 或图像数据。
- 同一 OS 用户会话同时只允许一个 Host 持有桌面写者锁；锁随退出/崩溃释放。
- 停止路径：连接 EOF、MCP cancellation、SIGINT、本机紧急停止热键（若注册成功，
  `--describe` 可见）；停止后尽力释放本会话按下的键鼠，失败会如实上报。

## 权限（macOS）

首次使用需在 系统设置 → 隐私与安全性 授予运行终端/宿主进程：
- **屏幕录制**（截图；`screen_capture_denied` 报错即缺此项）
- **辅助功能**（键鼠注入；`permission_denied` 报错即缺此项）

权限不足时 Host 会明确区分截屏与输入两类失败并给出本机处理指引，不尝试提权或绕过。

**Windows**：Host 必须与用户桌面同处一个**交互式、未锁定**的会话，边界照旧生效
（权限隔离、安全桌面如 UAC/锁屏/LogonUI 不可被普通进程截获或注入；锁屏会话中
GDI 截图会被拒绝，如历史上的 `0x80070005`）。当前实测（2026-09-28）：在解锁的
Session1 交互式桌面中，**真实截图（含缩放后的实际图像）与键鼠输入派发均已成功**；
但模型对截图内容的语义描述不可靠（见"当前状态"），语义级 GUI 任务验收仍为 0。
即：构建、协议链路、截图与输入通道在 Windows 已验证，**视觉语义正确性尚未验证**。

## 构建

```bash
cargo build --release --bin computer-host                          # macOS 本机
# Windows 原生（在 Windows 机器上）：
cargo build --release --target x86_64-pc-windows-msvc --bin computer-host
# 本机交叉编译（本环境已缓存 cargo xwin 0.23.1 + SDK/CRT，经
# scripts/discover-windows-toolchain.sh 只读检测）：
cargo xwin build --release --target x86_64-pc-windows-msvc
```

构建通过仅说明产物可编译部署；与"输入/截屏已在真实桌面验证成功"是两回事
（后者当前未完成，见"当前状态"）。

## 测试

```bash
cargo test --test runtime_contract    # 白盒故障注入（确定性 mock，不操作真实桌面）
cargo test --test protocol            # core 协议/状态测试
python3 tests/mcp_protocol.py --host target/release/computer-host "--host-arg=--mock-backend"
                                      # 黑盒 MCP 协议（默认要求显式 mock 后端标志；
                                      #  --allow-real-gui 仅协调者调度时使用）
python3 tests/mcp_protocol.py --host <fake-host> "--host-arg=--mock-backend" --selftest
                                      # 隔离 fixture 自测：跨进程租约场景显式跳过
                                      # （产品专属行为），其余场景要求零失败
```

注意：mock 后端的图片链路测试**不证明**真实截图能力；真实 GUI 验收由协调者统一调度。

模型与认证使用用户本机已有的 Claude Code 配置（`--setting-sources user`，见下）；
本项目不需要、也不配置 Runner/Model Bridge/API key，也不需要源码访问作为验收条件。

## 本地 MCP 接入（Claude Code 验收）

```bash
scripts/package-release.sh   # 默认在临时根下创建唯一发布目录，例如
                             #   $TMPDIR/computer-release.<uuid>-<pid>
                             # 结束时打印 ">> release ready: <路径>"；
                             # 复制该打印路径用于下一步。目录含无源码二进制、
                             # MCP 配置、任务与 MANIFEST.sha256 哈希清单。
                             # 也可用 --release-dir PATH 指定稳定位置，
                             # 但目录名禁止 rpa-computer-*/rpa-core-*
                             # （该命名形状保留给受控源码副本，脚本会拒绝）
scripts/sandbox-probe.sh     # 负向探针：证明沙箱确实拒绝读源码
scripts/run-blackbox-claude.sh --release-dir <上一步打印的发布目录> \
    --task <发布目录>/tasks/03-calculator.md --print
```

`run-blackbox-claude.sh` 额外选项：

```text
--verify-only     用临时目录中的假 claude 二进制校验启动 argv 策略
                  （--tools ''、禁止提权/绕过 flag、固定位置审计），
                  不启动真实 GUI，也不作为 GUI 证据
--print           验收路径：--output-format stream-json --verbose
                  --no-session-persistence --max-turns N，真实转录与 stderr
                  捕获到 <release>/evidence/<run-id>/（0700，在源码树外，
                  token 从不传给 Claude 故转录不含凭据）
--max-turns N     回合预算（默认 12）；超限 = 运行无效
--max-seconds S   墙钟预算（默认 600）；看门狗只终止本次运行的子进程，
                  绝不按进程名匹配
--diagnostics     诊断模式：配合 --no-sandbox 才允许跳过 sandbox-exec
                  （验收路径禁止 --no-sandbox）
```

**转录审计（fail-closed）**：print 运行结束后，启动器自动对真实转录执行
`scripts/audit-claude-transcript.py`——init 事件的有效工具清单必须**恰好**是
8 个 `mcp__computer__computer_*` 工具、MCP 服务器只有 `computer`（connected）、
全部工具调用必须是 computer_*、无权限拒绝、无 hook 执行记录、无 API/模型错误、
不超预算；任一不满足则**运行无效**（非零退出）。审计可手工重跑：
`scripts/audit-claude-transcript.py <transcript.jsonl> --max-turns 12 --max-seconds 600`。
通过审计只证明运行策略合规，**不**证明 GUI 任务正确（真值比对由协调者执行）。

黑盒启动策略：进程 cwd 为发布目录（无 CLAUDE.md/.git/源码自动上下文），
`--tools ''` 关闭全部内建工具（`--allowedTools` 只是自动批准清单，不是隔离），
`--strict-mcp-config` 只加载本项目 MCP，`--allowedTools` 仅 8 个 `computer_*` 工具，
`--disable-slash-commands` + 每次运行的 `disableAllHooks` 设置，
`--setting-sources user`（保留用户自己的模型/认证配置，忽略项目/本地设置），
`--system-prompt` **替换式**干净系统提示，任务文本在 --print 与交互模式都显式传入
（且必须非空、明确要求基于截图作答）；
macOS 下用 `sandbox-exec` 在 OS 层拒绝读取原始源码树、**已知源码副本**
（如 /tmp/rpa-core-shim）以及各临时根下匹配 `rpa-computer-*/rpa-core-*`
命名形状的源码暂存子树（该形状保留给受控源码副本）。**安全命名的发布目录位于
临时根下是支持的默认方式**（`package-release.sh` 默认即创建
`$TMPDIR/computer-release.<uuid>-<pid>`）；脚本仅拒绝把 `/tmp`、`/private/tmp`、
`$TMPDIR` 这些**根目录本身**当作发布目录、发布目录与源码树重叠、或目录名匹配
上述 `rpa-computer-*`/`rpa-core-*` 形状，均 fail-closed。

**隔离边界（不夸大）**：工具清单收缩（恰好 8 个 computer_* 工具）+ macOS
`sandbox-exec` 拒绝读源码，是**进程级文件读**限制；它不是绝对 OS 沙箱。
受测 Agent 理论上可通过 GUI 打开文件管理器/终端**目视**访问源码目录——
GUI 通道不受 sandbox-exec 约束。缓解措施是受控测试桌面（不打开源码/终端窗口，
准备清单见 `examples/acceptance/GUI-TEST-FIXTURE.md`）+ 提示禁令 +
完整工具调用轨迹审计；此残余风险必须在 QA 报告中列出。
脚本级回归测试：`scripts/test-release-scripts.sh`（危险发布目录拒绝、隔离 fixture
打包、argv 策略、真实形状转录审计、端到端假 claude 沙箱运行、Windows 启停脚本
所有权静态检查、mcp_protocol.py 对隔离假 host 的端到端自测；纯脚本，不启动 GUI）。

## Windows 远程测试（协调者执行）

```bash
scripts/discover-windows-toolchain.sh acer-win   # 只读探查构建工具
scripts/deploy-windows-test.sh --host-ssh acer-win --binary target/x86_64-pc-windows-msvc/release/computer-host.exe
# 部署脚本：MANIFEST.sha256 不覆盖自身；远端 PowerShell 全部经 EncodedCommand
# （无嵌套引号）；对部署的 .ps1 做远端**解析**检查（不执行）；token 按 SID
# 白名单 ACL（控制台 explorer 属主 + SYSTEM + Administrators），内容永不打印。
# 以下由协调者在确认后执行：
ssh acer-win 'powershell -File C:\rpa-computer-test\windows-start-host.ps1'   # 交互式计划任务启动（整体单引号，防本地 shell 吞掉反斜杠）
ssh -N -L 127.0.0.1:8399:127.0.0.1:8399 acer-win                            # 端口转发
# 本地 MCP 客户端经 scripts/mcp-bridge.py 连接（token 从文件读取，永不打印；
# 认证方式：连接后第一行发送原始 token，非 JSON）
```

Host 必须通过 **interactive 计划任务** 在 explorer 会话中启动；启动脚本通过
WTS API 选取当前活动控制台会话，以该会话中 explorer.exe 的**属主**作为任务主体
（不采用 SSH 环境变量用户），同名计划任务**碰撞时 fail-closed**（只重注册动作路径
与主体都与本部署一致的自有任务，绝不删除无关任务）；token 文件 ACL 按 **SID**
校验（非显示名，避免区域设置差异）。启动后按 PID + 可执行文件路径 + SessionId +
创建时间复核新进程身份，写入 `host-instance.json`；stdout/stderr 重定向到部署目录
持久日志。停止脚本只停止 `host-instance.json` 记录的任务/PID：`-TaskName` 不允许
覆盖为记录外的任务；停任务前复核其动作路径与主体，停进程前复核 PID 对应的 exe
路径**与创建时间**（防 PID 复用误杀），不做名称模式匹配、不碰无关进程、不改
防火墙/网络。

## 已知限制

- 单会话、单主屏目标；屏幕几何/DPI 变化会使旧观察失效并进入 Faulted。
- 跨进程崩溃不承诺 exactly-once；结果未知时重新观察，不盲重放。
- 截图经 Claude Code 发送到用户配置的模型服务；本项目不约束其留存策略。
- 本机紧急停止热键若注册失败会明确报告，不声称等价于远程 Ctrl+C。
- 会话总预算 10 分钟、单步 15 秒等上限见设计文档第 5 节。
