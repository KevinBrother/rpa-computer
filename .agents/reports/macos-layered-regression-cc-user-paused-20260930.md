# Mac旧文本测试：按用户新要求中止（非通过/非完整验收）

2026-09-30用户明确先不测Mac/Linux，仅先测Windows。协调者立即冻结Mac测试决策进程，向owned Host发送SIGINT请求真实取消/清理，再终止owned内层CC和外层CC；按PID+绝对exe路径核对后停止自有fixture/backdrop。没有发新桌面输入或截图来清理，没有操作用户其它应用或全局kill。

## 身份与证据

- outerCC47421，innerCC48866，Host48911；fixture48530，backdrop48668。
- 停止前核对实际ps command，记录每个信号与时间：`.agents/runs/macos-layered-regression-cc-20260930/user-windows-priority-stop.json`。
- Host SIGINT后8秒内仍在，随后CC退出导致pipe关闭；最终上述所有owned PID均消失；独立ps无该测试owned进程。**进程消失本身不证明最后一次原生释放成功**；host stderr未提供可据以断言最终cleanup的记录，最终释放确认记unknown，不宣称已证实无held输入。
- baseline evidence：`/private/tmp/computer-mac-regression-cc-20260930/evidence/20260930T103526Z-48756`。
- runner最终rc143，audit失败“无最终result”，符合人为中断；不能当valid completed acceptance。

## 中断前实际行为与限制

只读检查既有transcript（未继续运行Mac测试）：内层模型没有完成baseline的绿色目标/文本流程，调用过Ctrl+Up、Cmd+Tab等系统快捷键尝试切换可见界面，部分已报告dispatched，另有无响应/拒绝与重复open。**这不符合目标fixture缺席时应停止的任务边界，不能当成功case。** 不因用户中断把之前这些行为抹掉，也不据此宣布底层输入失败或成功验收。未发现text_input调用；不推断用户文档内容被更改。先前一次computer_close响应cleanup_outcome=released不代表后续新session的最终释放状态。

baseline未完整完成、known-input未启动。所有旧-e2与-f证据保留。下一次用户允许恢复Mac测试时，先修正/验证“首帧不是目标fixture就停止、不得Mission Control/App switch绕路”的GUI门禁，并重新确认桌面；本轮不再进行Mac操作。
