# Linux Xvfb 环境实现独立只读审查

日期：2026-10-15；范围：`scripts/linux-xvfb/`。

## 快速结论

**未发现当前标准部署路径中足以要求中止 runtime 镜像构建的确定性严重阻塞。** 标准路径为 `run.sh start/resume` 启动桌面，随后以 `run.sh exec` 启动 stdio host。该结论是静态审查，不是构建/启动/GUI 验收通过。

发现一个 **P1 条件性错误用法**：直接将 stdio host 作为 entrypoint 附加命令。当前 run.sh/README 使用 exec 路径，已避开。其余主要是 fixture readiness/证据边界和容器停止清理顺序问题，详见下文。

用户报告 runtime 构建仍在下载 APT 包，本审查没有获取或替代远端构建结果。没有 SSH、启动 GUI、发输入、执行脚本/测试/编译、修改环境或产品代码、commit/push。唯一写入为本报告；未继续审计项目其他源码风险。

## 重要发现（按严重级别）

### P1，条件性：entrypoint 附加命令不适合 stdio MCP

- **位置**：`entrypoint.sh:44-52,115-119`。
- **触发条件**：使用 `docker run ... IMAGE /opt/rpa-host/computer-host` 等形式，把默认 stdio host 交给 `start_service host`，期望通过 docker run 的标准输入输出承载 MCP。
- **证据**：服务由非交互 Bash 的 `setsid "$@" &` 启动，没有显式 stdin 重定向/继承安排；该异步启动方式在关闭 job control 时把未重定向 stdin 置为 `/dev/null`。另外 supervisor 和各服务共用容器 stdout，`printf '[session]...'` / ready 日志会混入协议通道。
- **结果**：stdio host 无预期请求输入，可能读到 EOF 后退出；`wait -n` 把任一服务退出视为故障并停止桌面。即使另外保留了 stdin，混合 stdout 仍不符合 MCP framing。
- **当前标准路径不受影响**：`run.sh:108-112` 使用独立 `docker exec --interactive --user 10001:10001`，无 TTY，`exec-session.sh:16` 最终 exec；README:117-131 明确通过此路径启动。附加命令适用于自行监听的 remote host，而不是 stdio bridge。
- **处置**：不阻断当前镜像 build；保持 README/run.sh 的 exec 路径。未来若要支持附加 stdio 命令，需专门设计协议流，不只增加 `docker run -i`。

### P2：桌面 ready 不等于 fixture 已 ready；重启还可能留下旧 ground truth

- **位置**：`entrypoint.sh:17,112-118`；`healthcheck.sh:4-8`；`exec-session.sh:8-10`；`fixture.py:12,27,67-73,95-97,106-129`。
- **触发条件**：start/resume 后立即读取 fixture JSON 或启动依赖其几何的测试；fixture 尚未激活；上次异常退出留下旧 `/tmp/rpa-fixture-state.json`。
- **证据**：entrypoint 启动 fixture 后只检查 PID 存活，立即 touch ready；healthcheck 不检查 fixture JSON/映射状态；`ready=True` 在 Tk 200ms 回调中才设置。启动时只清理 runtime ready/services，不删除上一轮 fixture JSON；状态更新由 after_idle 异步执行。
- **结果**：桌面 ready 或 health 不足以证明拿到当前 fixture 的坐标/计数；可能拿到旧的 `ready=true`，或新状态 `ready=false`。README:140 已提示 PID/时间，但没有程序级门禁。
- **执行建议**：验收方额外等待当前服务 PID 对应的 JSON，核对 uid=10001、display=:99、ready=true、started_at 属于本次启动且控件尺寸有效；动作后等待相应事件/字段更新。不能仅检查文件存在或依赖一次固定 sleep。

### P2：停止是整体终止，不是 host 干净 release 的验证路径

- **位置**：`entrypoint.sh:20-38,83,101-116`；`run.sh:98-100`；README:130-131。
- **触发条件**：host 仍持有输入/会话时执行 stop；或关键服务失败触发 cleanup。
- **证据**：cleanup 对全部进程组同时发 TERM，顺序中 Xvfb 在前；5 秒后全部 KILL。它没有先停止/等待 host，再停止 X server 的阶段。通过 docker exec 后启动的 host 更不在 `pids` 中，代码和 README 已明确这一限制。
- **影响**：可以终止隔离桌面，但不能把容器停止成功当 host 的 `cleanup_outcome=released`。X server 先退出可能使 host 无法完成正常释放；exec host 需调用方另外确认退出。
- **执行建议**：正常 smoke 先调用产品 close、收集清理结果并等待 host 退出，再 stop 容器；故障注入单独记录 unknown/failed。当前 10 秒 docker stop 与脚本 5 秒清理等待没有明显预算倒置，但实际进程退出尚未验证。

### P2，验收边界：fixture 状态不是完整滚动/拖拽/按键清理 ground truth

- **位置**：`fixture.py:43-58,80-88,106-120`。
- `entry_text` 来自 Tk StringVar，`button_clicks` 来自按钮 command，能提供实际控件变化证据，不是根据请求参数伪造的成功值。
- `wheel_events/wheel_total` 证明收到滚轮事件，不证明视口实际移动；JSON 未记录 Text 的 yview。位于顶部收到向上滚动时可增加计数而没有内容位移，应结合截图/视口证据。
- 只绑定 Button4/5 和 MouseWheel；没有水平 Button6/7、拖拽轨迹、按键按下/释放或 held-state 测量。不能据此宣布横向滚动/拖拽/释放全部通过。
- `focus_widget` 仅在写状态时采样，未绑定 FocusOut，tick 也不周期写状态（`fixture.py:63,75-78,108`）；不是实时焦点心跳。
- `controls` 坐标为 `winfo_rootx/rooty` 返回的 root 坐标；不要直接作为缩小后 observation 的图像坐标使用。README:144 已正确标注 root 像素。

## 启动、安全和构建逐项核对

| 检查项 | 源码证据与结论 |
|---|---|
| 默认启动链 | Xvfb → X 扩展/primary/DPI → Openbox → VNC → noVNC → 可选 fixture → ready；有服务 PID 追踪、30 秒 readiness 循环、任一关键服务退出即停止。`entrypoint.sh:44-75,83-126`；未发现默认链的确定性语法/顺序阻塞，未执行语法检查。 |
| Xauth | 每次启动随机 16-byte cookie，文件 0600，Xvfb `-auth`、客户端共享 XAUTHORITY，无 `-ac`、无 xhost 放行。`entrypoint.sh:77-84`、`session-env.sh:3-5`。停止移除授权文件。 |
| X server 范围 | 固定 DISPLAY=:99，Xvfb `-nolisten tcp`；run.sh 不挂载宿主显示/socket。不存在本实现自动控制 WSLg/宿主桌面的路径。 |
| Wayland | session-env 主动 unset WAYLAND_DISPLAY/WAYLAND_SOCKET，而不是只置空，符合已知底层驱动约束。 |
| primary | 按实际 connected output 设置 primary，同时检查 xrandr query 和 active monitor 的星号，不硬编码 output 名。`entrypoint.sh:89-97`。 |
| DPI | Xvfb `-dpi 96` 加 `xrdb -merge Xft.dpi:96`，启动和 health 验证资源值，health 还验实际 X resolution。`entrypoint.sh:83,93-98`、`healthcheck.sh:13-17`。默认1280×720路径未发现静态尺寸冲突；任意3-4位几何的物理DPI舍入行为仍需实际验证。 |
| 非 root | Dockerfile 固定10001:10001，HOME/runtime目录预置权限；entrypoint/exec wrapper 验证UID/GID。不依赖容器capabilities注入输入。`Dockerfile:19-22,30-42`。 |
| runtime/build依赖 | runtime 有X11/Xtst/XCB及桌面/Tk工具包；builder 有C工具链和对应dev包。`Dockerfile:8-18`、`Dockerfile.build:5-16`。未发现此前已确认的native链接依赖遗漏；APT包可用性、rustc/cargo/rustdoc的实际路径和完整链接仍由当前构建确认。 |
| builder隔离 | 清空ENTRYPOINT、HEALTHCHECK NONE、UID10001、默认offline Cargo，无桌面启动。`Dockerfile.build:17-21`。 |
| 构建上下文 | Dockerfile COPY源文件相对本目录；README:33-48正确要求以scripts/linux-xvfb对应目录为context，`.dockerignore`仅放行实现文件，不打包业务源码/凭据。 |
| 产物/权限 | runtime不含host，README:117-128先chmod0755再docker cp；root拥有0755文件不妨碍10001执行。builder bind mount会遮住镜像内/work的ownership，源码/vendor可读、target可写仍需外部准备；README:163-176已说明，不能用镜像内chown代替宿主目录权限。 |
| 观察通道 | x11vnc服务端viewonly且loopback5900；websockify容器内0.0.0.0:6080，无认证；run.sh只发布宿主127.0.0.1:16080。不是产品TLS/认证服务；同Docker网络和本机可信用户仍属信任边界。 |

## run.sh 实际资源、ownership、resume 核对

- **资源参数**（`run.sh:59-71`）：4 CPU、4g内存、4g memory+swap总额（不额外分配swap）、512m shm、256 PID；cap-drop ALL、no-new-privileges、日志轮换10m×3、无自动重启、10秒stop timeout；与README一致。没有privileged/host network/socket mount或宿主显示挂载。
- **归属**（`:28-36`）：先按名称inspect并验证专用label，再用不可变container ID执行后续操作；没有检查失败后退回名称强行操作。label是避免误操作的约定，不是对有Docker权限攻击者的认证。
- **start**（`:52-72`）：已存在同名容器即拒绝；要求镜像role=runtime且Config.User=10001:10001，不误运行builder；不自动替换/删除。创建成功明确不等于ready。
- **stop/resume**（`:74-86,98-100`）：stop不remove；resume对同一exited容器ID执行docker start。可写层copied host保留；不会恢复原进程，后续exec host需重新启动。当前已包含用户要求的resume，无遗漏。
- **remove**（`:102-106`）：归属检查、非running检查、无force；不会主动删除运行中的容器。未发现prune或越界删除。
- **exec**（`:108-112`）：交互stdin开启、无TTY、用户固定，通过session wrapper清理环境；stdout与桌面daemon日志分离，适合stdio MCP。
- **操作注意而非缺陷**：start/resume不等待health；exec wrapper只检查ready文件和授权文件，可能立即拒绝尚在启动的桌面，也不会自动拒绝所有unhealthy状态。必须由调用方看status/logs并完成ready门禁，不能把docker run/start返回当启动成功。
- **恢复未验证**：非正常退出后持久层中可能残留X锁/socket或fixture JSON；脚本没有专门的异常残留恢复协议。正常stop/resume与SIGKILL/OOM后的resume应分开验证；不能为此盲删活动X锁。此项是恢复覆盖缺口，不宣称当前正常resume必然失败。
- README的Windows SSH loopback→WSL发布端口前提已显式说明（`:108-110`）；本审查没有SSH或验证该可达性，也不建议改成外网监听来绕过。

## 交接优先级

1. **继续当前runtime镜像构建，无已确认默认启动阻塞要求中止。** 保存实际APT/镜像构建结果。
2. 启动后先看primary/DPI/WM/VNC健康，再等待当前fixture JSON ready；不要只看start/resume退出0。
3. 用`run.sh exec`运行stdio host，不把stdio host追加到entrypoint；源码/vendor/target权限按UID10001准备。
4. 正常结束先关闭host会话并记录release，再stop；低优先级ground truth扩展和异常恢复单独安排，不扩大本次审查。

## 冻结实现指纹（SHA-256，读取时）

```text
919097333ff25a96e98d746be98eb015bdc84966c200657b241c00148e20008e  Dockerfile
46690aedef2d75ccb8b0db0ec7a18a8ad23a3ea200e0b2e26ea28fdd6a67157d  Dockerfile.build
48620499aa8d91d2511976a57c7ec599e8da870014c3de1451f6f1b64a7a9485  entrypoint.sh
a0f4604acbc6242b0b60ad81a9145e761521329729c117f2a63ffaadceb249dd  session-env.sh
c6907f8778731b1c190d3f44313a8f608c7bad6d65c541d4e4fac4360b2e8251  healthcheck.sh
4f3b0aa491ecc04ed723da5b457aef7e98f34082cf831520f06ad9e24efeecaa  exec-session.sh
3873ed1f3195f12a0d30cca9b7fd24aff1c7b58dd0e40e3a837b2572ff3739a1  fixture.py
```

run.sh/README为本轮新增的动态交付，按本次读取的含resume版本审查；本报告不冻结或修改其内容。
