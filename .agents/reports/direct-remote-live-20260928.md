# Computer Client / Host 直接 TLS 连接：实现与真机验证

日期：2026-09-28。工作目录：`rpa-computer`。状态：**本轮远程直连功能完成并实测通过；模型 GUI 任务验收未通过，不在本报告中冒充完成。**

## 交付内容

```text
本机 Claude CLI
    ↓ stdio MCP（原有 8 个 computer_* 工具）
本机 Rust computer-client
    ↓ 直接 TCP / TLS + Token（不是 SSH 隧道，也不是 HTTP）
Windows Rust computer-host（TLS 监听 / 子进程监督）
    ↓ 认证成功后启动同一个 exe 的 stdio 模式
既有 Runtime / 原生桌面后端
```

- Host CLI：`--remote-listen IP:PORT --tls-cert PATH --tls-key PATH --token-file PATH [--log-file PATH]`。
- Client CLI：`--connect HOST:PORT --ca-cert PATH --server-name DNS-NAME --token-file PATH`。
- 严格 CA 与证书 DNS 身份验证，无 insecure 模式。Token 在 TLS 握手后发送，文件读取，不在参数中传值。
- Host 认证成功返回内部 `AUTH_OK\n`，Client 消费它后才转发 MCP。错误 Token / 缺失 ACK 为非零退出，ACK 不污染 MCP stdout。
- 单控制连接；额外并发连接拒绝。连接结束回收 owned stdio 子进程，下一次连接获得新 worker/session。不自动重连或重放动作。
- 最终 stdout 写入/flush 失败、join 超时、强制中止不伪报成功。
- 新增凭据生成、source-free Client 打包、Windows Interactive Scheduled Task 启停脚本及使用文档。
- 运行时为 Rust 二进制；Python/OpenSSL 只用于构建辅助、测试和凭据准备。

主要文件：`src/bin/computer-client.rs`、`src/mcp/remote/`、`src/bin/computer-host.rs`、`scripts/*remote*`、`tests/remote_transport.rs`、`tests/remote_packaging.py`、`docs/remote-connection.md`。源码未提交、未 push，未创建 worktree，未修改父 RPA 工程。

## 最终独立回归

| 验证 | 实际结果 | 日志（均在 `.agents/runs/`） |
|---|---|---|
| `cargo test --offline -- --test-threads=1` | **271 passed / 0 failed / 2 ignored** | `direct-remote-all-tests-green-20260928.log` |
| Rust lib | 204 passed / 1 ignored | 同上 |
| 原有 protocol integration | 12 passed | 同上 |
| 新 TLS remote integration | 17 passed | 同上 |
| runtime_contract | 35 passed / 1 ignored | 同上 |
| transport_lifecycle | 3 passed | 同上 |
| `cargo fmt --check` | exit 0 | `direct-remote-final-format-20260928.log` |
| `cargo clippy --all-targets --offline -- -D warnings` | exit 0 | `direct-remote-final-clippy-20260928.log` |
| Python MCP protocol（显式 mock） | 50 passed / 0 failed / 1 skipped | `direct-remote-final-protocol-20260928.log` |
| Python 旧 stdio / loopback transport | 40 passed / 0 failed | `direct-remote-final-legacy-transport-20260928.log` |
| 旧发布/隔离脚本 | 81 passed / 0 failed | `direct-remote-legacy-scripts-20260928.log` |
| 新 remote packaging | 32 passed / 0 failed / 1 skipped（共 33） | `direct-remote-packaging-live-final-20260928.log` |
| Mac release（两个 binaries） | exit 0 | `direct-remote-final-mac-release-20260928.log` |
| Windows x86_64 MSVC release（两个 binaries） | exit 0 | `direct-remote-final-windows-build-20260928.log` |
| Windows PowerShell ParseFile / 启动 / 两种停止路径 | 实机通过，见下文 | `direct-remote-windows-*.log` |

明确跳过：Rust 的手动真实截图诊断与 1000 次顺序步骤慢测试；Python mock 测试不能证明原生跨进程桌面锁；Mac 没有 pwsh，packaging suite 的本地 PowerShell parser 测试跳过。另已在 acer-win 上实际 ParseFile 与运行启停脚本，不能把这个 skip 写成“本机测试通过”。

中途红灯日志保留用于追溯；最终结论仅以上表的最终绿色日志为准。构建后产品源码与 Cargo 文件哈希未变化；最后修改仅修正 raw-TLS 测试的关闭/重连时序，且完整套件再次通过。

## acer-win 真机直连证据

- 动态探测地址为 `100.200.20.168`，端口 `8399`。此地址是本次检测结果，不应永久硬编码。
- Mac 到该地址路由走 `en1`。实际 Client socket 为 `100.200.20.32:53565 → 100.200.20.168:8399`，Windows 端也记录对应 peer。
- **没有 `ssh -L/-R/-D`。** SSH 只用于部署、启停、读日志/进程/TCP 状态。
- 首次 Mac TCP 连接超时；用户手动允许访问后，同一端口可直连，随后以下测试通过。协调者未修改防火墙；仅凭此现象不额外推断用户修改了哪一种具体策略。
- 最终测试 Host PID `20684`，Session `1`。原生 worker PID `18192` / `18924` 也在 Session `1`，ParentProcessId 为 `20684`，不是 Session 0。
- Host 通过 Interactive / Limited Scheduled Task 启动，以活动控制台 explorer 的属主为 principal。不使用 SSH Start-Process/WMI 启动桌面进程，不依赖 RPAD/Executor。

### 认证、截图与正常重连

日志：`direct-remote-native-after-allow-20260928.log`。

- 错误服务器名：Client exit **4**，stdout 0 字节。
- 错误 CA：Client exit **4**，stdout 0 字节。
- 错误 Token：Client exit **5**，stdout 0 字节；Host 日志为 invalid token，未启动 worker。
- 控制连接已占用时，第二个 Client exit **4**，未获取控制权。
- initialize / tools/list 成功，工具集恰好 8 个。
- 第一次真实会话 `session-18192-1`，PNG **960×540 / 131599 bytes**，SHA-256 `0047fff7e35e2206b458f50abbdf7c35a1201b651f7d07e85e99e9f745052218`。
- 第二次真实会话 `session-18924-1`，PNG **960×540 / 131647 bytes**，SHA-256 `20df0e5930916474c627cfed35893acdfbff94098a453cb2437f214c83d5051b`。
- 截图已由协调者查看，确为真实 Windows 桌面，非 mock 图。图片在被 gitignore 的 `.agents/runs/` 下，避免提交私人桌面内容。
- 两次 `computer_close` + Client stdin EOF 均 exit **0**，Host 保持监听并为下一连接创建新 worker。
- **没有调用 computer_step，没有注入键鼠输入。**

### 异常断线

日志：`direct-remote-native-disconnect-20260928.log`。

- 打开 `session-21780-1` 后，只强制结束本次 owned Mac Client，模拟未发 TLS close_notify 的断线。
- 等待后 Windows 只剩监听父进程 `20684`，旧原生 worker 已消失。
- 下一连接获得 `session-20848-1`，可打开、关闭并正常退出。
- Host 对 TLS 截断明确记录错误；错误不被当作 clean success，但不阻止完成清理后的下一连接。

### 本机真实 Claude CLI

日志：`direct-remote-claude-readonly-20260928.jsonl` / `.err`。

- 使用用户既有 CLI / 模型 / 认证配置，**没有传 --model，没有修改全局配置**。
- cwd 为源码外 Client 发布目录，关闭内建工具、禁用 hooks，只加载本次 strict MCP 配置。
- MCP server 实际状态 `connected`，可见原有 8 个 computer 工具。
- 转录中实际且唯一的调用是 `mcp__computer__computer_describe`，工具返回 `platform=windows`、`available=true`，CLI exit 0。
- 仅验证 Agent→Client→Host 工具连通性；未让模型接收截图或操作桌面。
- Claude 退出时关闭 MCP 子进程会产生截断日志；Host 已回收对应 owned worker，没有残留。

## Windows 脚本真机修复与清理

真机验证发现并修复静态解析未捕获的问题：
1. `(try {...} catch {...})` 会被解释为调用名为 try 的命令；改为 Where-Object 内真正的 try/catch 语句，无法读取进程创建时间时 fail closed。
2. 计划任务把 `NODE1\\Administrator` 规范化为 `Administrator`；停止脚本改为严格 SID 等价校验，同时验证记录中的 principal_user 与 principal_sid 自洽，保留 action / argv / 会话 / 创建时间等检查。

已测试：首轮 provisional record 清理；最终 healthy record 停止；停止后独立确认 owned listener、worker、任务、实例记录及 8399 LISTEN 均不存在。日志：`direct-remote-windows-provisional-stop-20260928.log`、`direct-remote-windows-final-stop-20260928.log`、`direct-remote-final-cleanup-20260928.log`。

服务器私钥和 Token ACL 禁用继承，仅控制台用户 SID、SYSTEM、Administrators；本机凭据目录 0700，私钥/Token 0600。检查 Host 日志不含真实 Token。CA 私钥未复制到远端。

## 保留产物与重新使用

**当前测试 Host 已停止，不是仍在运行。**

- Windows：`C:\computer-direct-20260928-final`，保留 binary、证书/Token、脚本及 host.log。
- Mac source-free Client：`/private/tmp/computer-direct-client-20260928-final`。
- 本地 MCP 配置：`/private/tmp/computer-direct-client-20260928-final/mcp/mcp.json`。
- 外置测试凭据：`/private/tmp/computer-direct-credentials-20260928-final`。这些是临时验证产物，不是持久化产品安装目录；目录被清理后应重新 provision/package，不把私钥放入仓库。
- Manifest 逐文件校验通过；Client 包不含源码、Token 或证书内容。

重新启动前先探测 Windows 当前 LAN 地址；使用文档中的 `windows-remote-start.ps1`，把 `RemoteDir` 指向上述目录，`ListenAddress` 传当时实际地址。地址变化时也更新 Client `--connect`；证书 DNS 身份仍为 `acer-win`。将本地 mcp.json 交给本机 Claude CLI 即可，SSH 不参与运行时数据通道。

产物 SHA-256：
- Mac Client：`d4639f7a5981aeaa83667d13753a2dc99d9c926c1eb596e7dcbefe8ce345cfad`
- Mac Host：`5636e52a4a24224ecf94b2283346ecbcd6e9b35a5af9d079b46b1d9e7a051b40`
- Windows Host：`1ef1a6e464966ab72700f46ea2b41af1fa6e08349a9fdf47e76807c8ace08bae`（远端 Get-FileHash 一致）

## 未宣称完成的部分

- **没有通过“Claude 看懂截图并操作计算器算 10×20”的模型 GUI 门禁。** 之前图片理解异常的根因仍未证明；本轮不擅自更换模型、不用协议测试代替 GUI 成功。
- 未完成每平台每任务 10 次且至少 8 次真实成功的完整 GUI 验收。
- 本轮真机没有验证键鼠动作或真实进行中动作的取消；已有 mock / runtime 回归不应描述为这类真机结果。
- 本版是单 Token、受控 LAN、单控制者的 CLI/服务实现，不是公网多租户服务或完整托盘/安装包产品。
