独立审查刚完成的两个独立scope最小修复，不GUI、禁止改代码/测试、不delegate、不commit，写 .agents/reviews/layered-fixture-helper-livegate-review-20260930.md。
1. Mac main.swift ShapesView clipsToBounds/draw bounds clip + 3offscreen self-tests，-e self-test49通过，但实拍因display inactive/no_display未进行。不把防御性回归当真机root cause已确认。保留主屏定位、raiseREADY、安全flag、UTF16exact、caseIDheader可读。特别检查header字符串加入caseID后的宽度500是否截断suite；需要完整case位置/ID/nonce/target可读，给具体字体测量即可不真实window。
2. 新独立helpers .agents/runs/layered-gui-helpers-20260930/*，duration默认25、1..30 failclosed，fixture身份验证/无input/只自有background，Mac绝对frame副屏双原点最小修复，不重复激活。Windowscoord已实际PSParser ParseFile成功且新helper+agent进程在Session1；不是完整GUI已通过。检查25min后台覆盖1200s+setup，定时watchdog保留，拒绝修改权限系统设置。
原文件旧helpers不改变，旧失败保留。selftest30静态/参数 checks及Mac49已coord重跑。boundedreview只列真实阻塞与建议，不为了审查制造重构，不说全套完成。
