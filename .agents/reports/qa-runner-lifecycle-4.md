# qa-runner-lifecycle-4 — watchdog 生命周期 + sandbox profile 持久化收尾（冻结）

日期：2026-09-24 ｜ 角色：QA runner 生命周期收尾（qa-runner-lifecycle-4）

边界遵守：**只写了 `scripts/watchdog-launch.py`、`scripts/run-blackbox-claude.sh`、`scripts/test-release-scripts.sh` 与本报告。** 未改任何其他源码、Rust、fixture、模型/配置。无 GUI、无远端启动、无 commit/push/worktree、无历史进程清理（协调者负责）。所有信号只发给**本进程/测试自己 spawn 且已核 pgid** 的组，从不碰调用者组。

---

## 最终快照（SHA-256，冻结）

| 文件 | hash |
|---|---|
| `scripts/watchdog-launch.py` | `be10ff48b4eebb7a1f17c7b6d620e321bcf0e62a808f8525c9e034ffdb38055a` |
| `scripts/run-blackbox-claude.sh` | `ec25aa6c196582738a47ef70ba79f7d884065fa60fd5332e15802be644d25d8a` |
| `scripts/test-release-scripts.sh` | `f7cbefcb22f2334ee8fdac88c9eb334aeae89c3c20ffd1f807267626ae04db07` |

---

## 验证结果（真实退出码）

- **`bash scripts/test-release-scripts.sh` → EXIT 0，`81 passed, 0 failed`**（连跑两遍均 81/0，确定可复现；此前基线为 73/0，新增 8 项检查）。
- **TDD red 阶段实测**（修复前）：4 类新检查全 FAIL——
  - `exact run sandbox profile persisted: sandbox.sb / sandbox.sb.sha256 missing from evidence dir`（旧 runner 退出即丢 mktemp profile）；
  - `normal-exit run cleaned the TERM-ignoring owned descendant: descendant pid=... still alive`（协调者审出的 qa-fix-3 遗留泄漏，测试已复现并做了测试侧最终清理，无泄漏残留）；
  - `interrupted watchdog cleaned leader + TERM-ignoring descendant (TERM): ... still alive`（中断路径完全无清理）；
  - `watchdog interruption cleanup (INT): watchdog did not exit within bounded wait`（INT 走 Python 默认 KeyboardInterrupt，finally 缺失 → 直接死掉且无清理）。
- **Green 阶段**：上述全部 PASS，含 `watchdog preserves leader exit code on normal exit (42, not 124)`、`watchdog reports truthful interrupted status (TERM -> 143 / INT -> 130)`、`persisted profile tamper is detectable via recorded SHA256`。
- **原有 timeout leader 场景保持绿色**：`watchdog exits 124 on wall-clock budget expiry` / `watchdog killed the owned descendant` / `did NOT touch the lookalike process outside its group` / `budget-exceeded run records exit code 124` 全 PASS。

---

## 改动逐项

### 1. `watchdog-launch.py` — 正常退出也做 owned-group 清理（qa-fix-3 承认的遗留）

- 抽出 `cleanup_owned_group(pgid)`：完全复用原有 TERM → 5s 宽限（组空才结束，leader 退出不结束）→ KILL → 2s 收敛 → 响亮告警逻辑，**未新建任何进程监督架构**。
- 清理从"仅 expired 分支"移入 `try/finally`：**预算到期、子进程正常退出、watchdog 自身被中断**三条路径执行完全相同的 owned-group 清理；组 id 始终是 `start_new_session` 后等于 spawned pid 的 pgid，绝不碰调用者组。
- **原始退出码精确保留**：正常退出返回 leader 自身 rc（回归测试用 42 验证，绝不变成 124）；预算到期仍 124。
- **中断（SIGINT/SIGTERM）**：信号处理器只记录 signum，清理在 finally 中统一执行；退出状态**如实**为 `128+SIG`（TERM→143、INT→130，与 shell 语义一致），绝不伪装成从未执行过的子进程 rc。INT 旧行为是 KeyboardInterrupt 栈追踪且零清理，现已修复。
- **枚举失败响亮化**：`group_members` 此前把 ps 枚举失败静默当空组（与自身注释相反）。现在 ps 非零退出/无输出/无可解析行 → 抛 `GroupEnumerationError`；`group_alive` 对失败枚举按 **UNKNOWN（非空）** 处理并打 ERROR，`signal_group` 在枚举失败时仍发经 `os.getpgid(pgid)==pgid` 安全闸验证的组兜底信号。**绝不再 success-as-clean。**

### 2. `run-blackbox-claude.sh` — 持久化真实使用的 sandbox profile

- print 模式沙箱路径下，把**负向探针与真实 launch 共用的同一份**生成 profile 复制到该 run 的证据目录（`cp` 于 `exec_and_audit` 之前，保证 probe 与 launch 用的正是这份字节）：
  - `evidence/<run-id>/sandbox.sb` — **0600**（profile 内含真实源码路径）；
  - `sandbox.sb.sha256` — 持久化副本的 SHA-256；
  - `sandbox.sb.meta` — 执行身份：generator 脚本路径 + 脚本 SHA-256、CLAUDE_BIN、max-seconds/max-turns；**不含任何 token/auth 内容**。
- 交互（非 print）模式无证据目录，维持原行为不扩 scope。源排除策略、单 profile 探针语义未动。

### 3. `scripts/test-release-scripts.sh` — 确定性回归（新增 8 项检查，73→81）

- **3b3 正常退出 leader + 忽略 TERM 的孙进程**：leader 立即 exit 42，留下同组 TERM-ignoring descendant；断言 watchdog rc=42 且 descendant GONE。有界等待（pid 记录由 fixture 自写），失败路径测试侧兜底 `kill/-9` 清理自己的 fixture，绝不泄漏。
- **3b4 watchdog 自身被中断**：TERM 与 INT 各一轮。child 为忽略 TERM 的挂死 leader + 忽略 TERM 的 descendant（两者都需 KILL 升级）；有界等待 fixture 就绪（10s 上限）后发信号，有界等待 watchdog 退出（30s 上限，`wait` 取真实 rc）；断言 rc=143/130、leader+descendant 均 GONE；失败路径测试侧 kill -9 兜底。
- **profile 持久化检查**：断言证据目录内 `sandbox.sb` 与 `sandbox.sb.sha256` **逐字节哈希一致**（非注释核对）、generator 脚本哈希与盘上脚本一致、meta 含脚本路径、profile 0600 / 证据目录 0700、deny 规则实存；另加篡改可检测性检查（改动副本后哈希必然不符）。
- 原 3b2 timeout 场景逐字未动，保持绿色。

---

## 已知限制（如实声明，不夸大）

1. **清理是尽力而为**：若 KILL 收敛后组内仍有活成员（理论上不可杀的内核态进程），watchdog 响亮告警但不改退出码语义——正常退出路径仍返回 leader rc，因为清理失败不改变"leader 正常退出"这一事实；告警文本是唯一信号。与 qa-fix-3 的 fail-loud 原则一致。
2. **枚举失败时倾向多信号**：`group_alive` 把 UNKNOWN 当非空，可能对已空组发无效信号（per-member pgid 复核 + 组 id 闸保证无害），代价是日志多一条 ERROR。绝不反向（把 UNKNOWN 当空而跳过清理）。
3. **交互模式不持久化 profile**：交互模式无证据目录，profile 仍在退出后随 mktemp 删除；仅 print（可审计）路径持久化。
4. **中断与到期同时发生的竞态**：两者都走同一清理，退出码取先被观察到的原因（interrupted 优先于 124）——语义如实，未做进一步细分。
5. **host 端 MCP 子进程在 CLI 退出后的 stdio EOF 行为仍未单独验证**（与 qa-fix-3 相同）；watchdog 只管自己 spawn 的组，正常退出路径的组清理现已覆盖该泄漏面。
6. 中断测试使用 `wait` 取 rc（128+SIG），与直接 exec 的退出码一致性已由 TERM/INT 两轮实测覆盖；未覆盖 SIGHUP/SIGQUIT（任务范围外）。

**实现已冻结，不再扩 scope。**
