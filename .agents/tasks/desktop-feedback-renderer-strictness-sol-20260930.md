# Renderer协议一致性修复（先测试源码、CC red，再实现）

开发Codex+gpt-6.1-sol；测试执行CC+GLM。直接编辑本项目。独占写集仅 desktop-feedback/{macos,windows}/{JSONValue或JsonValue,Protocol,SelfTests} 对应.swift/.cs文件及自身报告 .agents/reports/desktop-feedback-renderer-strictness-sol-20260930.md。不碰其它目录；原renderer作者已转入新macos-capturecrate，不再写renderer。Host owner和GUI旧文本任务并行，禁止真实GUI/截图/SSH/Host运行。不要worktree/commit/push，不执行测试，不启动其它agent；必要编译检查允许。

## 证据与需求

CC报告 .agents/reports/desktop-feedback-renderer-cc-verification-20260930.md 的43..49行把3项宽容视为可接受。协调者不同意作为最终strict v1交付：两个renderer与canonical Rust protocol必须一致，特别非法JSON前导零不是可接受兼容需求。不要修改原报告/抹旧69/67checks证据。

对照 crates/desktop-feedback/src/protocol.rs / codec.rs：
- session.id、surface.id 1..128 bytes ASCII `[A-Za-z0-9_.:-]`。
- surface.version 1..128 bytes ASCII `[A-Za-z0-9_.:,-]`，**必须保留逗号，真实Geometry.version可用**。
- pointer非null必须有surface同帧；坐标finite且surface有效（已有的正确逻辑保留）。
- JSON数字词法必须合法：不能接受01/-01/00.1等；u64字段精确，超过MAX/非整数/负数拒绝。坐标正常小数/科学记数法应合法，不把JSON语法收紧成只能整数。

## 分两段

A. 先检查现有解析器，编写两平台的针对性负例与合法边界/真实version正例测试源码，只改SelfTests（可加独立测试文件但更新build脚本须先报告）。不改解析器/Protocol实现，不执行测试。编译检查后报告 TESTS_READY_FOR_CC_RED + 精确命令和新增case名称；**暂停此任务等待协调者CC实际red结果**，不要自行接着修实现。
B. 收到CC red证据及GO后，做最小正确解析/validator修复，复核双方行为一致、无需改Host协议，无注入/库依赖。编译检查，报告READY_FOR_CC_RETEST，CC执行green。保留所有旧UI/状态/整数/16KiB测试，不弱化断言。

没有真实GUI验证就不报告GUI已通过。最终列精确文件/源码变更/编译结果/CC待测指令。
