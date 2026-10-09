# CC+GLM：双平台只读验收环境预检

你是测试执行者，只在 /Volumes/doc/workspace/datagrand/rpa/rpa-computer 工作。最新角色：Codex+gpt-6.1-sol写源码，CC+GLM执行验证。你只执行本任务的只读预检，不写产品源码/测试/脚本，不启动桌面应用，不输入/点击、不截图、不SSH启动服务，不改系统/TCC/全局Claude配置，不commit/push，不触碰用户Notepad。SSH仅只读环境检查。

目标：给后续真实GUI验收明确环境证据，不沿用旧“Mac已休眠”判断。
1. 阅读 .agents/runs/native-mac-display-probe-20260929.py，确认只读后运行，**传新的唯一输出路径** .agents/runs/complete-actions-mac-display-preflight-cc-20260930-<时间>.json，不覆盖默认旧输出。记录在线/活动/休眠屏数量；不解锁/唤醒。
2. ssh acer-win 查询实际hostname/IPv4、交互用户explorer.exe的SessionId、是否存在已知用户Notepad PID24332（查身份/SessionId不读取文档）、现有computer-host/fixture相关进程身份及监听8399/其它旧测试端口，仅列必要元数据，不读命令行凭据。验证现有Windows Framework64 csc.exe/PowerShell版本，确认将来GUI必须交互式ScheduledTask且非SSH服务会话。不要创建任何任务/启动任何应用。
3. 本机只读确认cargo-xwin/rust target工具可用，找旧Windows交叉构建成功日志命令以供下一任务复用（不构建、不安装）。
4. 列出可用预检/待环境项，不把“进程在交互会话”当桌面可点击/已解锁的证明。

只可写自己报告 .agents/reports/complete-actions-environment-preflight-cc-20260930.md 及专用runs证据，所有其它源文件只读。报告精确命令、日期、结果、下一步建议与限制，不含tokens/credential/用户文本。不执行任何GUI工具。本任务允许Bash中只读命令与已有probe，不编写新脚本；必要短只读Python/PowerShell表达式可以。
