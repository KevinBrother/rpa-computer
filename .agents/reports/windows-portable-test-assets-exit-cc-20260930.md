# Windows portable test assets — Gate 2 原生退出码 evidence-only 重跑

日期：2026-09-30。模型：glm-5.3-flash（sonnet alias）。无 agents/委派，无源码/配置改动，无 Mac/Linux 执行，无 GUI/网络 transport/部署 Host。

## 背景与缺陷

协调者复核发现：首次 gate 2 报告的 `EXITCODE=0` 不成立。原包装脚本在子进程退出后才读取 `Start-Process` 进程对象属性，PowerShell 返回 null（StartTime/ExitTime 均输出为空），会话中随后的 `0` 是包装器 exit 对 null 的强制转换，非 lib 原生数字退出码。与此前 feedback 的 Process.ExitCode-null 问题同型。原日志与首次身份空缺记录均保留未删。

## 重跑设计（attempt2）

- 同一冻结二进制：`C:\Users\Administrator\AppData\Local\Temp\portable-assets-cc-20260930-r1\lib-tests.exe`（SHA256 `593b5ee427354199371f00bfe9369ac458a418d7e23b5c39945c035404a06c6f`，未重建未改动）。
- 子脚本 `attempt2-child.ps1`：直接 `& .\lib-tests.exe --test-threads=1 *> .\lib-attempt2-stdout-stderr.log`，调用后立即 `$native = $LASTEXITCODE`，**先写 JSON marker**（`lib-attempt2-exit-marker.json`）再做任何其他 native 调用，最后 `exit $native`。
- 父监督 `attempt2-parent.ps1`：`Start-Process powershell -File attempt2-child.ps1`，**在 WaitForExit 之前**记录 child PID/Path/Session/StartTime 到 `attempt2-child-identity.json`；`WaitForExit(420000)` 强制 420s 限额；任何 timeout 终止前必须 `Get-Process` 核验 PID+Path 匹配才 Stop-Process（本轮未触发 timeout，无终止发生）；绝不读取 Process.ExitCode。
- 两侧均用 .ps1 文件，无内联 `-Command` 长引号串（首次远程调用因引号损坏未启动即失败，该次无副作用）。

## 实际证据（Mac 侧 Bash 工具 480s bound；远端执行 20:11:35 → 20:15:34，约 239s）

### 原生退出 marker（子脚本第一时间写入）

```json
{
    "marker":  "rpa-native-exit",
    "binary":  "C:\\Users\\Administrator\\AppData\\Local\\Temp\\portable-assets-cc-20260930-r1\\lib-tests.exe",
    "exitCode":  0,
    "capturedAt":  "2026-09-30T20:15:34.2138187+08:00"
}
```

**原生 exitCode = 0**（数字，来自 `$LASTEXITCODE`，与 `$?`/包装器 exit 无关）。

### child 身份（WaitForExit 前记录）

```json
{
    "childPid":  28936,
    "childSession":  0,
    "childPath":  "C:\\WINDOWS\\System32\\WindowsPowerShell\\v1.0\\powershell.exe",
    "childStart":  "2026-09-30T20:11:35.0720783+08:00",
    "parentPid":  17416,
    "recordedAt":  "2026-09-30T20:11:35.1470315+08:00"
}
```

（child 为承载 lib-tests.exe 的 powershell 宿主；lib-tests.exe 以 `&` 在其进程内直接执行，无中间 native 调用。）

### harness 结果

`test result: ok. 244 passed; 0 failed; 6 ignored; 0 measured; 0 filtered out; finished in 238.89s` — 与首次 gate 2 计数一致。

### 清理动作

实际清理动作：**无**。未发生 timeout，未调用 Stop-Process，无进程残留声明；远端仅新增 attempt2 日志/JSON 文件。原始 gate 2 日志与首次身份空缺记录完整保留（`logs/lib-stdout.log` 等）。

## 证据文件

- `.agents/runs/windows-portable-test-assets-cc-20260930/attempt2-child.ps1` / `attempt2-parent.ps1`
- `logs-attempt2/lib-attempt2-exit-marker.json`、`logs-attempt2/attempt2-child-identity.json`、`logs-attempt2/lib-attempt2-stdout-stderr.log`（已回传本地）

## 四 exe 拷贝声明复核

远端 `Get-FileHash` 已落盘为 `bundle-sha256.csv` 并回传，四个 exe hash 与源端逐字节一致（lib `593b5ee4…`、remote `82821f70…`、host `8c36602a…`、client `f40b68a3…`）。"四个文件已拷贝且 hash 一致"的声明有实际文件证据支持。

## 范围限定

仅补齐 gate 2 原生退出码证据；gate 1（10 TLS）与 gate 3（9 helper）的数值 exit 0 原本有效，未重跑。remote 17 项场景仍为 **DEFERRED**。本轮不宣称真实 TLS transport、feedback UI 或 remote 场景通过。
