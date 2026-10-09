# Gesture analyzer 测试 helper 最小修复

日期：2026-09-30。
状态：**根因修复已写入并冻结；等待真实 CC + GLM 在 Windows 重跑，不声明测试通过。**

## CC 的真实 red 证据

- 本地转录：`.agents/runs/gesture-fixture-windows-cc-20260930.jsonl`。
- Windows 固定快照日志：`C:\Temp\computer-gesture-cc-20260930-185517\evidence\python-analyzer-test-2.log`。
- CC 执行结果：Ran 26，FAILED，errors=24；错误为
  `TypeError: record() got multiple values for argument 'kind'`。
- 这些错误发生于测试数据构造，尚未进入对应分析器断言；不能据此评价那些断言的通过情况。
- 本轮仅读取本地转录与当前测试源码，没有连接或修改 Windows 固定快照。

## 根因与唯一源码修改

文件：`tests/gesture_gui_analyzer.py`。

```diff
-def record(kind, t, **kw):
-    r = dict(type=kind, ts=t, run_id="run", session_id="session", suite="multiclick",
+def record(record_type, t, **kw):
+    r = dict(type=record_type, ts=t, run_id="run", session_id="session", suite="multiclick",
```

第一位置参数代表记录类型（例如`input_event`）；`**kw`中的`kind`代表原生事件种类（例如`down`）。
原先两个不同概念使用同一参数名，调用绑定时冲突。重命名后，记录类型仍写入`type`，
事件的`kind`仍由原有`r.update(kw)`写入，数据结构与既有调用意图不变。

**未改动任何case、调用、断言、产品分析器或fixture其它源码。**
本轮另外只新增本报告。未扩展修复范围；未调查或声称排除了其它潜在问题。

## 执行边界及待 CC 工作

- 没有执行测试、self-test、parity、验收或GUI操作；没有SSH、commit/push。
- 本轮未执行py_compile；此前语法编译也不能识别这种运行时参数绑定错误，不能替代CC重跑。
- 已回读修改后的helper，源码仅上述两行替换；当前冻结，不继续修改。
- 保留旧远端snapshot及red日志。协调者将修复后的测试文件放入新的Windows验证快照，
  保持产品分析器、fixture和case/断言不变，由真实CC+GLM执行：

```powershell
python tests/gesture_gui_analyzer.py
```

记录实际新日志/退出码。若随后进入断言产生新失败，应作为独立缺陷报告，不扩大本次helper修复。
Mac/Linux测试仍不安排。
