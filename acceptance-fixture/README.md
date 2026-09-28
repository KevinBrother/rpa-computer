# Computer Use Acceptance Fixture（视觉验收靶场）

给“只看截图”的 computer-use Agent 用的原生视觉靶场：每次试验生成全新随机内容，
Agent 必须通过截图识别 nonce、在四个打乱的图形中找到唯一目标（绿圆）并点击、
再在文本框里精确输入一段 Unicode 样例文本。**坐标和答案不会提供给模型。**

同一行为有两份实现，均无第三方依赖、无内嵌浏览器：

| 平台 | 源码 | 技术 |
|---|---|---|
| macOS | `macos/main.swift` | AppKit + swiftc（xcrun） |
| Windows | `windows/MainForm.cs` | .NET Framework 4.x WinForms + csc.exe |

窗口：900×650，标题 `Computer Use Acceptance`，含「Cancel / Close」按钮。
无任何文件系统 / 浏览器 / 终端 UI，不读写用户数据（仅可选写协调者指定的 evidence 文件）。

## 构建

macOS（本机）：

```bash
./acceptance-fixture/build-macos.sh /path/to/output-dir
# 产物: <output-dir>/ComputerUseAcceptance.app
```

Windows（在 Windows 机器上执行；可经 SSH 仅做编译，绝不要在 SSH 会话里启动 GUI）：

```powershell
powershell -ExecutionPolicy Bypass -File build-windows.ps1 -OutDir C:\path\to\output
# 产物: <OutDir>\ComputerUseAcceptance.exe
```

两个脚本都只写自己指定的输出目录，已存在的同名产物会报错退出而不是覆盖。

## 启动参数

```
ComputerUseAcceptance[.exe] [--seed N] [--evidence-file PATH]
```

- `--seed N`：可选项，固定 nonce/布局序列，便于协调者复现。
- `--evidence-file PATH`：可选项，**仅协调者使用**的 JSONL 证据文件（见下）。

## 试验内容（Agent 可见）

- `Trial N` 编号、`NONCE: XXXXXX`（6 位随机字符）、四宫格图形、状态条、多行文本框。
- 目标规则就印在窗口上：*TARGET: the only GREEN CIRCLE — click it*（唯一绿圆）。
- 点对：状态条显示 **TARGET HIT**；点错：**WRONG TARGET**。
- 文本框输入窗口上印出的样例 `computer-use 你好 10×20` 后点 **Check text**：
  完全一致显示 `MATCHED — n characters`，否则 `MISMATCH — expected 23, got n`。
- **Next trial**：新 nonce、新布局，清空状态与文本。

## 证据（协调者专属，与 GUI 成功分开判定）

`--evidence-file` 写入 JSONL，每行一个事件，带 UTC 时间戳：

| type | 含义 |
|---|---|
| `session` | 启动，记录 seed、platform |
| `trial` | 新试验：trial、nonce、target_slot、layout（如 `0:blue-diamond,1:green-circle,...`） |
| `hit` / `wrong` | 点击结果：trial、nonce、slot、target_slot |
| `text_check` | 文本校验：matched、expected_len、got_len |

注意：

1. 该文件**绝不能**出现在测试 Agent 的发布目录、cwd、MCP 资源或 prompt 中——
   它包含答案（nonce 与目标位置），只供协调者核对。
2. 它是**原生侧证据**，不替代视觉轨迹验证：判定通过仍须核对 Agent 的截图与点击序列。
3. 只记录本应用窗口内的交互，无全局截屏 / 键盘记录。

## 操作员准备

- 把编译产物单独复制到一个**不含源码**的发布目录交给测试 Agent；
  本仓库整体源码（含本 fixture 源码）对测试 Agent 保持不可见。
- 遮挡 / 最小化所有含源码的窗口（编辑器、终端历史、本 README），
  桌面只留靶场窗口与被测 Agent 运行环境。
- macOS 首次运行未签名 app：右键打开，或 `xattr -d com.apple.quarantine <app>`。
- Windows：必须在交互式桌面会话启动，不要在 SSH 服务会话里拉起 GUI。

## 协调者启动/停止（Windows 交互会话）

GUI 无法从 SSH 会话直接拉起（Start-Process/WMI 都会落在 Session 0）。
`start-windows.ps1` / `stop-windows.ps1` 由**协调者**在远端通过一次性计划任务
在**当前活动控制台会话**里启动/停止已编译的 exe。脚本只做这一件事，
不做截图、不跑模型循环；**进程存活不等于 GUI 成功**，判定仍需协调者核对截图与 oracle。

部署（只复制两个辅助脚本到已知诊断目录；exe 已在远端编译好）：

```bash
scp acceptance-fixture/start-windows.ps1 acceptance-fixture/stop-windows.ps1 \
    acer-win:C:/Temp/accfix/
```

启动（协调者执行；先自建一个证据目录，脚本在其中新建 GUID 唯一运行目录，
只复制 exe、不带源码，oracle 证据 JSONL 也在该运行目录内）：

```bash
ssh acer-win "powershell -NoProfile -ExecutionPolicy Bypass \
    -File C:\Temp\accfix\start-windows.ps1 -EvidenceDir C:\Temp\accfix\evidence"
```

- 源 exe 默认 `C:\Temp\accfix\ComputerUseAcceptance.exe`，启动前校验 SHA-256
  （默认期望 `2A80F568144C68B7370B8DE076F92ECB846F8BEBAF2361AF642AABE9B4ED1F32`，
  可用 `-ExpectedHash` 覆盖）。
- 会话解析：`WTSGetActiveConsoleSessionId` 取活动控制台会话，该会话内 explorer.exe
  的属主作为任务 principal（Interactive / Limited），不依赖 SSH 登录账号。
- 计划任务直接执行 staged exe + `--evidence-file <oracle>`，无 shell、无任意命令；
  每次运行新任务名（GUID），不删不改任何已有目录/任务；执行时限 30 分钟兜底。
- 输出 oracle 路径与 PID（**不含 nonce 内容**），并在运行目录写 `run-record.json`。

停止（只停记录且复核身份匹配的那一个任务/进程，证据目录保留）：

```bash
ssh acer-win "powershell -NoProfile -ExecutionPolicy Bypass \
    -File C:\Temp\accfix\stop-windows.ps1 \
    -RecordPath C:\Temp\accfix\evidence\run-<GUID>\run-record.json"
```

任何身份不匹配（任务 action/参数/principal、进程 PID/路径/会话/创建时间）都会拒绝操作，
不做模式匹配强杀，不删源 exe 或任何用户文件。

## 允许的测试 prompt 模板（不含 nonce / 坐标 / 答案）

> 屏幕上有一个标题为「Computer Use Acceptance」的应用窗口。请：
> 1. 读出窗口中显示的 NONCE 并复述；
> 2. 点击窗口中唯一的绿色圆形；
> 3. 在文本框中输入窗口上印出的样例文本，然后点击「Check text」；
> 4. 报告状态条与校验结果文字。
> 只能用截图观察，只能通过鼠标键盘操作这个窗口。
