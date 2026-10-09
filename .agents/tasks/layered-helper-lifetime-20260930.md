继续已批准的分层GUI验收，修复测试helper的生命周期/副屏遮挡。只写全新 .agents/runs/layered-gui-helpers-20260930/{macos-backdrop.swift,macos-backdrop,gui-neutral-desktop-20260930.ps1,helper-selftest.py(如需)} 及 .agents/reports/layered-gui-helpers-20260930.md。不写其他文件，保留旧helper/旧失败证据。原文件 .agents/runs/native-gui-helpers-20260929/macos-backdrop.swift 和 .agents/runs/gui-neutral-desktop-20260928.ps1。

原因：原Windows backdrop timer硬编码5分钟（虽Task expiry25min）；GLM完整baseline15min，后半暴露用户terminal而非neutral，不可说整段全中性隔离。Mac旧helper max20min==agent max1200s还没算预检setup，可能提前消失；Mac readonlyprobe .agents/runs/layered-mac-baseline-d-visibility-probe-20260930.json 显示secondary bounds x3840而实际secondaryframe x1920，NSWindow(contentRect:screen.frame,screen:screen)可能双重屏幕origin。

设计已定：两端支持明确 --max-minutes/-MaxMinutes，默认25、允许1..30，超范围fail而不是clamp；timer跟参数、fixture-watchdog保留，Windows task另有30min安全expiry。Mac指定屏幕的NSWindow用局部content rect且setFrame到绝对screen.frame，确保每屏中性背景实际覆盖其frame；Windows仍只覆盖primary（原契约），不用改用户屏幕。single fixture activation best-effort，不反复抢焦点，不点击/按键、不AX/AppleScript、不改系统权限。不作topmost/窗口浮动扩展，只操作自己背景及激活匹配自有fixture。

Mac编译，args/geometry纯回归先fail后pass，不创建真实window、不GUI/不信号/不SSH。Windows tokenizer结构check复用 .agents/runs/native-gui-helpers-20260929/ps-structural-check.py；不假装真实pwsh验证。回归覆盖25min配置/拒绝0,31/secondaryorigin1920保留。不用长篇设计，不delegate、不commit/push。报告不能宣称真桌面已验证，由coord拍图。
