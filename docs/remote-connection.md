# 远程直连：Mac Client ↔ Windows Host（TLS + Token）

Mac 上的 Claude 只连**本地** `computer-client`（stdio MCP），Client 通过 **TLS 保护的换行 JSON-RPC + 共享 Token** 直连 Windows 上的 `computer-host`。
**这不是 HTTPS，也不是 MCP Streamable HTTP**：TLS 只是给原有 TCP 传输加密的 raw JSON-RPC 通道。SSH 只用于部署/启动/停止/取日志，**不做端口转发**（无 `-L/-R/-D`）。证书用独立私有 CA + DNS SAN 服务器证书。

证书/Token 由 Python 标准库 + openssl CLI 生成（**仅 provisioning 阶段**）；运行时只依赖两个 Rust 二进制（`computer-host`、`computer-client`），Windows 上**不需要 Python**。

## 1. 在 Mac（协调机）生成凭据

```bash
# 输出目录必须不存在，且其任何祖先路径组件都不得是符号链接；
# 整个目录 0700，私钥/Token 0600。
# --server-name 只接受 DNS 名（拒绝 IP 字面量：本工具只签发 DNS SAN 证书；
# 连接时用 --connect 传 IP 即可，二者职责分离）。
scripts/remote-credentials.py --output-dir ~/rpa-remote-creds \
    --server-name rpa-host.lan
```

产物：

```
~/rpa-remote-creds/
  ca/ca.key        私有 CA 私钥 —— 留在本机，绝不打包/分发
  ca/ca.pem        CA 证书
  host/server.pem  服务器证书（SAN: DNS:rpa-host.lan，EKU serverAuth）
  host/server.key  服务器私钥 —— 只拷到 Windows
  host/host.token  共享 Token（host 副本）
  client/ca.pem    Client 信任根
  client/client.token  共享 Token（client 副本）
```

安全性：输出目录在任何 openssl 工作之前以排他 `mkdir` **原子占位**，并发进程无法抢占；全部内容先在临时目录生成并**原子交换**到位（Linux 用 `renameat2 RENAME_EXCHANGE`，其他平台用 `os.replace` 覆盖自己占位的空目录），失败即整体回滚——只清理自己创建的目录，私钥/Token 先覆写再删除，**绝不打印其内容**。提交前做 fail-closed 校验：host/client Token 副本逐字节一致、熵 ≥ 256 bit、host 私钥 ≠ CA 私钥。

`--server-name` 是证书里的身份名，Client 侧 `--server-name` 必须一致；`--connect` 可以用 IP。

## 1b. 构建二进制

```bash
cargo build --release --bins
# 产物: target/release/computer-host(.exe) 和 target/release/computer-client
```

**跨平台交叉编译（Mac → Windows，可选）**：首次失败若缺 `llvm-lib`，**无需安装任何新工具**——Rust 自带的 `rust-lld` 支持 `-flavor link` 提供 `link.exe`/`lib.exe` 兼容模式。以下为 **Mac(aarch64-apple-darwin) 特定**环境变通（已验证），并非通用要求；Windows 本机在装有标准 MSVC 工具链时直接 `cargo build` 即可：

```bash
# 仅适用于 Mac 上交叉编译到 x86_64-pc-windows-msvc；不要硬编码用户主目录
eval "$(cargo xwin env --target x86_64-pc-windows-msvc)"
export DYLD_LIBRARY_PATH="$(rustc --print sysroot)/lib"
export AR_x86_64_pc_windows_msvc="$(rustc --print sysroot)/lib/rustlib/$(rustc -vV | sed -n 's/^host: //p')/bin/rust-lld"
export ARFLAGS_x86_64_pc_windows_msvc='-flavor link /lib'
cargo build --release --target x86_64-pc-windows-msvc --bins --offline
```

## 2. 查询 Windows 的 LAN 地址

在 Windows 上（无需管理员）：

```powershell
ipconfig                                   # 看活动网卡的 IPv4 地址
Get-NetIPAddress -AddressFamily IPv4 | Where-Object {$_.PrefixOrigin -ne 'WellKnown'}
```

从 Mac 侧验证可达：`nc -vz <WIN_IP> 8399`（启动 host 之后）。

## 3. 部署并启动 Windows Host

把 `computer-host.exe`、`host/server.pem`、`host/server.key`、`host/host.token` 和两个 ps1 脚本通过 SSH/scp 部署为：

```
C:\rpa-remote\bin\computer-host.exe
C:\rpa-remote\tls\server.pem
C:\rpa-remote\tls\server.key
C:\rpa-remote\host.token
C:\rpa-remote\windows-remote-start.ps1
C:\rpa-remote\windows-remote-stop.ps1
```

启动（**整条远程命令用单引号包裹**——否则本地 bash/zsh 会吞掉 Windows 路径里的反斜杠；`-ListenAddress` 必填且**禁止 0.0.0.0/wildcard**，用第 2 步**动态查到**的地址，不要照抄示例 IP；默认端口 8399。地址用 `[System.Net.IPAddress]::Parse` 解析，IPv6 字面量会自动加方括号以满足 Rust `SocketAddr`；省略 `-TaskName` 时自动生成 **GUID 唯一**任务名，绝不按端口共享）：

```bash
ssh acer-win 'powershell -NoProfile -ExecutionPolicy Bypass -File C:\rpa-remote\windows-remote-start.ps1 -RemoteDir C:\rpa-remote -ListenAddress 192.168.1.50 -Port 8399'
```

start 脚本：在**活动控制台会话**（WTS 会话 + 该会话 explorer.exe 属主，Interactive/Limited 计划任务）中**直接**启动 exe（无 cmd 包装）；路径参数在内部调用时**加引号以允许含空格路径**，但参数值本身拒绝引号/控制字符（argv 不做二次转义，直接禁止元字符）；部署文件若是 reparse point/符号链接直接拒绝；把 `host.token` 与 `tls\server.key` 的 ACL 设置为**仅控制台用户 SID + SYSTEM + Administrators**；**拒绝任何已存在的 `remote-host-instance.json`**——启动时绝不静默覆盖/删除旧记录，须先运行 stop 清理；**先写入 provisional 身份记录再启动任务**，随后校验指定地址端口的 LISTEN 确属该 PID：失败则把记录标记为 `unhealthy`（含原因）**再**报错退出，不误杀任何进程；身份（PID/会话/exe/创建时间 UTC ticks/任务参数/principal）最终写入 `remote-host-instance.json`（记录中只有路径，**绝无 Token/私钥内容**）。**不改防火墙**；若防火墙拦截会如实失败并保留记录供清理。

停止（**任何身份字段核验不过就完全不动**——任务名/PID/exe/创建时间精确 UTC ticks/会话/任务 action/principal 全部匹配才动手；15 秒等待后 force-kill 前还会**重新校验** PID 身份防复用）：

```bash
ssh acer-win 'powershell -NoProfile -ExecutionPolicy Bypass -File C:\rpa-remote\windows-remote-stop.ps1 -RemoteDir C:\rpa-remote'
```

日志：`C:\rpa-remote\host.log`（host 自己的 `--log-file`，启动 GUI 会话无需交互）。

## 4. 打包并配置 Mac Client

```bash
scripts/package-remote-client.sh \
    --binary target/release/computer-client \
    --release-dir /private/tmp/computer-remote-client-release \
    --connect 192.168.1.50:8399 \
    --server-name rpa-host.lan \
    --ca-cert ~/rpa-remote-creds/client/ca.pem \
    --token-file ~/rpa-remote-creds/client/client.token
```

`--release-dir` 必须**不存在**、**不得与源码树重叠**（任何方向）、**任何祖先不得是符号链接**；相对路径按**当前工作目录**解析并规范化为绝对路径。mcp.json 中所有路径均为**绝对路径**，可从任意 cwd 启动；`--connect` 校验为 `HOST:PORT`（IPv6 必须带方括号 `[v6]:port`），host 用 `ipaddress` 模块解析、DNS 名按 RFC 1123 校验；`mcp.json` 与 `RELEASE.json` 均由 `json.dump` 生成，路径中的引号/反斜杠不会破坏 JSON。

release 目录**只含**编译产物、MCP 配置（引用外部凭据路径，Token/证书**不复制进包**）、验收任务 prompt、`RELEASE.json`、`MANIFEST.sha256`。手写配置可参考 `examples/mcp_remote_client.json`。MCP 配置只含 client 命令 + 地址 + 凭据**文件路径**；Token 不进 argv、不进日志。

把生成的 `mcp/mcp.json` 交给 `scripts/run-blackbox-claude.sh`（`--strict-mcp-config`）即可。

## 5. 诊断

- **TCP 可达性**：`nc -vz 192.168.1.50 8399`；Windows 侧 `Get-NetTCPConnection -State Listen -LocalPort 8399`（OwningProcess 应是 host PID）。
- **TLS 证书链 + 主机名**（模拟 client 校验）：
  ```bash
  openssl s_client -connect 192.168.1.50:8399 \
      -CAfile ~/rpa-remote-creds/client/ca.pem \
      -verify_return_error -verify_hostname rpa-host.lan < /dev/null
  ```
  退出码 0 = 链与主机名均通过；换错误的 `--server-name` 必须失败。
- **Token 错误 / 明文连接 / 多余并发连接**：host 一律拒绝并关闭，且未认证不创建任何桌面 worker；host 单控制连接。
- **CA/证书/Token 轮换**：完全手动——重新运行 `remote-credentials.py` 到新目录，重新分发 host/client 两侧文件，stop 再 start。旧凭据不自动失效，请删除旧目录。
- **AUTH_OK 应用层 ack 由 Client 内部消费**：认证成功后 host 先回 `AUTH_OK`，Client 在 MCP 握手之前内部读取该校验——认证被拒则**非零退出**，且绝不把 ack 字节泄漏到 MCP stdout（stdout 只承载 MCP 帧）。
- Client **不自动重连、不重放动作**（无法确认动作是否已执行）；连接失败非零退出并输出 stderr。

## 边界与免责

- 单共享 Token、受控 LAN；不承诺多租户/公网抗 DoS；不自动改防火墙。
- PowerShell 脚本与打包验证：2026-09-28 coordinator 已在 acer-win 上用 PowerShell `Parser.ParseFile` 对两个 ps1 做真实解析，**均通过**；本地打包测试更新为 **33 项：32 通过 1 跳过**（跳过项因本机无 pwsh）。真实 Windows 侧已单独实测 ParseFile、启动、provisional 清理与健康停止。
- **Live 验证已完成（2026-09-28，acer-win）**：Interactive Task 运行在 Session1；Mac 原生 Rust `computer-client` 直连探测到的 Windows IP（8399 端口），**TLS 错误 CA / 错误主机名 / 错误 Token 以及并发 busy 均被拒绝**；取得 **2 张真实 960x540 截图**（来自两个不同会话，内容有区分）；客户端异常断开后 host 回收子进程，随后的全新会话正常工作。**未注入任何 GUI 输入**。
- 初次连接 TCP 超时，由**用户手动放行**网络访问后直连通过；agent **从未修改防火墙，也未使用 SSH 隧道**。DHCP 环境下 IP 变更后必须重新探测。
- 本地真实 Claude CLI（配置模型未改动）仅调用 `computer_describe`，收到 `platform: windows`、`available: true`。这是 **agent/工具连通性**证据，**不代表**模型理解截图内容或计算器任务验收——此前的 GUI 门禁**仍未通过**。
- 最终部署的 host/任务已停止，清理由 coordinator 检查（若 final-cleanup 日志尚缺，以"已停止"表述，验证跟踪见报告）。保留了二进制、证书文件与日志；**未修改任何持久化 Claude/全局配置**。测试部署位置 `C:\computer-direct-20260928-final`，本地无源码 client 包位于 `/private/tmp/computer-direct-client-20260928-final`（临时验证产物，非永久安装）。
- 完整 live 证据报告（由 coordinator 撰写）：`.agents/reports/direct-remote-live-20260928.md`；为避免泄露截图/私人桌面细节，本文档不列具体画面内容。
- 本文档**不代表** GUI 端到端验收已通过；模型"看懂图片/计算器"等任务门禁与本直连功能分开报告。
