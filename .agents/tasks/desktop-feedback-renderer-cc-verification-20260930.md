# CC+GLM：默认桌面反馈renderer双平台构建/纯测试

只审查、构建、执行明确无GUI self-test；不写/修源码/测试/脚本。源码由Codex+gpt-6.1-sol冻结，后续Mac capture库另一个写集不在本任务。
工作目录本项目，不worktree/commit/push，不启动agent，不改设置/TCC。只能写本任务report/runs、构建产物、Windows自有唯一临时目录。

1. 读 .agents/reports/desktop-feedback-renderer-sol-20260930.md 与 desktop-feedback/README.md、v1-wire契约。先确认Swift main与C# Program的--self-test确实在任何AppKit/WinForms窗口/DPI初始化前返回，不创建窗口/不输入/不截图。若不是，停止交回实现者。
2. 独立规格与质量审查：私有NDJSON字段/duplicate/版本/整数范围/16KiB、真实Geometry.version逗号、初始无session状态、旧sequence、stop当前代次、清理失败文案、装饰与Stop命中分离、异步有界read/write、ready只真实GUI初始化后。Mac unsupported与Windows requested不能报GUI验收成功。对照Rust crates/desktop-feedback真实协议相互一致性，数字与长度/错误等边界特别检查。只读，不自修。
3. Mac：以新的唯一产物路径传给 ./desktop-feedback/build-macos.sh（新建专属runs子目录），实际build；运行该产物 --self-test，保存exit与stdout/stderr，不加其它参数、不启动GUI。
4. Windows：ssh acer-win只读查TEMP，创建全新computer-feedback-cc-20260930-<timestamp>临时目录；scp desktop-feedback/windows源码/manifest/config及已有build-windows.ps1（不要上传build产物目录/缓存）。运行已存在Framework64 v4 csc的build-windows.ps1，保留日志。然后仅 --self-test，WinExe明确等待并收集exit/stdout/stderr（Start-Process -Wait -PassThru重定向用于**无GUI self-test**，绝不启动正常UI）。不要操作用户Notepad24332、不要创建计划任务、不要启动Host或renderer常规UI。
5. 失败如实保留并报告；不改代码/阈值让测试通过。无真实GUI就不能声称click-through/no-focus/Stop可达/截图排除已验收。

报告 .agents/reports/desktop-feedback-renderer-cc-verification-20260930.md，专用logs .agents/runs/desktop-feedback-renderer-cc-*（可新目录），精确列平台编译与self-test计数/退出码、源manifest/hash、CC实际目标机路径、待验收项。
