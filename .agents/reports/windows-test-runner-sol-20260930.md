# Windows bounded test runner — frozen implementation handoff

- 日期：2026-09-30；源码实现身份：Codex（用户指定 gpt-6.1-sol）。
- 分工：本次仅代码/自测源码；Windows 编译与实际验证交由真实 CC+GLM。
- **未运行测试、GUI、SSH、CC，也未进行本地编译、commit、push、worktree。没有新的 red/green 运行证据或通过声明。**
- 只写下表四个源码文件及本报告；未改 root/Rust/Cargo、renderer、fixtures、旧证据或现有部署/停止脚本。
- 既有任务记录指出原 Start-Process/WaitForExit/ExitCode=null 与嵌套 quoting 丢失退出码/日志的 red evidence；此处沿用记录，不重造或覆写旧证据。

## 冻结 SHA-256

以下路径相对于 `rpa-computer/`；CC 在 Windows checkout 中先核对文件字节（不要转换 BOM/换行）。报告自身 SHA 在交接回复中给出，避免自引用。

| 文件 | SHA-256 |
| --- | --- |
| `scripts/windows-test-runner.ps1` | `ad2a607ca0af59983b306ba5f6c32611005669a6b7d385834ccceb027a9572c4` |
| `scripts/windows-test-runner/Runner.cs` | `aced18e616afe881845b2ac233e32a190b7186b69492aaa7be416ac41e972214` |
| `tests/windows-test-runner/Child.cs` | `9cb8c32bc2bb7965aa91d20588e3c6c5c98fa9df9cc400dfcf6fccb4d45a5b56` |
| `tests/windows-test-runner/SelfTest.ps1` | `1e4212d7f6f0e4059b476f8d84f58cc49b2f61249653f2ca4794fe61b600cceb` |

## 实现契约

入口：`powershell.exe -NoProfile -File scripts/windows-test-runner.ps1 -ConfigPath <absolute JSON>`。
严格 JSON 仅允许五个必需属性：`executablePath`、`arguments`（string[]）、`workingDirectory`、`evidenceDirectory`、`timeoutSeconds`（整数 1..3600）。配置文件不超过 1 MiB；最多 4096 参数；命令行必须低于 Windows 32767 UTF-16 字符上限；拒绝 NUL。路径限本地 drive-qualified 绝对路径，拒绝 UNC/相对路径；不通过 PATH 查找目标。

- 不引入服务、计划任务、凭证、网络调用或全局配置；继承调用者 Windows session/context。只用于显式授权的纯 console/test executable，不能用来绕过 GUI/session 约束。
- Windows Framework CRT argv encoder 每参数独立加引号，保留空字符串、空格、双引号、反斜杠与尾部反斜杠；不是 shell 表达式。自测以 child 输出 Base64 独立比较输入，不调用 encoder 作为 oracle。任意使用自定义非 CRT argv parser 的 executable 不在 roundtrip 保证范围内。
- `CreateDirectoryW` 原子占用新 evidenceDirectory，已存在目录/文件不覆写；父目录必须已存在。保留原 config。任何已占用证据目录不会被删除。
- 拥有 `System.Diagnostics.Process`，`UseShellExecute=false`，在等待前缓存 OS handle，持久化 `identity.json`：PID、开始时间、启动 executable 绝对路径、session 与 runner/config/executable SHA。
- 短命进程的 `Process.SessionId` 元数据可能已消失：fallback 明确标为继承 caller session（无 alternate token），不是声称直接观察成功。executable identity 明确标为 ProcessStartInfo 的启动路径；没有声称原子防止磁盘 executable 替换。
- 两条独立 background long-running drain 以原始字节写 stdout.log/stderr.log；每流保留最多 4 MiB，超额继续读取并标记截断，计数记录保留/观察字节。没有按行积累子进程输出。
- 执行与 drain 共用 Stopwatch deadline。正常退出只接受真实整数 ExitCode；native 非零映射 harness 1，保留原 native code。harness 70 validation/launch/unexpected failure；124 timeout；74 drain/summary-write failure；75 unknown native exit；0 仅成功。
- 超时仍保留日志、summary 与 identity；可能取得终止后的 native code，但 outcome 仍 timeout，不当作成功。

### 失败证据边界（不得隐瞒）

未能解析/信任配置、已存在目录或目录占用失败时，不能安全写调用者指定目录；结构化 failure summary 输出 stdout，caller 必须保留 stdout/stderr。bootstrap（包括 Add-Type）失败输出 stderr 并 exit 70。自测为每例保留独立 harness stdout/stderr。有效配置且目录占用成功后，缺失 exe、坏 cwd、launch failure 等保留 config/summary。文件系统写入自身失败时只能向 stderr 报告，不能保证故障磁盘上仍有 summary；不删除任何已写证据。

## CC 运行命令（本次未执行）

在现有交互 Windows console/session，切到对应 Windows checkout 的 `rpa-computer`。不从 Session 0 启动 GUI，不触碰 Notepad PID 24332 或任何无关进程。

```powershell
# 核对下面输出与冻结表；这是交接前置检查。
Get-FileHash -Algorithm SHA256 scripts/windows-test-runner.ps1, scripts/windows-test-runner/Runner.cs, tests/windows-test-runner/Child.cs, tests/windows-test-runner/SelfTest.ps1

# 唯一自测入口；不安装测试库。
powershell.exe -NoProfile -File .\tests\windows-test-runner\SelfTest.ps1
$LASTEXITCODE
```

SelfTest 在新的 `%TEMP%\windows-runner-<GUID>` 下，用现有 Framework64/Framework `v4.0.30319\csc.exe` 编译 Child.cs，然后运行 **16 个 case**：zero、nonzero、argv、stress、timeout、missing、existing、nullarg、unknown、bound、duplicate、nonstring、launch、cwd、nullarray、malformed。zero 另断言 source/process identity；nonzero/timeout/launch/cwd 断言失败产物；argv 含 8 个特殊参数；stress 独立断言两流均达到 4194304 字节并截断。只有所有断言完成才写 `selftest-result.json`（cases=16/outcome=passed）及打印 PASS；异常/nonzero 为失败。

每次 runner invocation 外侧有独立 30 秒自测 watchdog，额外 root kill wait 2 秒与 harness output drain 2 秒。它只拥有并终止被测试 harness 根进程，不宣称 root 的 child fixture 也被回收。watchdog 到期会抛异常并保留 TEMP，CC 应记录当时任务 identity 后处理残留，不按名称杀进程。fixture csc 编译步骤本身没有内置 watchdog；CC 应在编译卡住时停止并记录，不能把它报成 runner pass。

CC 应保留 TEMP 全目录、实际 native harness exit、所有 summary/identity、case 结果与核对 SHA 的输出；任何编译失败/异常/未知 native code 先报真实失败，不 coercion 为 0，也不删除失败 attempt。若 CC 发现源码问题，带原始错误与冻结 SHA 交回实现 owner，不与其他 owner 文件混改。

## 超时/子进程限制审阅

**没有 Windows Job Object containment，也没有 descendants enumeration。**只对 exact owned Process/cache handle 请求 Kill，不使用 process-name/global kill，不按重新查询 PID 杀进程；root exit observed 不等于 descendants reaped。root 已退出但 descendant 持有 inherited stdout/stderr pipe 时，drain 会等到共用 deadline 后报告 timeout。残留 children 状态始终未知，summary 显式记录 limitation。

root kill 后允许额外固定 2000 ms 观察退出、250 ms drain grace。drain 未完成时计数/截断为 null（不是伪造完整结果），并标记 incomplete；不在阻塞 read 的同时 Dispose redirected streams，避免 Framework teardown deadlock。background drains 可在宿主退出时结束；文件可能仍 open/不完整，不能称已完成排空或已完全清理。

deadline 限制等待 root/drain，不是可打断所有 Windows 同步 syscall 的硬实时保证：Add-Type 编译、Process.Start、文件读写/hash、Kill 与 OS I/O stalled 等不可取消同步调用可能超出时间；固定 cleanup grace 也不计入配置 deadline。外侧 watchdog 用于发现这种 harness 卡住，但亦不能证明清理 descendant。没有 privileged isolation、reparse-point/恶意目录竞态防护或 executable hash-to-launch 原子锁定；假定授权者控制本地配置、工作目录与证据父目录。

## 交接状态

源码静态审阅并冻结，**Windows 编译/16-case 验证待 CC**；这是支持测试基础设施的实现交接，不是 root/renderer/fixture 产品验收。
