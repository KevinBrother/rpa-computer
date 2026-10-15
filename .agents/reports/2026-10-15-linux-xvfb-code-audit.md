# Linux + Docker/Xvfb 只读代码审查

日期：2026-10-15（Asia/Shanghai）  
仓库：`/Volumes/doc/workspace/datagrand/rpa/rpa-computer`  
读取时 HEAD：`cee786526f1b3a5011004368b4d2f3480e3cbf7e`

## 结论与证据边界

**存在真实 Linux/X11 后端，不是 mock，也不是“Linux 全部不可用”。** 单容器、单 Xvfb、primary 已设置、scale=1、XTEST/RANDR 可用的环境有对应实现路径，适合继续做受控 smoke；本报告不声称它已经构建或运行通过。

- 已读 `../AGENTS.md`、`.agents/README.md`、`.agents/CONTRACT.md` 以及下述实现和本机缓存的锁定依赖源码。
- 按本次最新授权只审查代码；旧文档中的 Windows-only 测试顺序不被解读为由本审查者启动 Linux 测试的授权。
- **没有 SSH、启动 GUI、发送输入、执行测试、执行编译、commit/push、修改产品代码或递归派单。** 唯一写入为本报告。读取时另有未跟踪的 `docs/superpowers/plans/2026-10-15-linux-xvfb-environment.md`，未修改。
- “已实现”只表示可追踪到源码；“静态确定”表示满足所列触发条件时由代码直接推出；实际 Xvfb 默认状态、动态链接、应用收到输入及清理效果均未验证。
- 下文依赖路径 `REG/` 指本机 `/Users/caojunjie/.cargo/registry/src/rsproxy.cn-e3de039b2554c837/`。版本以根 `Cargo.lock` 为准：`display-info 0.4.8`、`screenshots 0.8.10`、`xcb 1.7.1`、`libwayshot 0.2.0`、`libdbus-sys 0.2.7`。外部资料检索未返回可用正文；结论依据本地版本化源码，不以在线最新版本代替。

## 按严重级别的具体阻塞/风险

本次未发现可确认的 P0。P1 指条件满足后阻止启动或破坏输入/互斥正确性；P2 指部署条件、异常恢复或资源边界问题。不是所有条目都会在最小 smoke 中触发。

### P1-1：存在屏幕但无 primary，或 RANDR monitor 枚举不满足要求，真实后端无法启动

- **位置**：`src/backend/capture.rs:63-69`；`src/backend/desktop.rs:49-61`；`REG/display-info-0.4.8/src/linux.rs:125-160`；`REG/xcb-1.7.1/xml/randr.xml:964-982`。
- **触发**：Xvfb 可连接，但返回的 monitor 均非 primary；或没有 RANDR 1.5 `GetMonitors` 支持/活动 monitor；或某 monitor 没有 output。
- **证据**：依赖直接发 `GetMonitors(get_active=true)`，要求第一个 output；root 后端严格 `.find(is_primary)`，无“唯一屏幕即 primary”的回退。`DesktopBackend::new()` 在创建输入驱动前调用此查询。
- **结果**：`no_display` / “no primary display found” / 枚举失败；普通 host 初始化退出码 4（`src/bin/computer-host.rs:342-351`）。因此仅 `DISPLAY` 可连或 `xdpyinfo` 成功不足以证明可启动。
- **最小规避**：验收者读取 `xrandr --query`、`--listmonitors`、`--listactivemonitors`，必要时对实际枚举到的 output 设置 primary。不能硬编码 output 名为 `screen`，不能未经运行就断言所有 Xvfb 默认都缺 primary。

### P1-2：非 1.0 Xft DPI 缩放下，截图可通过尺寸检查但鼠标坐标错误

- **位置**：`REG/display-info-0.4.8/src/linux.rs:26-29,63-88,135`；`REG/screenshots-0.8.10/src/linux/xorg.rs:126-132`；`src/backend/capture.rs:95-109`；`src/backend/linux.rs:78-87`；`src/runtime/image.rs:129-136`；`crates/native-input/src/linux_x11.rs:384-394`。
- **触发**：依赖从根窗口 RESOURCE_MANAGER 成功读到 `Xft.dpi:\t192` 等非 96 DPI，得到 `scale_factor != 1`。
- **证据链**：display-info 将 monitor 的原点及尺寸 **除以 scale**；screenshots 截图时再 **乘回 scale**；root 却把缩小后的 `info.x/y/width/height` 直接当输入单位。CoordMap 映射到这些单位后，XTest 原样按 root pixels 移动。
- **静态示例**：root 为 1280×720、scale=2 时，输入几何变为 640×360，而 PNG 仍为 1280×720。图像点 `(640,360)` 被映射到 `(320,180)`；预期截图尺寸检查仍可通过。`src/backend/linux.rs:14-21` 关于 hard check 可兜住 exotic scale 的注释不能覆盖此错误。
- **额外情况**：root 的 `round()` 与截图依赖浮点乘积转整数的截断不同（`capture.rs:95-99` / `xorg.rs:129-130`），某些分数缩放还会直接触发 capture dimensions mismatch。
- **最小规避**：smoke 固定 scale=1，检查实际 `xrdb -query` 中的 Xft DPI；仅配置 Xvfb `-dpi 96` 不替代检查应用后续写入的 RESOURCE_MANAGER。真正修复需统一 Linux 输入原点/尺寸为 root pixels，本报告未改代码。

### P1-3：writer lock 不以 X display 为作用域，容器共享桌面时可失去互斥

- **位置**：`src/mcp/lock.rs:72-94,178-180,219-230`；`src/bin/computer-host.rs:303-327`。
- **触发 A**：两个容器共享同一个 X server/socket，但 `/tmp`/`TMPDIR` 不同；或不同 UID 都被授权访问同一 X server。
- **证据**：锁路径为 `std::env::temp_dir()/computer-host-<getuid>.lock`，锁住该 inode；没有 X server identity、DISPLAY 或跨容器统一锁服务。不同目录/UID 会得到不同锁，不能保证同桌面只有一个 writer。
- **触发 B（反向限制）**：同 UID、同临时目录启动两个 host，各自使用独立 Xvfb，仍会争同一锁，第二个退出 `lease_conflict`，虽然桌面不同。
- **最小规避**：一个容器独占一个 Xvfb 和一个真实 host；不要把宿主真实桌面的 X socket 共享给多个控制容器。确需共享时统一 UID、锁文件存储和路径，并另行协调所有 writer。不同 UID 的完整安全方案不应仅靠挂载同一个 `/tmp`。
- **已有保护仍有效**：同路径同 inode 的进程间 `flock`、进程生命周期持锁、无 unlink race 是已实现的；这里不是“Linux 没有会话锁”。

### P2-1：空 `WAYLAND_DISPLAY` 也会被底层驱动拒绝，与后端判定不一致

- **位置**：`src/backend/linux.rs:32-37,49-66,115-121`；`crates/native-input/src/linux_x11.rs:154-164`。
- **触发**：`DISPLAY` 指向有效 Xvfb、`XDG_SESSION_TYPE=x11`，但 `WAYLAND_DISPLAY` 被设为 `""` 而不是删除。
- **证据**：上层只把非空值视作 Wayland；驱动用 `var_os(...).is_some()`，空值仍命中 `unsupported_session`。上层单元测试包含空值通过，但不能代表完整驱动构造通过。
- **最小规避**：显式 **unset**，不是 `WAYLAND_DISPLAY=`。WSL/WSLg 环境中即使新设了 DISPLAY，也要清理继承的 Wayland 标记；只在目标确为独立 Xvfb 时这样做，不把此方法当作支持真实 Wayland/XWayland 的证明。

### P2-2：X 初始化无等待上限；原生 Xlib 调用异常也不保证可取消或干净清理

- **位置**：`src/feedback/worker_lifecycle.rs:36-66`；`src/mcp/worker.rs:629-630`；`crates/native-input/src/linux_x11.rs:17-28,168-183,213-219`；`src/bin/computer-host.rs:331-351,485-500`。
- **触发**：X server 接受连接但初始化/查询不返回；或运行中 X server 卡住、连接失效、发生异步 X 协议错误。
- **证据**：启动链两级 `recv()` 均无 timeout；信号 handler 只设 flag，启动等待不轮询该 flag。驱动 `XOpenDisplay` / `XSync` 无自身 deadline，且未安装错误处理器，模块明确记录默认 Xlib fatal handler 限制。
- **影响**：stdio host 可能尚未进入 transport 就一直等待；此时不能保证 SIGTERM 触发应用层干净退出。已启动阶段的取消也无法中断正在阻塞的 native call；Xlib 致命错误可能直接终止进程，绕过 Rust 正常清理。
- **已有保护**：运行阶段 transport timeout=20s（`src/mcp/worker.rs:292-298`），未知状态进入 fault/quarantine；remote executor 有退出等待和强制终止路径（`src/mcp/remote/executor.rs:13-18,293-350`）。这些是 containment，不是“释放所有输入已确认”。
- **建议**：由容器/主协调者提供外部超时、独立终止入口、异常后重建专属 Xvfb 的策略。不要用 `kill`/容器被杀代替 cleanup success 证据。

### P2-3：remote 监听/发现成功不等于桌面就绪，首次初始化失败会触发保守隔离

- **位置**：`src/mcp/remote/host.rs:26-33`；`src/mcp/remote/protocol.rs:52-60`；`src/mcp/remote/executor.rs:103-114,142-157`；`src/mcp/remote/broker.rs:573-629`。
- **触发**：TLS/token/端口可用，但首次 open 时子进程继承的 DISPLAY/XAUTHORITY/Wayland 环境错误，或 Xvfb 未 ready/无 primary。
- **证据**：supervisor 不构造桌面后端，未拥有 executor 时 discovery 的 `available` 和 topology 是 null。executor 用 `current_exe()` 运行 stdio child，未清理/改写环境，故继承 supervisor 环境；exit 4 被归类为 `Retirement::Unknown`，broker 进入 sticky quarantine。
- **影响**：修好 Xvfb 后，不能假定在同一个 supervisor 上不断重试 open 就会恢复。应先确认原 executor 退出及输入状态，再由操作员重启故障 supervisor；未知输入结果不能自动重放。
- **建议**：将“TLS可连接”和“真实 backend + open + PNG + 实际输入反馈”分开做健康门禁。在启动 supervisor **之前**固定正确 Xvfb 环境；不要只给客户端设置 DISPLAY。

### P2-4：Linux legacy 截图先分配全屏，返回图像预算不等于原生内存预算

- **位置**：`src/backend/desktop.rs:105-115`；`src/backend/linux.rs:70-88`；`REG/screenshots-0.8.10/src/linux/xorg.rs:79-93`；`src/runtime/session/observation.rs:69-77,105`；`src/runtime/image.rs:69-84`。
- **触发**：Docker 内存较小但 Xvfb 配置很大，或请求很小的 max_width/max_height 误以为会限制原始截图分配。
- **证据**：Linux 路径调用全屏 GetImage 并分配 `width*height*4` RGBA、编码 PNG，随后 runtime 才解码/缩小并检查 PNG 预算；Windows DisplayProvider 的源帧预算没有用于此分支。
- **影响**：峰值内存超出返回 PNG 大小时，可能 OOM/强制退出；不是小尺寸 smoke 的必然阻塞。
- **建议**：首轮限定单屏 1280×720×24，另行记录容器峰值内存；不可宣称低分辨率 observation 参数已经限制采集端内存。

## 已实现 / 明确不支持 / 未验证

| 项目 | 代码事实 | 验证边界 |
|---|---|---|
| Linux native factory | `backend/mod.rs:41-49` 选 Linux；`mcp/worker/native.rs:74-84` 构造 DesktopBackend；无静默 mock fallback | 未在 Linux 构建/启动 |
| X11 截图 | screenshots 的 XCB GetImage，PNG 编码及尺寸校验；不是调用外部截图命令 | 未见真实 Xvfb PNG；黑色空 root 也可能是有效截图，需带自有可见 fixture 判断 |
| 鼠标/滚轮/键盘 | `linux_x11.rs:383-443` 有 XTest motion/button/key/scroll；点击次数由应用按时间聚合 | 未验证靶点、焦点、拖拽、滚轮方向及应用实际接收 |
| 文本 | `linux_x11.rs:446-479` 仅当前 keymap 第一组未移位列已有字符；换行/tab 有路径 | 不是完全没有文本功能；默认布局通常可用小写/数字/空格；不能宣称任意 Unicode/大写标点原生文本可用 |
| 文本的部分结果 | `runtime/plan.rs:270-275` 逐 scalar 派发，`backend/dispatch_core.rs:115-122` 转发 | 后一字符不支持时前缀可能已经输入；单 scalar 的“先校验后副作用”不是整段原子性 |
| 剪贴板 paste | `backend/dispatch.rs:44-54` 默认明确拒绝；Windows 才覆盖实现 | Linux `text_input.method=paste` 不支持，不能作为 Unicode fallback |
| display-topology | 独立纯库无平台系统依赖（`crates/display-topology/Cargo.toml`）；Linux未接完整 native topology | `backend/mod.rs:163-190` + `desktop.rs:118-136`：仅 primary legacy；id/desktop selection 不支持，并非单屏观察/动作不可用 |
| 会话与清理 | 通用 held-state、pause/close cancel、epoch、限时及清理错误报告路径存在（`backend/dispatch_core.rs:73-112`；`mcp/worker.rs:350-362,523-537`） | 未验证 Xvfb 下实际 release；锁作用域和阻塞限制见上文 |
| Linux 全局急停热键 | `mcp/hotkey.rs:598-602` 明确 Unsupported；`computer-host.rs:355-376` 只告警，不拒绝服务 | 无 native 全局热键 ≠ host 无法运行；SIGINT/SIGTERM/SIGHUP、协议取消/close/EOF 有路径，但不是等价的本地桌面急停 |
| Wayland/XWayland 会话 | 后端和驱动主动拒绝带 Wayland 标记的会话 | 不在支持范围；纯 Xvfb 不因此不可用 |
| 可选桌面反馈 UI | 默认 off；`README.md:345-356` 明确 Linux 默认 renderer 不在本批范围 | 首轮不传 `--desktop-feedback`；不能用“默认无 renderer”判定 X11 backend 不可用 |
| Remote TLS | 平台通用 supervisor、认证、受控子进程、信号路径存在 | 尚未验证容器端口映射、TLS client pin、token、子进程环境和断线清理 |

## Linux/Docker 构建依赖建议（仅交付建议，未安装/执行）

建议先用 **Debian/Ubuntu glibc 基础镜像**做原生 Linux 构建，而不是把本机 macOS 构建成功当 Linux 证据；先不引入 musl/Alpine 的额外静态链接变量。工具链版本应固定并满足本 `Cargo.lock`；本仓库根 Cargo.toml 未声明 rust-version，本审查不凭 edition 推定最低 Rust 版本。

### 最小候选依赖

| 用途 | Debian/Ubuntu 包候选 | 源码依据/注意事项 |
|---|---|---|
| Rust native build | Rust + Cargo、`build-essential`、`pkg-config`、`ca-certificates` | `ring 0.17.14` 和 vendored libdbus 有 C 编译；最终必须完成链接，`cargo check` 不足 |
| 输入 FFI | `libx11-dev`、`libxtst-dev` | `linux_x11.rs:55,77` 直接 `#[link(name="X11"/"Xtst")]`；仅 runtime `.so.N` 不一定具备链接需要的 `.so` |
| 截图/枚举 FFI | `libxcb1-dev` | `REG/xcb-1.7.1/src/ffi/base.rs:118-124` 直接 link xcb；RANDR 为该 Rust crate 协议生成代码（display-info Cargo.toml:32-34），不能把扩展协议需求直接误写成必需额外链接 libXrandr/libxcb-randr |
| Xvfb + 检查 | `xvfb`、`xauth`、`x11-utils`、`x11-xserver-utils` | 用于虚拟服务器、授权、xdpyinfo/xrandr/xrdb；服务器仍须实际支持 XTEST 和 RANDR monitor 枚举 |
| 自有测试 UI | 独立 fixture 所需工具包/字体；例如 `fonts-dejavu-core`，按需 `x11-apps`/窗口管理器 | Host 本身不要求 GTK/Qt；焦点/窗口管理验收需由测试者明确配置，不把桌面环境当 Host 编译依赖 |

运行层候选：`libx11-6 libxtst6 libxcb1`，并以最终 ELF 的 `ldd` 结果补齐其余依赖；不要把 builder 的库目录挂载缺失误判成 Rust 不支持 Linux。上表包名是部署建议，不是已在指定镜像安装验证的清单。

**不应混淆的依赖：**

- `screenshots-0.8.10/Cargo.toml:57-65` 在 Linux 无条件引入 dbus（`vendored`）和 libwayshot，即使运行选择 X11 仍会编译这些 crate。
- `REG/libdbus-sys-0.2.7/build.rs:14-29` 的 `pkg-config dbus-1` 仅用于非 vendored 分支；当前锁定配置编译 vendored C 实现（`build_vendored.rs:339-395`）。**不能声称本图必须装 `libdbus-1-dev` 或启动 session bus 才能 X11 截图。** 改 feature 后另审。
- 当前 libwayshot → wayland-client → wayland-backend 未启用 `client_system`；`wayland-sys/build.rs` 仅在对应 features 下 probe 系统 Wayland。因此不要无证据把 `libwayland-dev` 当当前必需项；若实际构建图改变，再补充。
- 根 `Cargo.toml:19` 的 rustls 使用 ring 而非 OpenSSL；本 host TLS 编译不能机械加入 `libssl-dev` 必需项。测试证书生成可以另需 openssl CLI。
- `.cargo/config.toml:1-8` 替换 crates.io 到 rsproxy：容器首次 build 需要可达该镜像或完整离线缓存，网络失败不等同 Linux 代码编译失败。

交给构建执行者的检查命令（本次均未执行）：

```sh
cargo build --locked --release --bin computer-host --bin computer-client
ldd target/release/computer-host
ldd target/release/computer-client
```

`cargo check --locked --bins` 只能提供 Rust 编译阶段信息，不能替代上面的本机链接结果，也不能替代 GUI/输入验收。Docker 多阶段镜像须保证运行层 glibc/库版本匹配构建层。

## Xvfb smoke 最小检查清单（由获授权的 CC/验收执行者串行执行）

本清单是交付，不代表本审查者已执行，也不要求立刻进行所有故障注入。

### A. 构建与环境门禁

- [ ] Linux 原生 `--locked` 构建上述两个 bin 成功，保存目标架构、rustc/cargo 版本、镜像信息及日志；`ldd` 无 `not found`。
- [ ] 专属容器/专属 Xvfb，不共享真实用户桌面；一个真实 host；无 `--mock-backend`，无可选 renderer。
- [ ] 基线建议 `:99.0`、单 screen、1280×720×24、96 DPI；启动参数可采用 `-screen 0 1280x720x24 -dpi 96 -nolisten tcp +extension RANDR +extension XTEST`，并由环境负责人配置 `-auth`/Xauthority。不要开放 `xhost +`。
- [ ] Host、remote supervisor/child、fixture 使用同一 DISPLAY、XAUTHORITY 和可访问的 X socket。**unset WAYLAND_DISPLAY**；对独立 Xvfb 设置 `XDG_SESSION_TYPE=x11`。
- [ ] X server ready 后确认 XTEST、RANDR monitor 枚举、有活动 output 和 primary。需要时由环境负责人按实际 output 设置 primary；记录 xrandr 输出。
- [ ] 核对 RESOURCE_MANAGER/Xft DPI 不造成 scale≠1；记录输入几何与实际 root 尺寸。放置自有、可见、带点击/文字反馈的 fixture，焦点可控。
- [ ] 为初始化/运行均设置外部截止时间及独立终止入口；记录容器日志，不输出 token/私钥或未经授权截图。

### B. 单屏 native MCP 最小闭环

- [ ] 普通 stdio：initialize → tools/list，8 个工具；读取启动 stderr，确认真实后端初始化成功，Linux hotkey unsupported 仅告警。
- [ ] `computer_describe`：平台 linux；native 可用；primary-only，topology null + legacy note 是预期，不能要求 Windows 完整拓扑。
- [ ] `computer_open` 选择 primary；`computer_observe` 解码返回 PNG，核对 width_px/height_px、真实靶场内容以及图像坐标和 root 的对应关系。
- [ ] 基于**最新 observation**完成一个 move + click，重新 observe，证明目标控件实际变化，而非只看 dispatched。
- [ ] 同一自有 fixture 完成短拖拽、纵/横滚轮各一次、简单 key_chord 和短 key_hold；每步使用最新 observation/request 标识并确认应用反馈。
- [ ] 聚焦自有文本框，native 输入 `abc123`，可加换行/tab，观察实际内容。将不支持字符作为单独负例；混合文本可能已有前缀，不能要求整段回滚。
- [ ] pause → resume → 新 observation → close；确认 release/清理字段。不要把 Linux 全局 hotkey 未实现当这一闭环失败。
- [ ] 同 UID/同 TMPDIR 再起第二个 host，应得到 lease_conflict；关闭第一进程后新 host 可获得锁。不要删除正在使用的 lock 文件。

### C. Remote 最小补充闭环

- [ ] Xvfb 与 primary ready 后再启动 `--remote-listen` supervisor，显式设置其 X 环境。TCP `--listen` 限 loopback，不能在跨容器场景用它冒充任意地址的 TLS listener。
- [ ] 为 direct TLS 配置实际 listener 端口映射、证书/客户端 pin、token；只暴露所需端口，不暴露 X11 TCP。源码路径无 Windows-only gate，但网络连通性尚未验证。
- [ ] remote initialize/describe 成功只记为 discovery；必须 open → observe PNG → 一次真实 click → close 才记桌面 smoke。
- [ ] 断开拥有者连接，核对 executor 退出/清理、后继连接授权；强制终止或 unknown 不计 clean。若首次环境错误导致 quarantine，先核对退出和状态，再重启 supervisor，不盲目重放请求。

### D. 后续专项，不作为首轮正常链路的冒烟替代

- [ ] 无 primary / 无 DISPLAY / 空 WAYLAND_DISPLAY 的启动拒绝与诊断。
- [ ] Xft.dpi=192 及分数缩放的定位与尺寸回归（当前代码有静态缺陷，不应预期通过）。
- [ ] X server 卡住/退出、取消 during hold、容器 SIGTERM、close 后无残留 held input；保留 unknown/failed，不将 kill 当正常清理。
- [ ] 多容器共享桌面锁作用域、独立 Xvfb 的锁过度互斥、大屏低内存、可选 feedback renderer 均需专项设计/验证，不能由单屏 happy-path 推广。

## 交接摘要

优先环境动作：**原生链接依赖齐全 → Xvfb ready + RANDR primary + XTEST → 清除 Wayland 标记 → scale=1 → 单容器单 writer → open/PNG/真实反馈 → remote**。

优先产品后续议题：Linux root-pixel 几何修正、空 Wayland 标记一致性、锁作用域、初始化 deadline；这些均未在本次修改。当前结论是“存在受条件约束、值得继续实机验证的 Linux/Xvfb 实现”，不是 Linux 已验收通过，也不是 Linux 全部不可用。
