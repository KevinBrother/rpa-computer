# README closeout 2 — 文档事实性修正（2026-09-24）

范围：仅 README.md 与本文档。未改实现/脚本，未启动任何进程/GUI/远程操作，未 commit/push。

## 实际修改（3 处，全部为事实性修正）

1. **沙箱/发布目录段落（"本地 MCP 接入"节末尾）**
   - 原文错误声称沙箱拒绝"print 验收路径下的临时根目录"，且"发布目录永远不应位于
     /tmp 下；位于则 fail-closed"。
   - 静态核对 `scripts/run-blackbox-claude.sh`（拒绝逻辑在 release 目录与源码树重叠、
     release 目录为 `/`/`/tmp`/`/private/tmp`/`$TMPDIR`/`$HOME` **根本身**、或目录名匹配
     `rpa-computer-*`/`rpa-core-*` 时 fail-closed；sandbox profile 仅 deny 源码树、
     已知源码副本如 /tmp/rpa-core-shim、各临时根下 rpa-* 形状的暂存子树）与
     `scripts/package-release.sh`（默认发布目录就是 `$TMPDIR/computer-release.<uuid>-<pid>`），
     确认**临时根下安全命名的发布目录是支持的默认方式**。
   - 已改写为该真实策略，并保留 GUI 通道不受 sandbox-exec 约束的"隔离边界"段落（未动）。

2. **"当前状态"节**
   - 明确实现仍在验证中，不作最终通过声明。
   - 只读预检描述修正为实际 **5 次调用**（describe/open/observe×2/close，两次观察均
     `0x80070005` 拒绝访问），**零截图、零键鼠输入**；与
     `.agents/reports/blackbox-readonly-preflight-1.md`（"8tools/5calls/6turns"）一致。
   - 增补 release 协议回归当前 **49 通过 1 失败**（observe+取消竞态），未达发布就绪；
     链接 `.agents/reviews/continuation-review-4.md`。
   - 保留：GUI 成功 0 次、每任务每平台 10 次试验门禁待 Windows 解锁。

3. **Windows 远程测试节的 ssh 示例**
   - `ssh acer-win powershell -File C:\rpa-computer-test\...` 改为整体单引号
     `ssh acer-win 'powershell -File C:\rpa-computer-test\windows-start-host.ps1'`，
     防止本地 shell 吞掉反斜杠。仅修正文档引号，未执行。

## 核对确认无需改动的点

- "本地 MCP 接入"代码块中的打包示例已使用安全临时路径
  （`$TMPDIR/computer-release.<uuid>-<pid>`），并注明禁止 `rpa-computer-*/rpa-core-*`
  目录名与稳定位置选项；非源码树兄弟目录，无 rpa-* 基名，符合要求。
- 转录审计、Windows 启停脚本 fail-closed 描述与脚本/评审记录一致，未改动。

## 静态验证

- 仅依据对 `scripts/run-blackbox-claude.sh`、`scripts/package-release.sh` 相关行的
  只读核对（deny 清单、拒绝条件、默认发布目录形状）与两份引用报告
  （blackbox-readonly-preflight-1.md、continuation-review-4.md）的事实描述。
- 未运行任何测试或脚本；README 为纯 Markdown，无构建影响。
