# CC+GLM：Mac旧文本门禁补测（唯一GUI执行槽）

你是测试编排与验收执行者CC+GLM。唯一真实桌面GUI槽分配给本任务，其他任务无GUI。工作目录本项目，不修改产品源码/测试/脚本，不commit/push、不改全局模型/TCC/电源/登录设置。只有本任务可以启动自有fixture/backdrop并运行真实黑盒Claude CLI+GLM。所有点击/键盘只能由**隔离的内层CC+GLM通过8个computer MCP工具**执行；你不得用shell/AppleScript/AX/CDP/直接native事件操作桌面。可执行已有helper/script和安全的编排shell命令，不写新产品脚本。不SSH，不操作其它用户应用，不全局kill。

读取：.agents/reports/layered-mac-firstmouse-20260930.md、layered-gui-helpers-20260930.md、layered-gui-validation-20260930.md（相关Mac段）、complete-actions-environment-preflight-cc-20260930.md；scripts/run-blackbox-claude.sh与package-release.sh用法。

## 当前产物与安全边界

- 原子多击Host已通过CC纯测试/编译，但新的Host反馈源代码正在别的agent实施。**本任务不要cargo build/改源码**；直接把已有target/release/computer-host复制到全新source-free临时发布目录并记录hash，在整轮内固定该副本，不受后续构建影响。不要复用/覆盖旧release/evidence；正确配置mcp/computer.json只指向新副本（参考旧/private/tmp/computer-native-mac-20260929目录结构）。不要复制源码/CLAUDE.md/回答数据。
- 已修first-mouse且纯测试50/50、尚未GUI验收的-f fixture固定产物：`/private/tmp/computer-layered-mac-fixture-20260930-f/ComputerUseAcceptance.app`。先核对主exe SHA256应为5823306b714e00919157a70a8a53389dd57f2ef399d4a6c07398e45d63928099。不从正在修改的fixture源码重新build。可复制这个已编译app到全新自己的临时目录再启动。
- 中性backdrop：`.agents/runs/layered-gui-helpers-20260930/macos-backdrop`，已有helper支持--fixture-pid和--max-minutes 25（先读已有源/报告确认实际参数，不猜）。先复制编译产物至自有临时目录，不写/改源码。只能覆盖背景并一次activate自有fixture，不能关闭/最小化其他应用。
- 新的run/nonce/seed与oracle日志放coordinator-only目录，**不放黑盒Agent发布目录**、不向内层prompt透露oracle答案（known-input有意给固定payload除外）。不得覆盖旧-e2失败、旧-f准备run。
- 当前Mac探针2屏active但可能后来改变。先只读probe传**唯一新输出路径**，active0则不输入、不唤醒。GUI agent首张截图必须确认锁屏/登录/权限窗口不存在且目标fixture在；不符关闭会话并blocked，不能盲打。

## 本次只跑2个suite，串行不重试洗白

1. baseline10：真实验证-f首次点击是否HIT。启动自有fixture --suite baseline --seed <新seed> --evidence-file <外部oracle路径> --coordinator-raise（先核对旧-f已有参数），和25min/backdrop；记录实际PID/可执行路径/start身份。内层prompt采用`.agents/tasks/layered-baseline-20260930.md`原任务内容复制到新发布目录task，不能加答案/坐标。运行已有scripts/run-blackbox-claude.sh --release-dir <new> --task <release task> --source-dir <canonical repo> --max-turns 100 --max-seconds 1200 --print，不使用--no-sandbox/diagnostics逃过隔离。正向/负向源读取隔离probe必须实际成功才运行。模型固定现有sonnet->GLM，不改全局。
2. 在baseline终态、证据齐且所有owned GUI安全退出后，再新run执行known-input10，任务采用 .agents/tasks/layered-known-input-20260930.md（有意payload输入测试，不能改成OCR），相同隔离/一次输入一次Check不重试约束。

每suite：运行已有transcript审计，核对init工具仅8个computer工具、actual assistant model为glm-5.3-flash、调用参数/图像；终态后再用既有scripts/analyze-layered-gui.py（先看--help用法）关联oracle/模型payload/图片，不写新analyzer。必要短只读取证/图片提取命令可以，不改源。按caseID+nonce记录首次HIT/WRONG/text_check；rawpayload和rawactual对比，partial不当完整10/10。若模型无nonce报告与oracle配对分开说明。

异常/超时不自动重跑，保留失败。不要手工点击修正结果。若sandbox/权限/显示阻塞，报告具体证据并安全清理，不放宽策略。

清理：每suite之后仅按本任务记录的PID+exe/start身份终止自己的fixture/backdrop/owned runner，不能影响实现/其它CC进程。Host MCP应由内层computer_close+进程EOF清理，确认无自有遗留，未知如实报，不盲kill同名全局进程。

最终报告 `.agents/reports/macos-layered-regression-cc-20260930.md`，自身编排日志`.agents/runs/macos-layered-regression-cc-*`，保留所有release evidence/oracle。报告2suite分开统计、精确产物hash/工具隔离/实际model/失败样本、未完成项（punctuation/emoji/calculator仍后续）。不要宣称整个项目完成。允许通过已有run-blackbox脚本启动最多两条**串行内层CC**，这是唯一允许的嵌套测试agent，不启动其它委派。
