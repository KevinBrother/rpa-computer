# 分层 Computer-use 验证（2026-09-30）

用户已批准：不要把视觉识别错误与原生输入错误合并，先测清晰普通文本，emoji / 易混淆标点单独诊断；旧 0/9 原样保留。本轮使用真实 Claude CLI + GLM。只改验收fixture/离线分析工具，不因测试重构产品输入语义。

## 必须交付

1. WinForms / AppKit 原生靶场具有相同4套独立case：baseline10、known-input10、punctuation6、emoji6，case ID唯一且不回绕。legacy保留历史复现。
2. baseline截图抄写：不给prompt答案/坐标；常用中文、ASCII、数字，无特殊空格或emoji；文字清晰放大，不裁切。随机绿圆点击继续记录。
3. known-input明确给task_payload：原生输入与应用回读比对；包含12/34/38长度、非BMP、全角、NBSP、组合字符、CRLF/Tab。CRLF依据产品契约转换LF，原始payload与expected分开；不做其他Unicode归一化。
4. punctuation/emoji单独执行并统计失败；Windows单色字形的可辨识限制需由截图核实，不能宣称必定对应唯一Unicode。
5. Oracle只由协调者持有，记录suite/case/expected/actual/UTF16hex。工具参数与实际文本逐案例对齐。first-pass与重试分开，同case不算独立新case。
6. 纯测试、macOS真实构建、Windows真实csc构建及无GUIself-test；独立代码审查修复后才能运行新fixture。所有实现/脚本由真实Claude CLI编写。
7. Windows GUI使用已验证Session1 ScheduledTask + Client→TLS→Host；SSH只部署/编排/取证。Mac使用source-free发布目录、严格8工具和sandbox负向probe。Windows缺OS隔离证据，只称诊断。
8. 双平台实际GLM运行四套case，先baseline和known-input，再特殊字符。图像看不清、权限失败、锁屏或应用行为差异要报告，不能改答案或降低校验强度来通过。
9. 策略审计不是任务通过，分析工具不是视觉真值：协调者另外检查关键截图。
10. 完成后按身份核验停止自有fixtures/backdrops/agents/host，不全局kill，不触碰用户Notepad PID24332；保留旧证据/不commit/push。

## 本轮进展

- Mac09:38真实GLM已确认fixture可见，无锁屏，read-only审计17项通过。
- Mac计算器3轮补测独立进行，不替代分层文本案例。
- Windows已核验既有Host/Client SHA与昨日产物一致，09:44在Session1重启TLS Host（动态探测当前IP；不可长期硬编码）。
- fixture及分析器worker运行中，写入范围不重叠。最终通过率必须从实际证据补齐，当前不宣称完成。

## 12:20 CST 进展补记（原 scope 不缩小）

- fixture新Mac49项自检、Windows43项真机自检、33case导出一致，分析器30项回归及独立审查完成。所有修改由真实Claude CLI，coord只任务/审查/验证，无commit/push。
- Windows baseline10/10首次raw exact；known-input9/10（CRLF原生表示与canonical-LF靶场预期不一致，保留旧严格失败、不Normalize actual）；punctuation5/6（模型参数已丢4普通空格，应用忠实输入）；三组final result、17项工具审计、应用oracle/关键截图均已核实。
- Windows仅受控诊断，OS文件隔离欠缺仍保留。前两组背景console露出不足未洗白；新hidden runner副本在punctuation已有实际纯中性截图，且旧stop严格清理契约实测有效。
- Windows emoji正在串行推进；Mac -d主屏置前后真实截图顶栏文字缺失，-e防御性绘制修复离屏通过但实拍遭两屏inactive/Host no_display，Mac所有新suite及计算器补足3次仍未完成，不标整个目标完成。
- 最新结果与完整路径均在 `.agents/reports/layered-gui-validation-20260930.md`；历史旧0/9与本轮所有失败原样保留。

## Mac首次完整文本run已核实，后续未完成

Mac baseline同次真实GLM最终10/10 raw exact、17项策略审计通过；target9/10（首case无HIT，不改写成功）。新-f click-through纯50项及独立复核通过，GUI验收尚未做。Mac known预检又遇no_display，没有GLM输入，不能计通过。Windows emoji接续，其余Mac三suite/Calc3继续待补，整个goal保持active。


## Windows四suite执行完成；Mac剩余保留（2026-09-30 13:33:23 CST）

Windows emoji同次真实GLM正常结束，首次/最终raw exact5/6，HIT6/6，17项策略审计通过；唯一失败模型把U+2705抄成U+2611、应用actual与payload一致，协调者实看失败图。完整证据已归档、本次owned fixture/backdrop/agent/TLS Host已严格清理，8399无监听，用户Notepad24332仍在。Windows全部suite已执行，不是全部通过，旧0/9/各失败/OS隔离缺项保留。

Mac最新g/h只读显示两屏online但inactive/asleep、活动屏0，权限预检仍true；没有新输入。-f fresh自检50/50与SHA核验完成但不冒充GUI验收；known-input10、punctuation6、emoji6与Calculator完整3次仍待用户手动亮屏后继续。不得以Windows完成缩小目标/标整体验收完成。
