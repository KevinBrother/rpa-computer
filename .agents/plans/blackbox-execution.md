# 黑盒验收执行顺序（协调者，2026-09-24）

本计划不替代 implementation.md 的门禁；不降低每任务每平台10次、至少8次真实成功的要求。

## 分层证据，不能混用
1. 编译/单元/协议/失败注入：真实执行相应测试，但mock结果只证明逻辑。
2. Native只读诊断：同一桌面会话的Host通过最终传输抓到可见桌面；截图由协调者查看。旧exe诊断明确标旧，不作发布验收。
3. Agent隔离：源码之外的发布目录；真实CLI当前用户模型；八个computer MCP工具；同一OS sandbox profile做negative probe和实际运行；日志审计init.tools及实际tool_use。
4. 视觉判断：新的随机nonce/布局仅出现在屏幕；Agent prompt无nonce、坐标或oracle路径；协调者对照oracle和实际截图。
5. GUI动作：Agent看图点击绿圆、输入屏上Unicode样例并检查MATCHED；另通过系统计算器实际输入10*20并观察结果。仅回答200不计成功。

## 必要前置
- Windows Host交互式计划任务进入当前explorer Session；127.0.0.1监听OwnedPID；token限制ACL；SSH本机loopback转发；不改防火墙、不使用RPAD/Executor。
- macOS必须用户解锁；读到锁屏就停止，不输密码、不尝试旁路。
- 控制桌面不展示源码/终端/私有oracle；OS文件沙箱不能阻止代理通过GUI导航他处，残余风险保留报告。
- 产品/脚本冻结后记录源码清单哈希、最终二进制哈希、release manifest、任务/会话身份和模型CLI版本。
- 远端部署校验必须重新计算远端payload文件哈希，上传的manifest不是远端字节证明。

## 分阶段运行
A. 单次只读图片试验（open→observe→报告画面→close），先确认模型确实接收image块。若模型不支持/拒绝图像，明确记录真实模型限制，不更换配置冒充通过。
B. 单次交互试验，再核对原生oracle事件和Agent工具轨迹。失败先定位图片/坐标/聚焦/输入哪个边界，不能以重复重试掩盖系统问题。
C. 批量验收：每个trial仍有独立session/request/observation记录。图像/靶点/Unicode使用Next trial随机刷新；计算器每次清空后实际重输，不复用初次答案。每任务每平台N=10，逐次记录pass/fail，不只报告百分比。
D. 取消/断线/清理：危险输入不在锁屏执行；使用受控fixture或确定性失败注入分层验证，实际输入停止与测试级注入证据分别标注。不得声称mock证明OS失效场景。
E. 只清理当前run完整身份匹配的task/PID，先验证所有身份再修改；保留证据目录。失败也需收尾，不杀同名他人进程。

## 结果台账（待真实运行填写）
| 平台 | 图片识别 | 绿圆点击 | 计算器 | Unicode输入 | 原始证据 |
|---|---|---|---|---|---|
| macOS | 未运行 | 未运行 | 未运行 | 未运行 | 当前锁屏 |
| Windows | 未运行 | 未运行 | 未运行 | 未运行 | 只有fixture进程/Session证据 |

不得在未满足的行填PASS。源隔离验证失败或工具清单违规的run无效，不能算任务成功次数。
