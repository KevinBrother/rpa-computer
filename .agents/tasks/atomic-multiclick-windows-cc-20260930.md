# CC+GLM：Windows真实构建及无GUI原生测试

你是测试执行者，源码已由Codex+gpt-6.1-sol冻结。只能写本任务专用报告/runs、构建产物和Windows专属临时目录；不修任何产品源码/测试/脚本，不commit/push，不改设置，不启动其他agent。其它CC正在跑Mac/root纯测试，共享Cargo锁可等待，不终止别人的cargo。

读取 .agents/reports/atomic-multiclick-sol-20260930.md、.agents/reports/complete-actions-environment-preflight-cc-20260930.md、docs/remote-connection.md跨编译命令。

任务：
1. 审查 native crate Windows tests，确认非ignored用例不SendInput/不启动GUI/不截图，只纯测试。若存在GUI注入普通测试则停止，交回协调者。
2. 用本机现有cargo-xwin/rust-lld环境（docs/remote-connection.md原命令，不安装新工具）实际编译 native crate Windows unit test exe（--no-run），并构建root Windows release --bins。记录精确命令/exit和完整日志；失败不自行修改源码，报告。
3. 测试exe路径必须从本次cargo JSON compiler-artifact确定，不从目录随机选旧exe。计算sha256，SSH acer-win查询TEMP然后建立**新唯一**computer-multiclick-cc-20260930-<时间>目录，scp仅本次测试exe和必要已有产物。核对远端SHA256一致。以SSH直接执行该无GUI纯测试exe，**绝不加--ignored/--include-ignored**；保存stdout/exit与pass/ignored数。这是纯测试，不需要创建交互ScheduledTask。
4. 不在远端执行computer-host/fixture，不碰用户Notepad24332，不创建/停止服务/计划任务。不要启动任何GUI操作。
5. 报告 `.agents/reports/atomic-multiclick-windows-cc-20260930.md`：构建、hash、目标exe、Windows真实运行精确计数，任何失败保留、GUI未验收声明。专用runs `atomic-multiclick-windows-cc-*`。不要用“交叉编译成功”代替“目标机运行成功”。
