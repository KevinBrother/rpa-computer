# Acer Linux / Xvfb 测试环境实况

部署日期：2026-10-15（Asia/Shanghai）。这是本次实机部署记录；通用脚本见 `scripts/linux-xvfb/README.md`。

## 当前组成

- Windows：acer-win，Windows 11 Pro，Ryzen 7 5800H / 约16GB内存。
- 复用现有 WSL Ubuntu 24.04.4，虚拟磁盘原本位于 `D:\wsl\Ubuntu`；未安装 Docker Desktop、启用完整 Hyper-V或修改 `.wslconfig`。
- 复用 WSL Docker Engine 29.1.3；本次新增 runtime/builder 镜像、专属容器和手动启动的 Windows 保活任务。
- 容器：`rpa-linux-xvfb`，UID/GID10001，Xvfb `:99`、1280×720×24、96 DPI、primary `screen`（启动动态发现）、Openbox、Tk测试窗口、只读x11vnc/noVNC。
- 资源限制：4 CPU、4GiB内存、无额外swap、512MiB shm、256 PID；cap-drop ALL、no-new-privileges、日志10MB×3；无宿主显示/Docker socket挂载，无privileged/host network。
- 仅观察端口发布到 WSL loopback `127.0.0.1:16080`；Mac 经 SSH 隧道访问。没有发布产品TLS端口。
- host/client ELF位于容器 `/opt/rpa-host/computer-host`、`/opt/rpa-host/computer-client`。host **按需通过stdio启动**，不是已配置的常驻TLS服务。

## 位置

| 内容 | 位置 |
| --- | --- |
| Windows传输/保活脚本/证据 | `D:\rpa-test\linux-xvfb` |
| WSL环境文件 | `/opt/rpa-test/linux-xvfb-20261015/environment-v1` |
| 独立源码与vendor/构建输出 | `/opt/rpa-test/linux-xvfb-20261015/source` |
| 镜像与host构建日志 | `/opt/rpa-test/linux-xvfb-20261015/logs` |
| 容器GUI证据 | `/home/rpa/cc-validation` |
| Windows GUI证据副本 | `D:\rpa-test\linux-xvfb\cc-validation-evidence` |
| 本地原始记录 | `.agents/runs/linux-xvfb-20261015/`（git忽略） |

这些Linux目录位于现有D盘WSL虚拟磁盘内，不是C盘的新安装。

## 查看桌面

本次已建立Mac loopback SSH隧道，并打开noVNC浏览器标签页。地址：

```text
http://127.0.0.1:16080/vnc.html?autoconnect=true&resize=scale&view_only=true
```

后续重新连接，在Mac终端运行（保持终端打开）：

```bash
ssh -N -T -o ExitOnForwardFailure=yes \
  -L 127.0.0.1:16080:127.0.0.1:16080 acer-win
```

如果16080已被本次隧道占用，不要重复创建。当前协调者创建的隧道控制socket是
`/tmp/rpa-linux-xvfb-20261015.sock`，可检查或仅关闭这个隧道：

```bash
ssh -S /tmp/rpa-linux-xvfb-20261015.sock -O check acer-win
ssh -S /tmp/rpa-linux-xvfb-20261015.sock -O exit acer-win
```

noVNC是**服务端只读**，用于观察而非人工输入；真实自动化输入走Linux host。关掉查看隧道不停止容器。

## Windows保活与容器生命周期

实测发现：只运行短命 `wsl.exe` 命令时，WSL会在命令结束后关闭，systemd/Docker服务不能独自保活。
初次容器在16:48:13退出143，journal记录Docker/WSL正常关停；不是host崩溃或OOM。

已创建并启动专属计划任务 `RpaLinuxXvfb-Keepalive-20261015`，当前Windows用户
`Interactive / Limited`，动作 `wsl.exe -d Ubuntu --exec /bin/sleep infinity`。
无自动触发器、不保存密码、不以SYSTEM运行、不修改全局WSL配置；Mac断开后不依赖SSH保活。
**Windows注销、重启、任务异常退出后需要手动恢复，不宣称开机自启已配置或验证。**

在Acer当前用户的PowerShell中：

```powershell
# 先启动保活，再恢复已有容器。首次创建才用 run.sh start。
powershell -NoProfile -File D:\rpa-test\linux-xvfb\windows-keepalive.ps1 start
wsl.exe -d Ubuntu -u root --exec /bin/bash /opt/rpa-test/linux-xvfb-20261015/environment-v1/run.sh resume
wsl.exe -d Ubuntu -u root --exec /bin/bash /opt/rpa-test/linux-xvfb-20261015/environment-v1/run.sh status

# 查看保活任务
powershell -NoProfile -File D:\rpa-test\linux-xvfb\windows-keepalive.ps1 status
```

WSL中的日常操作：

```bash
cd /opt/rpa-test/linux-xvfb-20261015/environment-v1
./run.sh status
./run.sh logs
./run.sh exec /opt/rpa-host/computer-host --describe
# 真正stdio MCP服务，不分配TTY：
./run.sh exec /opt/rpa-host/computer-host
```

当前默认WSL用户未配置无密码sudo；上述Docker管理命令可通过Windows `wsl -u root` 运行，
**容器内host仍固定为10001，不以root运行**。没有为用户添加docker组或修改sudo权限。

暂停时先关闭host会话，再运行 `run.sh stop`，最后运行Windows保活脚本 `stop`。
恢复使用 `resume`，保留已复制的host；不要为重启随意remove容器。`remove`是显式删除，
会丢失容器可写层中的二进制/证据，脚本拒绝强删正在运行的容器。

## 构建与源版本

源码基线 `cee786526f1b3a5011004368b4d2f3480e3cbf7e`；本次未修改产品Rust源码/Cargo.lock。
本机vendor后传输，Linux builder以非root、无网络方式执行：

```bash
cargo-1.89 build --locked --offline --release --bins -j 2
```

实际rustc/cargo 1.89；完整release编译退出0。存在Linux未使用`Ordering`导入告警及
`screenshots 0.8.10`未来Rust兼容告警，不把告警当构建失败，也未自动修补依赖。

- host SHA256：`b798254dbbf852d64737c78dee0724008b16f6c4aa1931998af247c6109aa3bd`
- client SHA256：`3938fffed7101f055fe1017d3c0edfda696e35c582bb7157aaba3ceb67bebecf`
- runtime image：`sha256:a45492b4559e06119798fa68310188a2cf451e2b2b18317e0e7e96790d702c8d`
- builder image：`sha256:964457024ba47185061abba93f1004c58c2442478f86811d05e299ffa2a5d819`

Docker Hub/Rust官方站从Acer直连超时，使用成功拉取的Canonical公共ECR Ubuntu镜像，
固定base digest `sha256:5ce7043d3beb84e91bfeecacb5fafc80602bf247a1dd6016bc0fe7d69d707eca`；
APT仍使用Ubuntu发行版源。没有关闭TLS校验或修改全局代理。

## 本轮验收与边界

独立Claude Code执行确定性真实GUI smoke，当前配置实际报告模型 `claude-opus-5[1m]`，
未将其标作旧项目记录中的GLM。使用真实Linux host，无mock，无xdotool/Tk伪造输入。

核心GUI probe **41 PASS / 0 FAIL，退出0**：

- MCP initialize、describe、open、observe；原生PNG尺寸1280×720，图像含实际fixture。
- host点击按钮后 `button_clicks` 从0到1。
- host聚焦输入框并输入 `abc123`，fixture内容精确匹配。
- host发送向下3次滚轮，fixture事件数3、累计-3；保留前后PNG。
- close cleanup=`released`，stdin EOF后host退出0。
- 独立追加检查证明后继host能重新获取writer lock；不等同于双writer竞争测试。

证据：`.agents/runs/linux-xvfb-20261015/native-evidence/evidence.json`、原生PNG、
`cc-validation.jsonl`与从真实tool result提取的`cc-actual-tool-results.json`。

初次验证包含Windows命令行长度/测试探针模板/WSL生命周期问题，原始失败保留，修复后重新验证；
不能把早期失败日志删除或将手写输出摘要当唯一执行证据。

代码审查发现的产品风险尚未修复：非1.0 Xft DPI下输入坐标可能错误；writer lock作用域不适合
跨容器共享同一个X server；Wayland环境标记空值判定不一致等。当前环境以独占Xvfb、单屏、96DPI规避
这些已知条件，**不代表问题已修复**。Linux全局紧急热键不支持、文本输入有keymap限制。

未覆盖：Wayland、HiDPI、多屏、中文完整输入、拖拽专项、浏览器/复杂桌面应用、产品TLS直连、
模型黑盒任务、故障注入/断线恢复、冷启动或Windows重启恢复；未执行完整Rust测试套件。

静态报告：
- `.agents/reports/2026-10-15-linux-xvfb-code-audit.md`
- `.agents/reports/2026-10-15-linux-xvfb-environment-review.md`
- 独立实测报告：`.agents/reports/2026-10-15-linux-xvfb-validation.md`

没有commit/push、worktree或Windows重启。只增加本次环境与文档，不改已有Windows测试脚本/配置。

## 收尾核对

最终快照：容器running/healthy，StartedAt仍为2026-10-15T08:59:46Z、RestartCount=0；
container内存采样约88MiB（不含WSL/Docker宿主开销），fixture为clicks=1、text=abc123、wheel=-3。
本地取回的evidence.json及最终PNG SHA256与容器原件匹配。
验证器结束时曾越出任务写入范围，向其项目记忆文件追加经验；协调者按执行前原文与精确追加内容核验后撤销了该次追加，未改用户原有内容。纠正记录在忽略目录中的validator-scope-correction.txt。
