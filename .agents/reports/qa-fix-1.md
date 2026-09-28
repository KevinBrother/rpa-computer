# qa-fix-1 — QA/release 安全缺陷修复报告

日期：2026-09-24 ｜ 作者：QA/release repair worker（接手初始 QA worker 的范围）
依据：`.agents/reviews/initial-review.md` 中协调者 review 的缺陷清单。
范围：tests/、scripts/、examples/acceptance/、README.md；未改任何 src/Cargo/lib，未 commit/push。

> 历史报告（`qa.md` 等）保持原样，本报告只描述本次修复；旧报告中的结论不视为已通过。

---

## 1. run-blackbox-claude.sh：真正的工具隔离（review #1/#2）

**问题**：旧脚本只用 `--allowedTools`，那是“自动批准清单”，不关闭内建工具；Agent 仍可用
Bash/Read 等内建工具读源码。另有 trailing 任意参数透传、`--no-sandbox` 随意可用、
探针与正式进程 profile 不一致等问题。

**修复**（scripts/run-blackbox-claude.sh，285 行重写）：
- `--tools ''` 关闭全部内建工具；`--allowedTools` 仅列 8 个 `mcp__computer__computer_*`；
- `--strict-mcp-config`（只加载发布目录 mcp.json）+ `--disable-slash-commands` +
  `--settings '{"disableAllHooks":true}'`（每次运行独立）+ `--append-system-prompt` 干净系统提示；
- 任务文本在 `--print` 与交互模式**都**显式作为参数传入；
- `--no-sandbox` 仅在 `--diagnostics` 同时给出时允许（验收路径禁止）；
- 拒绝 `--` 之后的任意 trailing claude 参数（exit 2）；
- 拒绝危险提权 flag（`--dangerously-skip-permissions`/`bypassPermissions`）——静态检查保证
  这些字符串不出现在 CLAUDE_ARGS 组装行；
- `canonical_dir()`（`pwd -P`）规范化路径；任务文件位于源码树内则拒绝；MCP 配置中
  引用源码路径则拒绝；
- 单一 `build_sandbox_profile` 生成 profile（deny file-read*/file-write* 源码树 +
  `/tmp|/private/tmp|$TMPDIR/rpa-computer-*` regex），**负向探针与最终 exec 使用同一文件**；
- 探针命令固定为 `scripts/sandbox-probe.sh`（重写：支持 SANDBOX_PROFILE 环境变量、
  canonical 源码路径、读/列目录/写探针 + `/etc/hosts` 正向对照）；
- `--verify-only`：在 mktemp 中建假 claude 记录 argv，按固定位置审计 0–13 位参数，
  不匹配即非零退出。明确声明：这只是 argv 策略证据，不是 GUI 证据。

**证据**：`scripts/test-release-scripts.sh` 第 2 节 5 项全过（verify-only 通过、
--no-sandbox 拒绝、trailing 参数拒绝、无提权 flag、`--tools ''` 存在）。

## 2. package-release.sh：发布目录销毁风险（review #3）

**问题**：旧脚本对调用方给定路径词法检查后直接 `rm -rf`，可删除用户数据；
dev profile 解析成 target/dev（应为 target/debug）；manifest 在 RELEASE.json 之前生成。

**修复**（220 行重写）：
- 目的地校验（fail-closed）：拒绝 `/`、`$HOME`、源码树内部、源码树祖先、/tmp、TMPDIR
  根、符号链接；对不存在路径用 `resolve_parent_canon` 取最深存在祖先的 canonical 路径再比对；
- 已存在目录：只有含合法 `.rpa-computer-release` marker（内容 `rpa-computer-release-v1`）、
  无符号链接、bin/mcp/tasks 形态正常时才允许复用；清旧内容用
  `find -mindepth 1 -maxdepth 1 ! -name marker -exec rm -rf -- {} +` 逐项删除，绝不对给定路径 rm -rf；
- 其他任何已存在目录直接拒绝（exit 2），用户数据保持原样；
- `target_dir_for_profile`：dev→debug、release→release、自定义→同名；
- 所有写入 JSON 的路径/元数据经 python3 `json.dumps` 转义；
- MANIFEST.sha256 最后生成，覆盖 RELEASE.json 与全部暂存文件；并有完整性回路校验
  （逐文件 grep + 条目数==文件数），不满足则 exit 1；
- 不再声称“干净环境”——mcp.json 的 `env: {}` 只表示不新增变量，注释已说明继承环境。

**证据**：测试第 1/1b 节 12 项全过，含“已存在用户目录被拒绝且 keep.txt 完好”、
manifest 覆盖 RELEASE.json、manifest 条目数==11==暂存文件数、marker 复用放行。

## 3. Windows 启停脚本：交互会话身份与自有进程边界（review #4）

**问题**：旧脚本用 `$env:USERNAME`（SSH 会话用户）当计划任务主体与 token ACL 对象；
启动后只按进程名匹配（会匹配到所有 computer-host 进程）；停止脚本按名称模式杀进程。

**修复**：
- `windows-start-host.ps1`（150 行重写）：Add-Type 调 `WTSGetActiveConsoleSessionId` 取
  活动控制台会话；要求 explorer.exe 恰好运行在该会话；用
  `Get-CimInstance Win32_Process | Invoke-CimMethod GetOwner` 取 explorer **属主**作为
  计划任务主体；token 文件 ACL 拒绝 Everyone/BUILTIN\Users/Authenticated Users 读；
  生成 run-host-logged.cmd 包装，stdout/stderr 持久重定向到部署目录
  host.stdout.log/host.stderr.log；启动后 10×1s 轮询，要求新进程
  Name==computer-host.exe 且 ExecutablePath==预期 exe 且 SessionId==控制台会话且
  CreationDate≥启动时刻；写 `host-instance.json`（task_name/pid/exe/session_id/
  interactive_user/port/started_utc）。
- `windows-stop-host.ps1`（51 行重写）：只认 host-instance.json（无记录→"nothing owned"，
  exit 0）；只停止记录的计划任务；停进程前重新查 Win32_Process，要求
  ExecutablePath==记录 exe 且 Name==computer-host.exe（防 PID 复用误杀），否则拒绝；
  成功后删除记录。无 `Get-Process -Name` 模式匹配。
- `deploy-windows-test.sh`：token ACL 远端命令改为内嵌 WTS Add-Type 解析控制台会话
  explorer 属主，不再用 `$env:USERNAME`；删除死代码 REMOTE_SUMS 块。
- 不启动 GUI、不改防火墙（本任务范围外）。

**证据**：测试第 3 节 3 项静态检查全过（无名称模式杀、exe 路径复核存在、
WTS 会话选择存在且主体不来自 SSH env 用户）。PowerShell 脚本未在 Windows 上实际执行
（见“未测项”）。

## 4. Rust 契约测试拆分与断言强化（review #5/#6）

- `tests/runtime_contract.rs` 由 >1600 行单文件改为 47 行瘦根 + `tests/contract/` 下
  10 个主题模块 + support.rs（MockBackend/SharedBackend/Harness/tiny_png 等，275 行）；
- 幂等 close 强化为显式终态：第二次 close 必须是“会话作用域错误”或
  state∈{closed,faulted} 的成功，cleanup_outcome∈{not_needed,released,failed,unknown}，
  且不得产生新输入；cleanup 失败后 close 必须报 faulted 而非 released；shutdown 的
  清理失败不可重试、必须持续上报；
- freshness/partial/capture_fail/cancel 增加边界断言（stale step 不得派发事件、
  events_completed 边界、capture 失败后 get_step 结果保留、取消的长动作不得报 dispatched）；
- registry 1000 请求测试保留覆盖但标记 `#[ignore]`（1000 步串行，显式 `--ignored` 运行），
  使用 32×32 小 fixture；另有非 ignore 的 distinct-request-id 派发测试保持活跃；
- `tests/mcp_protocol.py` 的幂等 close 场景同步强化（第二次 close 必须会话错误
  或 closed/faulted）。

**证据**：无法执行——见“未测项 #1”。模块行数：47/151/160/219/181/168/103/118/169/176/275/141。

## 5. mcp-bridge.py：有界队列与文档对齐（review #7）

- 双向队列各 64MiB 上限，超限 `fail(...)` 关闭而不是无限缓冲；
- stdin→TCP 一次性 SHUT_WR 干净半关闭；TCP EOF 先冲刷再退出；绝对（不可延长）截止时间注释明确；
- docstring 明确 Host 认证是**原始 token 行**（换行结尾），不是 JSON；
- token 只从文件读取、永不打印（shell tracing 中不出现）。

README 同步：桥接认证方式、xwin 工具链、新脚本选项、Windows 启停语义均已更新。

## 6. GUI 验收夹具计划（review #8）

- 新增 `examples/acceptance/GUI-TEST-FIXTURE.md`：协调者桌面准备清单（关闭所有窗口/
  终端/编辑器、中性壁纸、逐任务 fixture 准备）、stream-json init 工具清单审计
  （必须恰好 8 个 computer_* 工具）、完整工具调用轨迹审计、最终截图人工核对、
  明确非声明（--verify-only 不是 GUI 证据；GUI 非密闭沙箱）。
- `02-target-click.md` 等任务提示增加“不得用终端/AppleScript/文件读取计算或验证坐标”。
- 本 worker 不运行 GUI；真实 GUI 验收待协调者按清单执行。

---

## 执行的命令与结果

| 命令 | 结果 |
|---|---|
| `scripts/test-release-scripts.sh` | **20 passed, 0 failed**（危险目录拒绝 6、happy path/manifest/marker 6、argv 策略 5、Windows 静态 3） |
| `bash -n scripts/*.sh`（6 个脚本） | 全部 OK |
| `python3 -m py_compile scripts/mcp-bridge.py tests/mcp_protocol.py` | OK |
| `QA_HARNESS_SELFTEST=1 python3 tests/mcp_protocol.py` | argparse 需 --host；仓库内无 fake python host 可执行文件，自测模式当前不可用（见未测项 #2） |
| `cargo test --test runtime_contract --no-run` | **失败**：lib 编译错误（E0061/E0063/E0277/E0308/E0609，11 errors + 2 warnings）——core/backend worker 范围，按约定单次重试后不再等待 |

## 未测项（明确声明，不算“已验证”）

1. **Rust 契约测试一行都未运行**：lib 编译被 core/backend 的 11 个错误阻塞（本次无 src 改动权限）。拆分的测试代码本身未经编译器验证，可能存在编译问题，待 lib 修好后需首先跑 `cargo test --test runtime_contract` 与 `cargo test --test runtime_contract -- --ignored`（registry 慢测试）。
2. **mcp_protocol.py 未端到端运行**：既无可用的 host 二进制（lib 编译失败），也无仓库内 fake host；自检入口存在但缺配套假 host 文件。幂等 close 的强化断言未被执行过。
3. **真实 GUI 验收未运行**（也不允许本 worker 运行）：工具清单审计、轨迹审计、截图核对均为计划而非结果。--verify-only 只证明 argv 策略，不证明任何 GUI 行为。
4. **PowerShell 脚本未在 Windows 执行**：仅静态策略检查 + bash 侧部署脚本语法检查。WTS/explorer 属主/计划任务/停止复核逻辑需在目标 Windows 机器首跑验证。
5. **sandbox 探针只验证了脚本逻辑与 profile 语义**，未对真实 claude 进程做过一次完整沙箱化运行（claude 进程需协调者调度）。
6. mcp-bridge.py 的有界队列/半关闭为代码审查结论，未做压测。
