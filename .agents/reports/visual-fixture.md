# 视觉验收靶场（acceptance-fixture）实现报告

日期：2026-09-24 ｜ 角色：实现 worker ｜ 范围：仅 `acceptance-fixture/**` 与本报告。**只编译，未运行任何 GUI。**

## 交付物

```
acceptance-fixture/
├── README.md            # 构建/启动参数、原生证据 vs GUI 判定、操作员准备、允许 prompt 模板
├── build-macos.sh       # swiftc → .app；只写指定输出目录，存在即报错不覆盖
├── build-windows.ps1    # Framework64 csc.exe → winexe；同上保护
├── macos/main.swift     # 353 行，AppKit，无依赖
└── windows/MainForm.cs  # 402 行，WinForms + Framework 4.x，无 NuGet
```

行为两平台一致：900×650、标题 `Computer Use Acceptance`、明显的 Cancel/Close；
每试验新 6 位 nonce（去歧义字符集）+ Trial 编号；四宫格图形打乱，唯一绿圆为目标
（目标规则印在窗口上，prompt 不含答案）；点对 `TARGET HIT`、点错 `WRONG TARGET`；
多行文本框 + `Check text` 精确比对 `computer-use 你好 10×20`（23 个字位簇），
显示 matched/mismatch 与字符数；`Next trial` 重置全部状态。
`--seed N` 可复现；`--evidence-file PATH` 输出协调者专属 JSONL
（session/trial 含 nonce+layout+target_slot/hit/wrong/text_check，UTC 时间戳）。
无网络、无内嵌浏览器、不读写用户数据，证据文件绝不进 Agent 发布目录。

## 编译验证（真实产物）

### macOS（本机，swiftc 6.3.3，arm64）

- 命令：`./acceptance-fixture/build-macos.sh`
- 产物：`acceptance-fixture/build/macos/ComputerUseAcceptance.app/Contents/MacOS/ComputerUseAcceptance`
- SHA-256：`1ad8a3c4cd26f9bc64b1a15d11f6d62c3b89f88f8ff9eea4bc2a27ff7b133cf6`
- 结果：编译零警告零错误。

### Windows（ssh acer-win，Framework64 csc.exe v4.0.30319，仅编译未启动）

- 命令（远端）：
  `C:\Windows\Microsoft.NET\Framework64\v4.0.30319\csc.exe /nologo /target:winexe /platform:anycpu /utf8output /optimize+ /out:C:\Temp\accfix\ComputerUseAcceptance.exe C:\Temp\accfix\MainForm.cs /reference:System.dll,System.Drawing.dll,System.Windows.Forms.dll`
- 产物（暂存远端，待协调者转交互式桌面发布目录）：`C:\Temp\accfix\ComputerUseAcceptance.exe`
- SHA-256：`2A80F568144C68B7370B8DE076F92ECB846F8BEBAF2361AF642AABE9B4ED1F32`
- 结果：exit=0，零警告零错误；`build-windows.ps1` 已同步到远端同目录。

## 诚实声明 / 缺口

1. **未运行、未验证任何 GUI 行为**：两个产物只编译通过，窗口布局、点击命中、
   文本比对、证据文件写出均未做运行时验证，需协调者在交互式桌面实测。
2. Windows 端 nonce 非种子熵源用 `DateTime.UtcNow.Ticks`（随机性弱于 macOS 的
   `UInt64.random`），仅影响未传 `--seed` 时的不可预测性，不影响功能。
3. 远端产物在 `C:\Temp\accfix\`（含源码副本），发布给测试 Agent 前须只取 exe 并清理源码。
4. macOS app 未签名未公证，首次启动需右键打开或去 quarantine（README 已注明）。
