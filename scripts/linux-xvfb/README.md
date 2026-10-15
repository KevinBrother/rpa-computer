# Linux Xvfb 隔离测试环境（第一阶段）

本目录提供 Docker runtime、可选离线 Rust builder、X11 会话和自有 Tk fixture。
**本文是部署说明，不是测试报告；未据此宣称构建、启动、GUI smoke 或模型黑盒验收通过。**
实际版本、运行日志和验收结果由协调者与独立 CC 另行补充。

## 范围和安全边界

- 运行位置：`acer-win` 的 WSL Ubuntu 24.04，systemd 管理现有 Docker；WSL VHD 位于 D 盘。
  环境文件部署于 `/opt/rpa-test/linux-xvfb-20261015/environment-v1`，产品源码独立置于
  `/opt/rpa-test/linux-xvfb-20261015/source`。
  本目录脚本不执行 SSH、不安装 Docker、不修改 WSL/Docker 全局配置。
- runtime 用户固定为 UID/GID **10001:10001**，不与 base 中可能存在的 `ubuntu:1000` 冲突。
- Xvfb 使用 `:99`、默认 `1280x720x24`、`-dpi 96`、`-nolisten tcp` 和每次启动随机生成的
  xauth cookie；不使用 `-ac` 或 `xhost +`。`SCREEN_GEOMETRY` 可在自行审查的部署命令中覆写，
  例如 `1920x1080x24`；当前脚本沿用审查建议的 1280×720。
- 会话固定 `DISPLAY=:99`、`XDG_SESSION_TYPE=x11`，清除 `WAYLAND_DISPLAY` / `WAYLAND_SOCKET`；
  Openbox 提供窗口管理和焦点支持。启动时枚举实际 connected output 并设置 primary，检查
  XTEST/RANDR 与 primary active monitor，设置 `Xft.dpi: 96`。
- x11vnc 只监听容器内 `127.0.0.1:5900`，不发布 VNC 端口；noVNC 容器端口 6080 仅发布到
  **Docker 宿主 `127.0.0.1:16080`**。VNC 无密码，依赖隔离与 SSH 隧道，**仅用于受信测试环境**。
  Docker 宿主和容器网络不是互不信任用户间的安全边界，勿接入不可信容器或配置公网转发。
- **noVNC 为服务端强制只读：不能点击、输入或人工接管桌面。** 输入只能由获授权的同会话 host
  或独立验证流程执行；不得为了方便观察移除只读限制。
- 不使用 privileged、host network、宿主 X/WSLg 挂载或 Docker socket 挂载。
  默认限制：4 CPU、4 GB 内存、4 GB memory+swap 总额、512 MB shm、256 PID；
  `cap-drop ALL`、`no-new-privileges`；json-file 日志每文件 10 MB、最多 3 个。
- 关键服务退出时 entrypoint 终止其余服务及其进程组并以失败状态退出；容器不自动重启。
  健康检查失败只标记 unhealthy，不会自动修复卡死进程。
- 本阶段**不默认启动或暴露产品 TLS 服务**。loopback/noVNC 是观察通道，不是产品 TLS 远程方案。

## 1. 构建 runtime

以下命令在 **WSL Linux shell** 执行。本目录文件的实际部署路径为：

```bash
ENV_DIR=/opt/rpa-test/linux-xvfb-20261015/environment-v1
cd "$ENV_DIR"
```

构建上下文使用上述 `environment-v1` 目录（包含本 README 与 Dockerfile），
不要把独立 `source` 目录或凭据作为上下文上传。

默认 base 为 `ubuntu:24.04`。协调者已报告远端拉取成功的 Canonical ECR base 可这样覆写：

```bash
docker build \
  --build-arg BASE_IMAGE=public.ecr.aws/ubuntu/ubuntu:24.04@sha256:5ce7043d3beb84e91bfeecacb5fafc80602bf247a1dd6016bc0fe7d69d707eca \
  -t rpa-linux-xvfb:local .
```

`BASE_IMAGE` 需提供兼容 Ubuntu 24.04 apt 包及用户管理命令的基础环境；不是任意发行版兼容承诺。
没有镜像构建时的 Rust 编译步骤，也不复制产品 host。apt 包未锁定版本，需保留实际构建日志与镜像 ID。

## 2. 生命周期

### 先启动 Windows keepalive，再恢复容器

协调者提供的 `.agents/runs/linux-xvfb-20261015/wsl-lifetime.log` 记录了多轮 WSL boot 与 Docker
停止；本次部署报告的容器退出为 exit143、非 OOM。为避免短命 `wsl.exe` 命令结束后 WSL
退出，本目录提供独立的 **Windows 计划任务**，不修改容器镜像、`.wslconfig` 或 Docker 配置。

将 `windows-keepalive.ps1` 放在 Windows 可访问的目录，在 **拥有 Ubuntu 分发且已登录的当前
Windows 用户**下执行（不是 SYSTEM，不传凭据）：

```powershell
.\windows-keepalive.ps1 start
.\windows-keepalive.ps1 status
```

固定任务名 `RpaLinuxXvfb-Keepalive-20261015`，动作为
`%SystemRoot%\System32\wsl.exe -d Ubuntu --exec /bin/sleep infinity`。
任务采用当前用户的 `Interactive / Limited`，无自动触发器、执行时间限制为 0、允许电池运行且
不因切换电池停止。存在同名任务时先核验 description 标记、唯一 action、principal 和设置；
不匹配则拒绝操作，绝不覆盖。`start` 请求成功或任务显示 Running 都不等于 Docker/GUI 验收通过。

随后在 WSL 中等待 Docker 可用，再执行：

```bash
cd /opt/rpa-test/linux-xvfb-20261015/environment-v1
./run.sh resume               # 已有且停止的容器；首次创建才使用 start
./run.sh status
```

日常暂停时先执行 WSL 的 `./run.sh stop` 保存容器，再在 Windows 执行
`.\windows-keepalive.ps1 stop`。不再需要任务时使用 `.\windows-keepalive.ps1 remove`；两者
只操作核验匹配的任务，不执行 `wsl --shutdown`、不重启、不终止其他 WSL 进程。
**没有开机/登录自动恢复保证**：注销、重启或任务异常退出后需手动重新 `start` 并确认环境状态。
2026-10-15 主模型已报告：review 后远端执行 `start/status` 成功，任务为 Running，
Interactive Session 1 中存在 action 进程。这是协调者提供的运行证据，不代表 GUI/host 验收通过。
keepalive 由 Windows 计划任务独立运行，**无需 Mac 持续连接或保持 SSH 会话**；
观察用 SSH 隧道需另外手动开启，keepalive 不会替你建立隧道。

### 容器操作

```bash
./run.sh start
./run.sh status
./run.sh logs
```

默认镜像 `rpa-linux-xvfb:local`，可通过 `IMAGE=... ./run.sh start` 选择同契约 runtime 镜像。
容器名固定 `rpa-linux-xvfb`，启动参数包括 `ENABLE_FIXTURE=1`。
`start` 只代表 Docker 创建/启动请求完成，**不代表 ready 或验收通过**；观察 `status` 和 `logs`。

存在同名容器（包括已停止的容器）时 `start` 一律报错，不替换、不删除。
`resume/status/logs/stop/remove/exec` 都要求容器带有本脚本设置的归属 label：
`org.datagrand.rpa.managed-by=linux-xvfb-run-v1`，随后使用已检查的容器 ID 操作。
主模型若先手工部署，要使用同一 label 才能交给本脚本管理；不要给来源不明容器补标以绕过保护。

日常暂停/恢复使用原容器，不删除已注入的 host：

```bash
./run.sh stop                 # 保留容器、注入的二进制与状态文件
./run.sh resume               # 启动已停止的同一容器，不重建、不重新注入 host
./run.sh status               # 查看重新启动后的健康状态
```

`resume` 只对归属匹配且状态为 `exited` 的容器 ID 执行 `docker start`；运行中明确提示无变更，
其他状态拒绝操作。恢复会重新启动桌面/fixture，并重置其会话状态，不是恢复原进程；通过 `exec`
启动的 host 需在桌面 ready 后重新执行 `./run.sh exec /opt/rpa-host/computer-host`。

仅在明确要丢弃旧容器并重新创建时使用：

```bash
./run.sh stop
./run.sh remove               # 删除已停止且归属匹配的容器；先保存产物/证据
./run.sh start                # 创建新容器，需要重新注入 host
```

`remove` 会丢失容器可写层中的文件，先保存所需证据；不支持强制删除，不执行 prune。
核心镜像使用 tini，不需额外 `--init`。脚本无需 Compose，不提供任意端口或宿主挂载透传。

## 3. Mac 只读观察

**需要查看 noVNC 时**，在 Mac 单独打开终端建立以下 SSH 隧道，并在观察期间保持连接：

```bash
ssh -N -L 16080:127.0.0.1:16080 acer-win
```

浏览器地址：

```text
http://127.0.0.1:16080/vnc.html?autoconnect=true&resize=scale&view_only=true
```

此命令假设 `acer-win` SSH 服务端的 loopback 能到达 WSL 发布的 16080。
若不通，由协调者检查 Windows/WSL loopback 可达性；本脚本不自动添加 portproxy、防火墙规则，
也不退回 `0.0.0.0` 发布。没有硬编码 SSH IP。浏览器缩放只影响观察，不应拿缩放后的页面坐标做输入。
结束观察时在隧道终端按 Ctrl-C；Windows keepalive 不依赖此隧道，关闭隧道或断开 Mac
不会主动停止 keepalive 或容器，只会关闭这条观察通道。

## 4. 注入 Linux host，以 stdio 运行

准备与容器架构、Ubuntu 24.04 运行库兼容的 Linux 二进制。下面由协调者执行，源路径按实际产物调整：

```bash
chmod 0755 /path/to/computer-host
docker cp /path/to/computer-host rpa-linux-xvfb:/opt/rpa-host/computer-host
./run.sh exec /opt/rpa-host/computer-host
```

`docker cp` 后文件可能由 root 拥有；源文件的 0755 权限使 UID10001 可执行，不需要让 host 以 root 运行。
不要复制凭据到镜像或把 host 的日志/协议流与构建输出混合。

`run.sh exec` 等价于检查归属后以 `docker exec -i --user 10001:10001` 调用
`/usr/local/bin/exec-session.sh`。不分配 TTY，保留 stdio 协议；包装脚本强制同用户、DISPLAY、
XAUTHORITY 和 X11 环境，并要求桌面 ready。host 的协议参数按实际产品契约提供。

通过 `docker exec` 后启动的 host **不属于 entrypoint 的受监督关键服务**；其退出由调用方检测和记录，
不会自动停止桌面。桌面关键服务退出则会终止容器及其中的 host。当前不默认开放任何产品网络端口。
后续若改用只读产物挂载或产品 TLS，由协调者另行审查部署，不挂载宿主显示或 Docker socket。

## 5. 自有 fixture 与 ground truth

启用后窗口请求尺寸/位置为 **960×620、+80+40**，窗口标题为
`RPA Linux X11 | Owned smoke fixture`。最终位置可能受窗口装饰影响，以 JSON 为准。
状态文件为 `/tmp/rpa-fixture-state.json`，通过同目录临时文件 + 原子替换写入，包含：

- `ready`、PID、UID、DISPLAY、启动/更新时间；辨别旧文件时同时检查时间和 PID。
- `button_clicks`、`entry_text`（**仅允许输入自有测试内容，不输入密码/业务数据**）。
- `wheel_events` 与 `wheel_total`：正值向上、负值向下；X11 Button4 为 +1、Button5 为 -1。
  两个方向抵消时累计值可为 0，事件数仍增加。
- `controls.entry/button/scroll_area` 的 root 像素 `x/y/width/height` 与 `center`；还有窗口矩形和焦点控件。

独立 CC 可通过 `./run.sh exec cat /tmp/rpa-fixture-state.json` 获取定位与结果；文件不向 VNC 以外的
网络服务发布。这是 **deterministic GUI smoke** 的 ground truth，不是模型黑盒的盲测材料。
记录证据时分开列明“确定性真实 GUI smoke”和“模型黑盒验收”，不能把前者计作后者通过。
fixture 未添加浏览器或自动输入 harness，关闭 fixture 会触发关键服务退出并停止容器。

## 6. 可选离线 builder（不是日常 GUI 镜像）

```bash
docker build -f Dockerfile.build \
  --build-arg RUNTIME_IMAGE=rpa-linux-xvfb:local \
  -t rpa-linux-xvfb-builder:local .
```

builder 构建阶段以 root apt 安装构建依赖及 `rustc-1.89` / `cargo-1.89`；不要使用 Ubuntu 默认的旧 rustc。
运行阶段仍为 UID/GID10001，`WORKDIR=/work`，`RUSTC=/usr/bin/rustc-1.89`，清空 ENTRYPOINT、禁用
继承的健康检查，默认命令为 `cargo-1.89 build --locked --offline`，**不会启动 Xvfb/WM/noVNC**。

协调者准备独立 `source`、vendor 与指向实际 vendor 路径的 `.cargo/config.toml`，不更改 Cargo.lock；
源码、Cargo home 和输出目录须满足 UID10001 的读写权限。可用隔离容器执行：

```bash
docker run --rm --network none --user 10001:10001 \
  --cpus 4 --memory 4g --memory-swap 4g --shm-size 512m --pids-limit 256 \
  --cap-drop ALL --security-opt no-new-privileges=true \
  --log-driver json-file --log-opt max-size=10m --log-opt max-file=3 \
  --mount type=bind,src=/opt/rpa-test/linux-xvfb-20261015/source,dst=/work \
  rpa-linux-xvfb-builder:local
```

此示例仅挂载独立构建源码；vendor 若在 source 之外，需由协调者补充专用只读挂载及正确配置。
构建会写 target，勿挂载原始工作区或依赖 root 绕过权限问题。需要 release 时显式追加
`cargo-1.89 build --locked --offline --release`。镜像构建的 apt 联网与容器运行时的离线 Cargo 是不同阶段。

## 验收边界

尚无本目录交付所证明的实际构建、服务健康、焦点、输入或截图结果。healthcheck 仅检查进程、X11
几何/DPI/扩展、primary monitor、WM 与 VNC/noVNC 可达性，不证明 native host 的截图/输入正确。
未覆盖 browser、多显示器、不同 DPI/scale、Wayland、真实桌面或完整 GUI 验收。
产品 TLS 远程链路、确定性 GUI smoke、模型黑盒分别保留独立证据门禁，由协调者/独立 CC 执行。
