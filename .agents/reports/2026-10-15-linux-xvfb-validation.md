# Linux/Xvfb 独立验证报告（环境 + 确定性原生 GUI smoke）

日期：2026-10-15（Asia/Shanghai）
验证者：独立 Claude Code 验证进程（未改产品/环境代码，未 commit/push）
验证者模型：**Claude Opus 5**（model id `claude-opus-5[1m]`，当前配置模型，未路由到其他模型）。
本报告是 **环境门禁 + 确定性真实 GUI smoke** 的验证结果，**不是**模型黑盒验收，也不声称覆盖下文"未测试场景"。

## 总结论

| 门禁 | 结果 |
|---|---|
| Gate 1 运行时容器隔离/资源/X 环境 | **PASS**（28 项检查全过；含一次由基础设施事件引起的重跑，见"过程事件"） |
| Gate 2 host 二进制（ELF/版本/ldd/完整性） | **PASS**（5 项检查全过） |
| Gate 3 协议闭环（initialize→tools/list→describe→open→observe） | **PASS** |
| Gate 4 原生输入 ground-truth（click/entry/type/scroll/close/EOF） | **PASS**（41 项检查 41/41，fixture 白盒断言全部精确成立） |
| Gate 5 证据留存 | **PASS**（结构化 evidence.json + 6 张 PNG，仅存文件，未打印任何图像 blob） |
| Gate 6（可选）锁释放 | **PASS**（第二个 host 在第一个退出后成功获取 `/tmp/computer-host-10001.lock`） |

**确定性真实 GUI smoke：通过。** 真实 Linux 原生后端（无 `--mock-backend`）在专属 Xvfb 容器内完成
click / 聚焦 / 原生文本输入 / 滚轮 / close / EOF 退出，且每一步都以 fixture 状态文件（白盒 oracle）
验证**应用实际收到并处理了输入**，而非仅看 `dispatched`。

## 执行环境与路径

- 远程执行：`python3 .agents/runs/linux-xvfb-20261015/remote_preflight.py <本地脚本>`（唯一远程通道）。
  注：任务开始时该 helper 曾因 Windows cmd.exe argv 长度限制（约 8191 字符）无法传输 >2KB 脚本
  （首次 gate1 7KB 脚本报 "The command line is too long."，退出码 1，已保留）；协调者随后将其改为
  SCP 传输 + 短命令调用，此后所有脚本正常运行。
- 容器：`rpa-linux-xvfb`（image `rpa-linux-xvfb:local`，digest
  `sha256:a45492b4559e06119798fa68310188a2cf451e2b2b18317e0e7e96790d702c8d`），DISPLAY=:99，UID/GID
  10001:10001，XAUTHORITY=/home/rpa/.Xauthority，fixture 启用。
- host：`/opt/rpa-host/computer-host`，sha256
  `b798254dbbf852d64737c78dee0724008b16f6c4aa1931998af247c6109aa3bd`（与协调者 `host-built.json` 一致）。

## Gate 1：运行时容器（命令与结果）

命令（每条均经 remote_preflight.py 在 WSL root 执行，exit code 见括号）：

- `gate1a-inspect.sh`：`docker container inspect --format` 逐项断言（exit 0）
  - running=true；health=**healthy**；Config.User=10001:10001；NanoCpus=4000000000（=4 CPU）；
    Memory=4294967296（≤4GiB）；MemorySwap=4294967296（≤4GiB）；Privileged=false；
    NetworkMode=bridge（非 host）；ShmSize=536870912；PidsLimit=256；RestartPolicy=no；
    LogConfig json-file max-size=10m max-file=3。
- `gate1b-security.sh`（exit 0）：CapDrop 含 ALL；SecurityOpt=[no-new-privileges=true]；
  Binds/Mounts 为空（**无 Docker socket、无宿主 X 挂载**）；PortBindings 仅
  `127.0.0.1:16080 -> 6080/tcp`（`docker port` 交叉确认）；Config.Env 无 WAYLAND_*。
- `gate1c-exec.sh`（exit 0）：
  - `docker top -o pid,uid,gid,comm,args`：tini/entrypoint/Xvfb/openbox/x11vnc/websockify/fixture
    全部 uid=gid=10001（以非 root 运行）。
  - 显式 `docker exec --user 10001:10001 rpa-linux-xvfb /usr/local/bin/healthcheck.sh` → exit 0。
  - `xrandr --query`：`screen connected primary 1280x720+0+0`；`--listactivemonitors`：`0: +*screen`。
  - `xdpyinfo -display :99 -queryExtensions`：XTEST、RANDR 均在；dimensions 1280x720；96x96 dpi。
  - `xrdb -query`：`Xft.dpi: 96`（scale=1，规避了代码审查 P1-2 的非 1.0 缩放缺陷）。
  - `/tmp/rpa-fixture-state.json` 可读：ready=true、uid=10001。

## Gate 2：host 二进制（exit 0）

- sha256 与 host-built.json 完全一致。
- ELF 头（容器内 python3 读前 20 字节）：`\x7fELF`、class 2（64 位）、data 1（LSB）、machine
  0x3e（x86-64）→ **ELF Linux x86_64**。
  （过程更正：首次检查漏加 `docker exec -i`，python 读到空 stdin、无输出却按 exit 0 判过——该次为
  **无效通过**，已用 `-i` 重跑得到上述真实结果；此处如实记录。）
- `--version` → `computer-host 0.1.0`，exit 0。
- `ldd` → libxcb/libX11/libXtst/libXext/libXau/libXdmcp/libbsd/libmd/libgcc_s/libm/libc 全部解析，
  **无 "not found"**。
- 启动 stderr 出现 `WARNING: emergency stop hotkey unavailable ... only for macOS and Windows
  builds` —— 按任务定义视为预期告警，不计失败。

## Gate 3+4：有界原生 GUI smoke（单进程探针，41/41 PASS，实际耗时 2.49s）

执行方式：`docker exec -i --user 10001:10001 rpa-linux-xvfb /usr/local/bin/exec-session.sh python3 -`
（探针脚本经 stdin 注入，未在远程落盘）；探针以 stdin/stdout 管道拉起
`exec-session.sh /opt/rpa-host/computer-host`，newline-JSON-RPC 2.0；单 RPC 超时 ≤20s、总预算 180s
（均未触发）。探针脚本：`cc-validation/gate34-probe.sh`（本地）。

协议链（全部真实返回，无 mock）：

1. `initialize`（protocolVersion 2024-11-05）→ serverInfo `computer-host 0.1.0`。
2. `notifications/initialized` → `tools/list` → **8 个工具**（describe/open/observe/step/get_step/
   pause/resume/close；数量取自运行时契约，非硬编码）。
3. `computer_describe` → platform=**linux**，isError=false。
4. `computer_open {primary, max_width 1280, max_height 720}` → `session-1456-1`，state=ready，
   surface_id=linux:60。
5. `computer_observe` → 120131 B PNG，**IHDR 1280×720 与 meta width_px/height_px 完全一致**；
   解码后 fixture 窗口矩形 [81,60,1041,680] 内 1537 种颜色、主导色占比 0.5162（真实渲染内容，
   非空白截图）；root 1280×720 与图像 1:1（scale=1）。

原生输入（每步 `based_on` 取最新 observation_id，唯一 request_id，全部
input_outcome=dispatched / observation_outcome=available / cleanup_outcome=not_needed，每步返回的
新 PNG IHDR 均与自身元数据一致）：

| 操作 | 图像坐标 | fixture ground-truth 断言 |
|---|---|---|
| click 按钮（count=1） | (189,293) | `button_clicks` 0 → **1**（恰好 +1） |
| click 输入框 | (561,231) | `focus_widget` = `.!frame.!entry`，文本不变 |
| text_input `abc123` | — | `entry_text` `""` → **`abc123`**（精确增量） |
| scroll delta_y=+3（wheel_ticks） | (561,491) | `wheel_events` 0 → **3**，`wheel_total` 0 → **-3**（Button5×3，与"正=向下"工具约定及 fixture "负=向下"约定一致） |

6. 显式 `computer_observe`（session-1456-1-obs-6，123021 B）→ `computer_close` → state=**closed**，
   cleanup_outcome=**released**。
7. 关闭 stdin → host **EOF 干净退出，exit code 0**。
8. fixture 保持运行可见（未触碰其它应用；noVNC 仍为服务端只读）。

## Gate 5：证据留存

- 容器内 `/home/rpa/cc-validation/`：`evidence.json`（14 KB，含事件流、fixture 前后快照、stderr、
  汇总 41 pass / 0 fail）+ 6 张 PNG（observe-1 / step-click_button / step-click_entry /
  step-type_abc123 / step-scroll_down_3 / observe-final，各约 120 KB）。
- 已 `docker cp` 到 Windows 侧 `D:\rpa-test\linux-xvfb\cc-validation-evidence\` 供人工查看。
- 本地 `.agents/runs/linux-xvfb-20261015/cc-validation/`：全部探针脚本与各门禁输出日志
  （gate1/gate2/gate34/gate6-output.log）。
- 全程未打印任何图像 base64/blob、auth cookie 或密钥。

## Gate 6（可选）：锁释放

在第一个 host 已退出后（**不存在双 host 输入**）启动第二个真实 host，仅
initialize → tools/list → describe（无 open、无任何输入），EOF 后 exit 0，stderr 首行
`acquired desktop writer lock at /tmp/computer-host-10001.lock` → 前一持有者退出后锁可被新 host
获取。未做双活 host 抢锁（lease_conflict）注入，属后续专项。

## 过程事件（如实保留，不作为产品失败）

1. **WSL 自动关机事件**：任务开始时容器为 RUNNING+healthy；16:48:13（UTC 08:48:13）容器收到
   SIGTERM（exit 143），随后 WSL systemd 整体关机（journalctl 证据在
   `wsl-lifetime.log`）。当时 gate1a 首跑因此报 running=false / unhealthy（该次结果已保留在
   `cc-validation/gate1-output.log` 注记中）。协调者确认为基础设施事件（无 Windows 侧 WSL 进程
   时 WSL 自动关闭），已恢复同一容器（resume，二进制保留），并持有 WSL 存活连接。恢复后 gate1
   全部重跑通过，以上记录的即重跑结果。
2. **传输通道长度限制**：remote_preflight.py 最初把脚本 base64 进 Windows argv，超过 cmd.exe
   限制；协调者改为 SCP 后解决。期间产生一次 "command line is too long"（exit 1）与一次
   gate1a 模板笔误（`{{State.Running}}` 缺 `.`，10 项误报 FAIL），修正后重跑。
3. **gate2 ELF 首检无效通过**：见 Gate 2 一节，已更正重跑。

## 未测试场景（pending，不因本次通过而视为已覆盖）

- 完整 Linux 发行版桌面、真实物理显示、多显示器、HiDPI / 非 1.0 缩放（代码审查 P1-2 指出的
  定位缺陷未在本环境触发，也未验证修复）。
- Wayland / XWayland 会话（后端主动拒绝，未测）；X server 卡死/崩溃恢复、取消 during hold、
  容器 SIGTERM 清理语义（审查 P2-2）。
- Linux 全局急停热键（明确未实现，仅告警——本报告按预期处理）。
- TLS 远程链路（supervisor/broker/executor、端口映射、client pin、quarantine 恢复）。
- 双活 host lease_conflict、多容器共享桌面锁作用域（审查 P1-3）、拖拽/key_chord/key_hold、
  不支持字符的负例、pause/resume 闭环。
- **模型黑盒验收**（由模型自主驱动工具）——本报告仅覆盖确定性 smoke，二者不得互相替代计数。

## 结论

在限定为"单容器 / 单 Xvfb(:99, 1280×720, 96dpi, scale=1) / 单 host / 自有 fixture"的范围内，
Linux 原生后端的截图（真实 PNG、尺寸契约一致）与 XTEST 输入（click / text / wheel）经白盒
ground-truth 验证全部成立，会话生命周期与 EOF 清理干净，容器隔离与资源限制符合部署契约。
上述范围之外的结论请以"未测试"为准。
