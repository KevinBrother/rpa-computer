# qa-fix-3 — 最终报告（脚本/QA 集成修复，冻结）

日期：2026-09-24 ｜ 角色：唯一 QA/脚本集成修复（qa-fix-3）
边界遵守：**只写 `scripts/**`、`acceptance-fixture/{start,stop}-windows.ps1`、`acceptance-fixture/README.md`、协调者追加授权的 `tests/mcp_protocol.py`（独占，无其他 writer）与本人报告。未改任何 Rust / fixture Swift/C# 源码。无 commit/push/worktree。无 GUI、无输入注入、本人未注册/启动任何任务（GUI 由协调者运行）。从不按进程名强杀、不碰未知目录/任务。用户 model/auth 未动。**

---

## 最终快照（SHA-256）

| 文件 | hash |
|---|---|
| `scripts/run-blackbox-claude.sh` | `e74ea2bd4005c7be43d9c95fee110564061615a9540b501e77e24b846c4c409e` |
| `scripts/watchdog-launch.py` | `2b3f6972b99ca6aa38c46ce8a62e35d6513a5ca575567e91fc44096fd543cefa` |
| `scripts/package-release.sh` | `6382ac5ec5d5630239425b67ee382d9a277233833ea8056773161c5033e19d35` |
| `scripts/test-release-scripts.sh` | `19f66eb327638cc5eb5ef86964aaf3f199995d4023230e4ba70250e700a53b5f` |
| `scripts/deploy-windows-test.sh` | `c59196141e957b28a4ec8e5ff7af606240c62495e8f204c56a5b8e1726a66311` |
| `scripts/windows-start-host.ps1` | `8b6d1d00e97251f3441b2eb1df6087712f94f0fbd0e824688d2fc828c23046e7` |
| `scripts/windows-stop-host.ps1` | `04d51a8031eaaaa57128a76c382c36b495f1a30030699ba5cc15c9c23f43b4a6` |
| `scripts/mcp-bridge.py` | `91a259bd6882ec4c0c2022b243fb300f1f4461df601e1f8d35496f20e29e0928` |
| `scripts/audit-claude-transcript.py` | `d4c22dcd4152f4d8786249cafab57bcf8c11ddc51e7a40989ebbfaac266387e3` |
| `tests/mcp_protocol.py` | `b139f6f96eed4303f3981759fed89099c1fa19176bb8af3d3b3e4fdf6304cc7e` |
| `acceptance-fixture/start-windows.ps1` | `446adf038031762580e652dfe6c55887c5881538f37afdc75cf38e75307ea27b`（不变） |
| `acceptance-fixture/stop-windows.ps1` | `026d4331cd09867360f1f03c0a965018f650d91ac84962dbbfd89d0c8d8e3744`（**最终，取代 96b92f / a31e21**） |
| `acceptance-fixture/README.md` | `e92fc8613ce0efc31299c95996ec707afa73afb1e6507f1e0ee97a9271eaa9f1`（不变） |

---

## 验证结果（真实退出码，非伪造）

- **`scripts/test-release-scripts.sh` → EXIT 0，`73 passed, 0 failed`**（pipefail 下真实退出码；日志含 exact sandbox profile 负向探针、owned-group watchdog e2e、deploy 远端 Get-FileHash/CRLF/大小写归一、release/source overlap 拒绝、锁屏静态检查、安全命名临时根 release 接受 + rpa-* 命名拒绝）。协调者独立复跑同结果（`coordinator-release-scripts-3.log`）。
- **watchdog-launch.py 直接回归（本人复测）**：两种 TERM 形态均 `rc=124` 且子孙 `GONE`——
  (a) leader 忽略 TERM + 孙进程忽略 TERM；
  (b) leader 默认（TERM 即退出）+ 孙进程忽略 TERM 且会比 leader 活得久（旧实现会漏的场景，现已清）。
- **`--verify-only` argv 策略**：22 项 argv 全部正确（`--tools ''`、严格 MCP、禁 hooks、`--setting-sources user`、clean system-prompt 替换、print/stream-json/无会话持久化、任务文本在两模式都为末位位置参数）。
- **remote-bridge 打包冒烟**：`package-release.sh --skip-build --remote 8399` rc=0；`mcp.json` 仅含 `__TOKEN_FILE__` 占位（token 未入 release）；release 仅 `bin/mcp-bridge.py` 一个 .py；marker 在；MANIFEST 完整。
- **mcp_protocol.py 安全门**：mock+native-lock→拒绝 rc=2；native-lock 无 `--allow-real-gui`→rc=2；`--with-input` 无 `--allow-real-gui`→rc=2；真后端无 `--allow-real-gui`→rc=2。
- **协调者独立验证**：mock 协议 `50 passed / 0 failed / 1 explicit skip`（真实 debug binary）；native-lock `--allow-real-gui --verify-native-lock`（先 caffeinate 仅唤醒显示、不解锁、无 step 输入）`3 passed / 0 failed`（`coordinator-native-lock-2.log`）。

---

## 任务逐项状态

### A. Windows fixture helpers — 完成（已冻结，协调者已用 start 快照成功启动过）
- `start-windows.ps1`：`-Arguments`→真实 `-Argument`（远端实测参数集无 `Arguments`）；内存级语义验证通过。**hash `446adf...` 不变。**
- `stop-windows.ps1`：`-f` 位置修复；**verify-all-before-mutate**（先读+验任务 action/principal 与进程 PID/路径/会话/创建时间，全匹配才 Stop/Unregister，任何不匹配非零退出且保留记录）；Stop-Process 前存活复查（任务停止可能已结束同一 PID——已消失的已验证 PID 视为安全成功）。**最终 hash `026d4331...`**（fixture-launch-ready.md 已同步更新，请以此为准部署停止脚本）。

### B. Runner（`run-blackbox-claude.sh`）— 完成
- **单一精确 sandbox profile**：`build_sandbox_profile` 生成一份，既做负向探针（`sandbox-probe.sh` 必须证明源码树在该 profile 下不可读，否则拒绝启动）又交给最终 `sandbox-exec -f` 运行——删除早期“探针用较弱 profile”的缺陷。
- 拒绝 release/source 双向 overlap、release 落在文件系统/临时/home 根、release 内含 `.rs/Cargo*/.git/CLAUDE.md` 源痕迹；basename 命中 `rpa-computer-*/rpa-core-*`（本工具族暂存源码副本名形）的 release 目录直接拒绝（该名形在 sandbox 下会被 deny）。
- Deny 精确化：源码树 + `~/.cargo`/`~/.rustup` + 已验证含源的本项目历史副本（`/tmp/rpa-core-shim` 等，先确认确有源才列入）+ 临时根下 `rpa-(computer|core)-*` 名形（regex，canonical root 去重，命中 release 自身则跳过该 root）。**不地毯式封临时根**——Claude 需要 `$TMPDIR`/网络，安全命名的临时根 release 合法且可用。
- **锁屏安全**：clean system-prompt 追加——任一截图为登录/锁屏/凭据界面立即停止、调 `computer_close`、报告环境被锁屏阻塞，绝不向该界面输入凭据或任务文本（Mac 当前锁屏，未尝试解锁）。

### C. Watchdog 进程所有权 — 完成（含本轮孙进程泄漏修复）
- `watchdog-launch.py`：`subprocess.Popen(..., stdin=DEVNULL, start_new_session=True)` → 子进程 pgid==pid，子孙（sandbox-exec→claude→MCP/bridge）继承同组。预算到期：先核 pgid 仍等于 spawned pid（绝不碰调用者组 `os.getpgid(0)`）→ TERM → 5s 宽限 → **组内仍有活成员才 KILL**（leader 退出不结束清理，只有组空才结束）→ 2s 收敛 → 仍有成员则**响亮告警（绝不静默泄漏）**。
- **本轮关键修复**：原先“leader 死才 KILL”会漏掉忽略 TERM 的孙进程；且 `ps(1)` 成员枚举在 macOS 上被证实**不可靠**（间歇漏报 pgid 明明匹配的存活成员）——改为**逐成员发信号**（发前立即用 `os.getpgid` 复核该 pid 仍在组内，pid 复用者跳过）+ `killpg` 兜底，成员枚举只作 liveness 判定。**回归用 trap-ignored 孙进程两种形态实测通过。**
- stdin=DEVNULL → 超时后无无限后台输入。
- **保留**：`--tools ''` 严格 MCP、八工具精确审计、用户 model/auth 不变；源排除检查未为通过假测试而削弱。

### D. 锁屏安全 — 完成
见 B（clean Agent 指令追加锁屏即停条款）。Runner `--print` 任务文本强制要求截图/观察类结果。

### 部署脚本（`deploy-windows-test.sh` / `windows-start-host.ps1` / `windows-stop-host.ps1`）— 完成（已冻结）
- deploy：远端**实际 `Get-FileHash`** 逐文件核对（不信上传的 MANIFEST），`tr -d '\r'` + `tolower()` 归一（远端 CRLF + Get-FileHash 大写）；`ps_remote` 的 `$` 一刀切拒绝已移除（parse/ACL 块本身含 `$`，单引号调用点已保证安全）；MANIFEST 不含自身。协调者真实诊断部署 exit0（远端 hash/parse/ACL 全过）。
- start-host：explorer 属主两步 SID 解析；任务碰撞即拒绝（只重注册**本工具**任务）；启动后**必须** 127.0.0.1:Port 监听器 OwningProcess==所启 PID 才报 OK；进程已起但端口不符时写 `healthy=false` 的 UNHEALTHY 实例记录（不丢失已启动资源身份，供协调者安全清理）。协调者真实启动 exit0（PID17416 Session1 owned listener）。
- stop-host：verify-all-before-mutate + 完整字段（PID/exe/CreationDate 精确±2s/**session_id**）；缺 creation_date 即拒；mismatch 非零退出且**保留记录**；任务/进程确认皆消失才删记录；Stop-Process 前存活复查（已消失的已验证 PID 安全成功）。

### `tests/mcp_protocol.py`（协调者追加授权范围）— 完成
- mock 协议对**原生排他性显式 SKIP**（`second host open is refused (lease_conflict)`），注明 mock 不持真实桌面锁、该属性**此处未验证**，绝不假 pass；selftest 同样显式 skip。
- 新增 `--verify-native-lock`（**仅协调者**，需 `--allow-real-gui`，禁 `--with-input`，禁 mock）：只跑双 host 锁场景，observe 级、无 step/输入。第二 host 若在 acquire-lock 时**非零退出 + 准确 lease_conflict stderr** 视为合法拒绝（不强制它还能回 initialize 工具错误）；**泛 timeout 绝不当作锁成功**——第二进程活着但不响应、或退出但无准确 lease_conflict 证据，都判为**环境失败**（锁屏/休眠显示/wedged reader/未实现 lease），绝不 fake pass。
- Host 启动失败现在把进程退出码 + stderr tail 纳入诊断（`StartupFailure`），不再裸 stacktrace。

---

## Windows remote release 的 stage / 运行命令（交付协调者）

```bash
# 1) 打 remote-bridge release（本机；不含 host exe，host 在 Windows 由协调者另行部署启动）
scripts/package-release.sh --skip-build --remote 8399 \
  --release-dir "$RELEASE_DIR"

# 2) 每 run 生成 token（0600，放在 release 目录之外，只有 MCP bridge 子进程读，绝不传给 Agent）
install -m 600 /dev/null "$TOKEN_FILE" && openssl rand -hex 32 > "$TOKEN_FILE"
sed -i '' "s|__TOKEN_FILE__|$TOKEN_FILE|" "$RELEASE_DIR/mcp/mcp.json"

# 3) 黑盒 runner（协调者运行；--print 出可审计 transcript）
scripts/run-blackbox-claude.sh --release-dir "$RELEASE_DIR" \
  --task "$RELEASE_DIR/tasks/01-image-recognition.md" --print
```

---

## 诚实声明 / 尚未验证项（不夸大）

1. **watchdog“正常 leader 退出后是否清理残余子孙”未覆盖**：预算到期路径已实测（含忽略 TERM 的孙进程两种形态），但**非超时**的“leader 自己正常退出、却留下孤儿子孙”路径，helper 只等 `proc` 本身退出后返回——**该路径不做组清理，未验证也不保证清理**。这是真实限制，报告如实写明。
2. **host 端 stdio EOF 后 MCP 子进程的清理由 host 实现负责，本人未单独验证**（watchdog 只管自己 spawn 的组）。
3. **fake claude 测试只证明脚本策略正确，绝不证明 GUI 成功**；GUI 验收由协调者对照 ground truth 另行判定。
4. **mock 协议证明的是“mock 图像流经协议”，不证明真实截图/输入能力**；原生排他性已由协调者经 `--verify-native-lock` 单独补证（3/0），mock 套件中该项为显式 SKIP。
5. **Windows 旧 exe 的 tools/list EOF 断连缺陷为 host 端问题**（协调者已用直连 socket 缩小边界：initialize 正常、tools/list 得 EOF）；新 exe 须在同边界实测——协调者 diagnostic3 新 exe 经当前 bridge 的 tools/list(8)+open/close 已成功，observe 到 native 报 `capture_error: GDI screen capture failed ... 拒绝访问 0x80070005`（同 Session1 有 LogonUI.exe，锁屏所致，非 TCP 问题），两端用户已被请求手动解锁，**未得到图、零输入**。
6. Mac 桌面锁屏未尝试解锁；runner 的锁屏即停条款已就位。
7. 本人未注册/启动/停止任何 Windows 任务或进程；所有启停由协调者按精确身份执行（diagnostic Host 26452 已自行 EOF 退出，task 已清理 stop-3 exit0）。

**实现已冻结，不再扩 scope。**
