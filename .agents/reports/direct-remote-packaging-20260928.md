# Direct-Remote Packaging Sidecar — 实现报告（2026-09-28）

任务：实现与 Rust 核心完全解耦的直连远程打包侧车（凭据生成、client 打包、Windows 启动/停止脚本、文档、示例配置、测试）。未触碰任何现有文件（Cargo/src/现有脚本与测试零改动），未提交 git。

## 交付物（全部为新建）

| 文件 | 说明 |
|---|---|
| `scripts/remote-credentials.py` | Python 标准库 + openssl CLI：私有 CA（CA:true/keyCertSign）+ 服务器叶证书（DNS SAN + EKU serverAuth）+ ≥32 字节随机 Token。`--output-dir` 必须不存在且非 symlink；staging+rename，失败不留半成品；私钥/Token 0600，输出目录 0700；只打印路径与 CA 指纹，绝不打印 Token/私钥内容；不碰任何信任库。 |
| `scripts/package-remote-client.sh` | 打包 `bin/computer-client` + 严格 MCP 配置（python3 生成 JSON，无 sed 插值）+ acceptance prompts + `RELEASE.json`（只含 sha256，不含秘密内容）+ `MANIFEST.sha256`。release 目录必须全新；输入必须为普通文件（拒绝 symlink）；凭据**只引用外部路径、绝不复制进包**；包内零 .rs/Cargo/.git/.token/.key/.pem、零 symlink（打包前自检）。 |
| `scripts/windows-remote-start.ps1` | WTS 活动控制台会话 + 该会话 explorer.exe 属主为计划任务 principal（Interactive/Limited）；任务动作**直接执行 exe + 参数**（含 `--remote-listen/--tls-cert/--tls-key/--token-file/--log-file`，无 cmd 包装）；对 `host.token`、`tls\server.key` **显式设置** ACL 为仅 {控制台用户 SID, S-1-5-18, S-1-5-32-544} 并回读核验；`-ListenAddress` 必填且拒绝 0.0.0.0/::；拒绝覆盖已存在任务/实例（终态残留仅删记录）；启动后核验指定地址 LISTEN 归属本 PID（有界等待），失败保留记录供清理；身份记录含 PID/会话/exe/创建时间 UTC/任务参数/principal。无防火墙命令、无 WMI Start-Process、无 RPAD。 |
| `scripts/windows-remote-stop.ps1` | 读实例记录，先核验**全部**身份（进程：Name/ExecutablePath/SessionId/CreationDate±2s 防 PID 复用；任务：单 action 的 Execute/Arguments/principal 全等），任一不符则**零变更**退出 1；只 Stop-ScheduledTask/有界等待/最后手段 Stop-Process 该 PID/Unregister 该任务/删记录文件，其余一律不动。 |
| `docs/remote-connection.md` | 简明中文：两台机器完整配方（生成凭据→查 LAN 地址→部署→启动→打包→MCP 配置→诊断）、`openssl s_client -verify_return_error -verify_hostname` 诊断法、手动轮换说明、明确"非 HTTPS/非 Streamable HTTP、仅 TLS 承载 JSON-RPC"、"不自动重连/重放"、不宣称 GUI 验收通过。 |
| `examples/mcp_remote_client.json` | 远程 client 的严格 MCP 配置模板（绝对路径占位 + 外部凭据路径）。 |
| `tests/remote_packaging.py` | 17 个用例（见下）。 |

## 测试（`python3 tests/remote_packaging.py`，stdlib only，全部有界超时）

结果：**17 ran, OK（1 skipped）**。skipped = pwsh 解析检查（本机无 PowerShell；已做替代静态检查：必备原语存在 + 禁带命令仅允许出现在注释中）。

覆盖：
- **凭据**：布局/0700/0600 权限断言；openssl verify 链 OK；叶证书 SAN/EKU serverAuth/CA:FALSE，CA 证书 CA:TRUE/keyCertSign；错误 CA 验签失败；**真实 s_server/s_client 握手**：正确 hostname 通过、错误 hostname 拒绝；拒绝已存在目录、拒绝 symlink 输出目录（victim 目录零改动）；失败路径不留任何产物；12 种非法 server-name（含通配、空白、斜杠、换行、超长、下划线）全部拒绝且不建目录；Token/私钥内容不出现在 stdout/stderr。
- **打包**：release 内容精确（bin/mcp.json/tasks/RELEASE.json/MANIFEST）；mcp.json 反序列化后参数逐项断言（`--server-name` 显式传递，`computer` 服务器名与 run-blackbox-claude.sh 一致）；`shasum -c` 校验通过；包内无 symlink/源码/凭据后缀文件；RELEASE.json 与 mcp.json 不含 Token 内容（仅 sha256/路径）；拒绝已存在 release 目录；7 种非法 `--connect`、6 种非法 `--server-name`、symlink binary、缺参全部拒绝。
- **PowerShell**：仅静态检查，未远程执行、未触碰任何 Windows 机器。

测试中的 fake client binary 是显式标注 `TEST-ONLY` 的 shell 脚本（echo fake-client），仅用于打包管线验证。

## 与核心的接口（按固定 CLI 契约）

- host: `computer-host --remote-listen IP:PORT --tls-cert server.pem --tls-key server.key --token-file host.token --log-file host.log`
- client: `computer-client --connect HOST:PORT --ca-cert ca.pem --server-name NAME --token-file client.token`
- `--server-name` 在打包配置中**始终显式传递**；新二进制尚未存在不影响本侧车——`--binary` 接受任意路径，核心就绪后可直接复跑打包脚本独立验证。

## 覆盖缺口（如实报告）

1. **PowerShell 未在真实 Windows 上执行**（本环境无 Windows/pwsh）：任务注册、WTS 会话、Set-Acl、Get-NetTCPConnection 的实际行为需按设计文档验收第 5 条在 acer-win 上验证；pwsh 解析检查在有 PowerShell 的环境会自动启用。
2. `computer-client`/`computer-host` 的 remote 模式由核心 owner 实现，本侧车未做真实二进制的端到端 TLS 验证（openssl 握手测试只验证证书材料本身正确）。
3. `Get-NetTCPConnection` 在部分 Windows SKU 需要 NetTCPIP 模块可用；脚本对其失败做了 SilentlyContinue + 有界重试，最坏情况是如实报"未观测到 LISTEN"而非误判成功。
4. stop 脚本的 CreationDate 比较容忍 ±2s（WMI 时钟粒度），与现有 windows-stop-host.ps1 做法一致。
5. 防火墙拦截场景只如实报错，未验证（脚本本就不允许改防火墙）。
